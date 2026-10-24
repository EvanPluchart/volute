//! The audio thread: owns the `Audio` backend (COM lives here only), applies the UI's commands,
//! turns Core Audio's events into model updates, and sends frames back.
//!
//! Nothing polls. Hidden, the thread sleeps on its channel; an event wakes it, it updates its
//! model and, at most, tells the tray that the master mute changed. Shown, it also reads peaks at
//! 30 Hz for the meters, and loads app names and icons the first time they are needed.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use volute_audio::{
    Audio, AudioEvent, Device, DeviceId, Flow, MeterId, Role, Session, SessionState, Target,
    default_device,
};
use volute_core::activity::Activity;
use volute_core::apps::{self, App, AppKey};
use volute_core::curve;
use volute_core::memory::{Memory, Remembered};

const METER_EVERY: Duration = Duration::from_millis(33);
/// How much of the last level a meter keeps each frame: fast attack, slow release.
const RELEASE: f32 = 0.82;
/// A tray wheel notch moves the master by this much.
pub const MASTER_STEP: f32 = 0.02;

/// What the UI asks of the audio thread.
#[derive(Debug)]
pub enum Command {
    /// The mixer was shown (meters run) or hidden (nothing runs).
    Visible(bool),
    /// A slider moved: the app's volume at this slider position.
    SetVolume { key: String, position: f32 },
    SetMute { key: String, muted: bool },
    /// A device's volume (its endpoint scalar, already tapered by Windows) or mute.
    SetDeviceVolume { id: String, volume: f32 },
    SetDeviceMute { id: String, muted: bool },
    /// Makes this output the default for every role.
    SetDefault { id: String },
    /// Moves the master output by `steps` notches (the tray's wheel).
    // Sent by the tray's wheel, wired with the tray in the next step of M1.
    #[allow(dead_code)]
    StepMaster(i32),
    /// Stop now. Not waiting for every sender to drop, which a callback holding one would
    /// prevent (decision 0004).
    Quit,
}

/// Everything that reaches the audio thread, from the UI or from Core Audio's callbacks.
#[derive(Debug)]
pub enum Input {
    Ui(Command),
    Audio(AudioEvent),
}

/// One row of the mixer, as plain data the UI thread turns into its model.
#[derive(Clone, Debug, PartialEq)]
pub struct RowData {
    pub key: String,
    pub name: String,
    pub system: bool,
    /// Slider position, 0.0 to 1.0 (the perceptual curve already applied).
    pub volume: f32,
    pub muted: bool,
    pub playing: bool,
    /// Index into the icons sent so far, or -1.
    pub icon: i32,
}

/// A device, as plain data.
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceData {
    pub id: String,
    pub name: String,
    pub output: bool,
    /// The endpoint scalar, shown as is: Windows tapers it already.
    pub volume: f32,
    pub muted: bool,
    pub default: bool,
}

/// An app icon, sent once; rows refer to it by `id`.
#[derive(Clone, Debug, PartialEq)]
pub struct IconData {
    pub id: i32,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// What the tray shows: the master output's volume and mute.
#[derive(Clone, Debug, PartialEq)]
pub struct TrayData {
    pub device: String,
    /// Percent, 0 to 100.
    pub volume: u32,
    pub muted: bool,
}

/// What the audio thread sends the UI thread. Each part is there only when it changed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Frame {
    pub rows: Option<Vec<RowData>>,
    pub devices: Option<Vec<DeviceData>>,
    /// One level per row, in the order of the latest rows; empty while hidden.
    pub levels: Vec<f32>,
    pub master_level: f32,
    pub mic_level: f32,
    pub icons: Vec<IconData>,
    /// The error to show: empty clears it.
    pub error: Option<String>,
    pub tray: Option<TrayData>,
}

/// Held by a frame on its way to the UI thread: at most one is in flight, and it frees the way
/// for the next when dropped (applied, or dropped with an event loop that ended).
pub struct InFlight(Arc<AtomicBool>);

impl Drop for InFlight {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

pub type Backend = Box<dyn Audio>;
/// Builds the backend on the audio thread, where COM objects must be created.
pub type MakeBackend = Box<dyn FnOnce() -> volute_audio::Result<Backend> + Send>;
/// Writes the remembered volumes somewhere (the settings folder).
pub type SaveMemory = Box<dyn Fn(&Memory) + Send>;
/// Hands a frame to the UI thread; `false` when it is gone.
pub type Deliver = Box<dyn Fn(Frame, InFlight) -> bool + Send>;

/// Names and icons of executables, read through volute-shell (the OS stays out of this crate).
pub trait Looks: Send {
    /// A readable name and an icon of `size` physical pixels.
    fn look(&mut self, exe: &std::path::Path, size: u32) -> (Option<String>, Option<volute_shell::Icon>);
}

/// The real one: the executable's own resources.
pub struct ExeLooks;

impl Looks for ExeLooks {
    fn look(&mut self, exe: &std::path::Path, size: u32) -> (Option<String>, Option<volute_shell::Icon>) {
        let info = volute_shell::exe_info(exe, size);
        (info.description, info.icon)
    }
}

/// None, for tests and the mock: names from file names, no icons.
pub struct NoLooks;

impl Looks for NoLooks {
    fn look(&mut self, _: &std::path::Path, _: u32) -> (Option<String>, Option<volute_shell::Icon>) {
        (None, None)
    }
}

pub struct Options {
    /// App icons' side in physical pixels.
    pub icon_size: u32,
    /// The remembered volumes, and whether to save them (`None`: neither read nor written).
    pub memory: Option<Memory>,
    /// Where remembered volumes are saved; `None` saves nothing.
    pub save: Option<SaveMemory>,
}

pub fn spawn(
    make: MakeBackend,
    looks: Box<dyn Looks>,
    options: Options,
    deliver: Deliver,
    inputs: (flume::Sender<Input>, flume::Receiver<Input>),
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("volute-audio".into())
        .spawn(move || {
            let (sender, receiver) = inputs;
            let mut mixer = Mixer::new(make, looks, options, deliver, sender);
            mixer.run(&receiver);
            mixer.save_memory();
        })
        .expect("spawning the audio thread")
}

struct Mixer {
    audio: Option<Backend>,
    looks: Box<dyn Looks>,
    icon_size: u32,
    deliver: Deliver,
    in_flight: Arc<AtomicBool>,
    started: Instant,
    visible: bool,
    devices: Vec<Device>,
    /// The default output, whose sessions are the rows, and the default input.
    master: Option<DeviceId>,
    mic: Option<DeviceId>,
    sessions: Vec<Session>,
    apps: Vec<App>,
    /// Indices into `apps` of the rows shown.
    shown: Vec<usize>,
    activity: Activity,
    memory: Option<Memory>,
    memory_dirty: bool,
    save: Option<SaveMemory>,
    /// Per executable (lower case): its readable name and icon id, once looked up.
    looked: HashMap<String, (Option<String>, i32)>,
    next_icon: i32,
    pending_icons: Vec<IconData>,
    meters: Vec<MeterId>,
    owners: Vec<usize>,
    peaks: Vec<f32>,
    levels: Vec<f32>,
    master_level: f32,
    mic_level: f32,
    rows_dirty: bool,
    devices_dirty: bool,
    tray: Option<TrayData>,
    tray_dirty: bool,
    error: String,
    error_dirty: bool,
    next_meter: Instant,
    next_expiry: Option<Duration>,
}

impl Mixer {
    fn new(make: MakeBackend, looks: Box<dyn Looks>, options: Options, deliver: Deliver, sender: flume::Sender<Input>) -> Mixer {
        let (mut audio, mut error) = match make() {
            Ok(audio) => (Some(audio), String::new()),
            Err(err) => (None, err.to_string()),
        };
        if let Some(backend) = audio.as_deref_mut() {
            // Callbacks only queue the event for this thread.
            let sink = Arc::new(move |event| {
                let _ = sender.send(Input::Audio(event));
            });
            if let Err(err) = backend.listen(sink) {
                error = err.to_string();
            }
        }
        let now = Instant::now();
        let mut mixer = Mixer {
            audio,
            looks,
            icon_size: options.icon_size,
            deliver,
            in_flight: Arc::new(AtomicBool::new(false)),
            started: now,
            visible: false,
            devices: Vec::new(),
            master: None,
            mic: None,
            sessions: Vec::new(),
            apps: Vec::new(),
            shown: Vec::new(),
            activity: Activity::default(),
            memory: options.memory,
            memory_dirty: false,
            save: options.save,
            looked: HashMap::new(),
            next_icon: 0,
            pending_icons: Vec::new(),
            meters: Vec::new(),
            owners: Vec::new(),
            peaks: Vec::new(),
            levels: Vec::new(),
            master_level: 0.0,
            mic_level: 0.0,
            rows_dirty: true,
            devices_dirty: true,
            tray: None,
            tray_dirty: false,
            error_dirty: !error.is_empty(),
            error,
            next_meter: now,
            next_expiry: None,
        };
        // Read once at start, so the tray knows the master's state while hidden.
        mixer.read_devices();
        mixer
    }

    fn now(&self) -> Duration {
        self.started.elapsed()
    }

    fn run(&mut self, inputs: &flume::Receiver<Input>) {
        let mut volumes: Vec<(String, f32)> = Vec::new();
        self.send_tray();
        loop {
            let received = if self.visible {
                inputs.recv_deadline(self.next_meter)
            } else {
                // Hidden: sleep until something happens. Nothing ticks.
                inputs.recv().map_err(|_| flume::RecvTimeoutError::Disconnected)
            };
            match received {
                Ok(input) => {
                    // Take everything waiting, so a fast drag applies its last position once.
                    let mut next = Some(input);
                    while let Some(input) = next.take().or_else(|| inputs.try_recv().ok()) {
                        match input {
                            Input::Ui(Command::Quit) => return,
                            Input::Ui(Command::SetVolume { key, position }) => {
                                match volumes.iter_mut().find(|(k, _)| *k == key) {
                                    Some(pending) => pending.1 = position,
                                    None => volumes.push((key, position)),
                                }
                            }
                            Input::Ui(command) => self.command(command),
                            Input::Audio(event) => self.event(event),
                        }
                    }
                    for (key, position) in volumes.drain(..) {
                        self.set_volume(&key, position);
                    }
                }
                Err(flume::RecvTimeoutError::Timeout) => {}
                Err(flume::RecvTimeoutError::Disconnected) => return,
            }
            let now = Instant::now();
            if self.visible && now >= self.next_meter {
                self.tick();
                self.next_meter = (self.next_meter + METER_EVERY).max(now);
            } else if !self.visible {
                self.send_tray();
            }
        }
    }

    fn command(&mut self, command: Command) {
        match command {
            Command::Visible(visible) => {
                if visible && !self.visible {
                    self.next_meter = Instant::now();
                    // Shown again: rows that expired while hidden go now.
                    self.refilter();
                }
                if !visible {
                    self.save_memory();
                }
                self.visible = visible;
            }
            Command::SetMute { key, muted } => {
                let error = self.with_sessions(&key, |audio, target| audio.set_mute(target, muted));
                self.report(error);
                if let Some(app) = self.apps.iter().find(|a| a.key.to_text() == key).cloned() {
                    let position = curve::position_from_scalar(app.volume);
                    self.remember(&app.key, position, muted);
                }
            }
            Command::SetVolume { key, position } => self.set_volume(&key, position),
            Command::SetDeviceVolume { id, volume } => {
                let result = self.audio.as_deref_mut().map(|a| a.set_volume(&Target::Device(DeviceId(id)), volume));
                self.report(result.and_then(|r| r.err()).map(|e| e.to_string()));
            }
            Command::SetDeviceMute { id, muted } => {
                let result = self.audio.as_deref_mut().map(|a| a.set_mute(&Target::Device(DeviceId(id)), muted));
                self.report(result.and_then(|r| r.err()).map(|e| e.to_string()));
            }
            Command::SetDefault { id } => {
                let device = DeviceId(id);
                let mut error = None;
                if let Some(audio) = self.audio.as_deref_mut() {
                    // Every role, as Windows' own output picker does.
                    for role in [Role::Console, Role::Multimedia, Role::Communications] {
                        if let Err(err) = audio.set_default_device(&device, role) {
                            error.get_or_insert(err.to_string());
                        }
                    }
                }
                self.report(error);
            }
            Command::StepMaster(steps) => {
                if let Some(master) = self.master_device().cloned() {
                    let volume = (master.volume + steps as f32 * MASTER_STEP).clamp(0.0, 1.0);
                    let result = self.audio.as_deref_mut().map(|a| a.set_volume(&Target::Device(master.id), volume));
                    self.report(result.and_then(|r| r.err()).map(|e| e.to_string()));
                }
            }
            Command::Quit => {}
        }
    }

    fn event(&mut self, event: AudioEvent) {
        match event {
            AudioEvent::DevicesChanged => self.read_devices(),
            AudioEvent::DeviceVolume { id, volume, muted, .. } => {
                if let Some(d) = self.devices.iter_mut().find(|d| d.id == id) {
                    d.volume = volume;
                    d.muted = muted;
                    self.devices_dirty = true;
                }
                if Some(&id) == self.master.as_ref() {
                    self.update_tray();
                }
            }
            AudioEvent::SessionAdded { device } => {
                if Some(&device) == self.master.as_ref() {
                    self.read_sessions(true);
                }
            }
            AudioEvent::SessionVolume { id, volume, muted, .. } => {
                if let Some(s) = self.sessions.iter_mut().find(|s| s.id == id) {
                    s.volume = volume;
                    s.muted = muted;
                    self.regroup();
                }
            }
            AudioEvent::SessionState { id, state } => {
                if state == SessionState::Expired {
                    self.sessions.retain(|s| s.id != id);
                } else if let Some(s) = self.sessions.iter_mut().find(|s| s.id == id) {
                    s.state = state;
                }
                self.regroup();
            }
        }
    }

    fn master_device(&self) -> Option<&Device> {
        self.master.as_ref().and_then(|id| self.devices.iter().find(|d| &d.id == id))
    }

    fn set_volume(&mut self, key: &str, position: f32) {
        let scalar = curve::scalar_from_position(position);
        let error = self.with_sessions(key, |audio, target| audio.set_volume(target, scalar));
        self.report(error);
        if let Some(app) = self.apps.iter().find(|a| a.key.to_text() == key).cloned() {
            self.remember(&app.key, position, app.muted);
        }
    }

    fn remember(&mut self, key: &AppKey, position: f32, muted: bool) {
        if let Some(memory) = self.memory.as_mut()
            && memory.remember(key, Remembered { position, muted })
        {
            self.memory_dirty = true;
        }
    }

    fn save_memory(&mut self) {
        if self.memory_dirty
            && let (Some(memory), Some(save)) = (&self.memory, &self.save)
        {
            save(memory);
            self.memory_dirty = false;
        }
    }

    /// Runs `apply` on every session of the app keyed `key`; the first error, if any.
    fn with_sessions(
        &mut self,
        key: &str,
        mut apply: impl FnMut(&mut dyn Audio, &Target) -> volute_audio::Result<()>,
    ) -> Option<String> {
        let audio = self.audio.as_deref_mut()?;
        let app = self.apps.iter().find(|a| a.key.to_text() == key)?;
        let mut error = None;
        for session in &app.sessions {
            if let Err(err) = apply(audio, &Target::Session(session.clone())) {
                error.get_or_insert_with(|| err.to_string());
            }
        }
        error
    }

    fn report(&mut self, error: Option<String>) {
        let error = error.unwrap_or_default();
        if error != self.error {
            self.error = error;
            self.error_dirty = true;
        }
    }

    fn read_devices(&mut self) {
        let Some(audio) = self.audio.as_deref_mut() else {
            return;
        };
        let result = audio.devices(&mut self.devices);
        if let Err(err) = result {
            self.devices.clear();
            self.report(Some(err.to_string()));
        }
        self.devices_dirty = true;
        let master = default_device(&self.devices, Flow::Render).map(|d| d.id.clone());
        self.mic = default_device(&self.devices, Flow::Capture).map(|d| d.id.clone());
        if master != self.master {
            self.master = master;
            self.sessions.clear();
            self.read_sessions(false);
        }
        self.update_tray();
    }

    /// Reads the master's sessions again. `remembered`: new sessions get the volume the user set
    /// in Volute for their app, when Windows gave them another one.
    fn read_sessions(&mut self, remembered: bool) {
        let (Some(audio), Some(master)) = (self.audio.as_deref_mut(), self.master.clone()) else {
            self.sessions.clear();
            self.regroup();
            return;
        };
        let before: HashSet<_> = self.sessions.iter().map(|s| s.id.clone()).collect();
        let result = audio.sessions(&master, &mut self.sessions);
        let error = result.err().map(|e| e.to_string());
        if error.is_some() {
            self.sessions.clear();
        }
        if remembered && let Some(memory) = &self.memory {
            let mut changes = Vec::new();
            for session in self.sessions.iter().filter(|s| !before.contains(&s.id)) {
                let key = apps::key_of(session);
                let position = curve::position_from_scalar(session.volume);
                if let Some(r) = memory.to_apply(&key, position, session.muted) {
                    changes.push((session.id.clone(), r));
                }
            }
            for (id, r) in changes {
                let target = Target::Session(id.clone());
                let scalar = curve::scalar_from_position(r.position);
                let _ = audio.set_volume(&target, scalar);
                let _ = audio.set_mute(&target, r.muted);
                if let Some(s) = self.sessions.iter_mut().find(|s| s.id == id) {
                    s.volume = scalar;
                    s.muted = r.muted;
                }
            }
        }
        self.report(error);
        self.regroup();
    }

    /// Groups the sessions into apps again; the rows change only if something visible did.
    fn regroup(&mut self) {
        let before = std::mem::take(&mut self.apps);
        apps::group(&self.sessions, &mut self.apps);
        let now = self.now();
        self.activity.update(&self.apps, now);
        if self.apps != before {
            self.rows_dirty = true;
        }
        self.refilter();
    }

    /// Which apps are rows now (apps that stopped playing leave after a while).
    fn refilter(&mut self) {
        let now = self.now();
        let shown: Vec<usize> = (0..self.apps.len())
            .filter(|&i| self.activity.shown(&self.apps[i], now))
            .collect();
        self.next_expiry = self.activity.next_expiry(&self.apps, now);
        if shown != self.shown {
            self.shown = shown;
            self.rows_dirty = true;
        }
        self.meters.clear();
        self.owners.clear();
        for (row, &i) in self.shown.iter().enumerate() {
            for session in &self.apps[i].sessions {
                self.meters.push(Target::Session(session.clone()));
                self.owners.push(row);
            }
        }
        if self.levels.len() != self.shown.len() {
            self.levels = vec![0.0; self.shown.len()];
        }
    }

    fn update_tray(&mut self) {
        let tray = self.master_device().map(|d| TrayData {
            device: d.name.clone(),
            volume: (d.volume * 100.0).round() as u32,
            muted: d.muted,
        });
        if tray != self.tray {
            self.tray = tray;
            self.tray_dirty = true;
        }
    }

    /// While hidden, only the tray's state goes to the UI, and only when it changed.
    fn send_tray(&mut self) {
        if !self.tray_dirty || self.in_flight.load(Ordering::Acquire) {
            return;
        }
        let frame = Frame {
            tray: self.tray.clone(),
            ..Frame::default()
        };
        self.in_flight.store(true, Ordering::Release);
        if (self.deliver)(frame, InFlight(self.in_flight.clone())) {
            self.tray_dirty = false;
        }
    }

    /// Names and icons for the rows that have none yet: read only while shown, once per
    /// executable.
    fn look_up(&mut self) {
        for &i in &self.shown {
            let Some(exe) = exe_of(&self.apps[i], &self.sessions) else { continue };
            let key = exe.to_string_lossy().to_lowercase();
            if self.looked.contains_key(&key) {
                continue;
            }
            let (name, icon) = self.looks.look(&exe, self.icon_size);
            let id = match icon {
                Some(icon) => {
                    let id = self.next_icon;
                    self.next_icon += 1;
                    self.pending_icons.push(IconData { id, width: icon.width, height: icon.height, rgba: icon.rgba });
                    id
                }
                None => -1,
            };
            self.looked.insert(key, (name, id));
            self.rows_dirty = true;
        }
    }

    fn row_data(&self, app: &App) -> RowData {
        let looked = exe_of(app, &self.sessions)
            .and_then(|exe| self.looked.get(&exe.to_string_lossy().to_lowercase()));
        let name = looked
            .and_then(|(name, _)| name.clone())
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| app.name.clone());
        RowData {
            key: app.key.to_text(),
            name,
            system: app.system_sounds(),
            volume: curve::position_from_scalar(app.volume),
            muted: app.muted,
            playing: app.playing,
            icon: looked.map_or(-1, |(_, id)| *id),
        }
    }

    fn tick(&mut self) {
        if self.next_expiry.is_some_and(|at| self.now() >= at) {
            self.refilter();
        }
        self.look_up();
        // The UI has not applied the last frame yet: skip this one, keep what changed.
        if self.in_flight.load(Ordering::Acquire) {
            return;
        }
        let mut targets = self.meters.clone();
        let extra = [self.master.clone(), self.mic.clone()];
        for id in extra.iter().flatten() {
            targets.push(Target::Device(id.clone()));
        }
        let mut fresh = vec![0.0; self.shown.len()];
        let (mut master, mut mic) = (0.0, 0.0);
        if let Some(audio) = self.audio.as_deref_mut()
            && audio.peaks(&targets, &mut self.peaks).is_ok()
        {
            for (peak, &row) in self.peaks.iter().zip(&self.owners) {
                fresh[row] = f32::max(fresh[row], *peak);
            }
            let mut rest = self.peaks[self.owners.len().min(self.peaks.len())..].iter();
            if self.master.is_some() {
                master = rest.next().copied().unwrap_or(0.0);
            }
            if self.mic.is_some() {
                mic = rest.next().copied().unwrap_or(0.0);
            }
        }
        let smooth = |old: f32, new: f32| if new >= old { new } else { (old * RELEASE).max(new) };
        for (level, new) in self.levels.iter_mut().zip(fresh) {
            *level = smooth(*level, new);
        }
        self.master_level = smooth(self.master_level, master);
        self.mic_level = smooth(self.mic_level, mic);
        let frame = Frame {
            rows: self
                .rows_dirty
                .then(|| self.shown.iter().map(|&i| self.row_data(&self.apps[i])).collect()),
            devices: self.devices_dirty.then(|| self.devices.iter().map(device_data).collect()),
            levels: self.levels.clone(),
            master_level: self.master_level,
            mic_level: self.mic_level,
            icons: std::mem::take(&mut self.pending_icons),
            error: self.error_dirty.then(|| self.error.clone()),
            tray: self.tray_dirty.then(|| self.tray.clone()).flatten(),
        };
        self.in_flight.store(true, Ordering::Release);
        if (self.deliver)(frame, InFlight(self.in_flight.clone())) {
            self.rows_dirty = false;
            self.devices_dirty = false;
            self.error_dirty = false;
            self.tray_dirty = false;
        }
    }
}

/// The executable of an app, from any of its sessions.
fn exe_of(app: &App, sessions: &[Session]) -> Option<PathBuf> {
    app.sessions
        .iter()
        .find_map(|id| sessions.iter().find(|s| &s.id == id).and_then(|s| s.exe.clone()))
}

fn device_data(d: &Device) -> DeviceData {
    DeviceData {
        id: d.id.0.clone(),
        name: d.name.clone(),
        output: d.flow == Flow::Render,
        volume: d.volume,
        muted: d.muted,
        default: d.default,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use volute_audio::{MockAudio, SessionState};

    use super::*;

    struct Run {
        tx: flume::Sender<Input>,
        frames: Arc<Mutex<Vec<Frame>>>,
        thread: thread::JoinHandle<()>,
    }

    impl Run {
        fn send(&self, command: Command) {
            self.tx.send(Input::Ui(command)).unwrap();
        }

        fn stop(self) -> Vec<Frame> {
            self.send(Command::Quit);
            self.thread.join().unwrap();
            std::mem::take(&mut *self.frames.lock().unwrap())
        }

        fn last_rows(&self) -> Option<Vec<RowData>> {
            self.frames.lock().unwrap().iter().rev().find_map(|f| f.rows.clone())
        }
    }

    fn start_with(audio: MockAudio, memory: Option<Memory>) -> Run {
        let frames = Arc::new(Mutex::new(Vec::new()));
        let record = frames.clone();
        let (tx, rx) = flume::unbounded();
        let thread = spawn(
            Box::new(move || Ok(Box::new(audio) as Backend)),
            Box::new(NoLooks),
            Options { icon_size: 32, memory, save: None },
            Box::new(move |frame, in_flight| {
                record.lock().unwrap().push(frame);
                drop(in_flight);
                true
            }),
            (tx.clone(), rx),
        );
        Run { tx, frames, thread }
    }

    fn start() -> Run {
        start_with(MockAudio::demo(), None)
    }

    fn wait() {
        thread::sleep(Duration::from_millis(150));
    }

    #[test]
    fn hidden_sends_only_the_tray_and_shown_sends_rows_then_levels() {
        let run = start();
        wait();
        {
            let frames = run.frames.lock().unwrap();
            assert_eq!(frames.len(), 1, "hidden: just the tray's state");
            let tray = frames[0].tray.as_ref().expect("the tray state");
            assert_eq!((tray.volume, tray.muted), (56, false));
            assert!(frames[0].rows.is_none() && frames[0].levels.is_empty());
        }
        run.send(Command::Visible(true));
        thread::sleep(Duration::from_millis(250));
        run.send(Command::Visible(false));
        thread::sleep(Duration::from_millis(50));
        let count = run.frames.lock().unwrap().len();
        thread::sleep(Duration::from_millis(150));
        let frames = run.stop();
        assert_eq!(frames.len(), count, "hidden again: nothing more");
        assert!((5..=11).contains(&count), "about 30 frames a second, got {count}");
        let shown = &frames[1..];
        let rows = shown[0].rows.as_ref().expect("the first shown frame has the rows");
        let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["Discord", "firefox", "Spotify", "hollow_knight", ""]);
        assert!(rows[4].system);
        assert!(shown[0].devices.as_ref().is_some_and(|d| d.len() == 3));
        assert!(shown[1..].iter().all(|f| f.rows.is_none() && f.devices.is_none()), "only changes");
        assert!(shown.iter().all(|f| f.levels.len() == rows.len()));
        assert!(shown.iter().any(|f| f.levels[2] > 0.0), "Spotify's meter moves");
        assert!(shown.iter().any(|f| f.master_level > 0.0 && f.mic_level > 0.0));
    }

    #[test]
    fn nothing_polls_sessions_events_do() {
        let audio = MockAudio::demo();
        let control = audio.clone();
        let run = start_with(audio, None);
        run.send(Command::Visible(true));
        thread::sleep(Duration::from_millis(400));
        assert_eq!(control.calls().sessions, 1, "read once at start, then only on events");
        control.add_session("{0.0.0.00000000}.{speakers}", MockAudio::session("vlc", 9, r"C:\VLC\vlc.exe"));
        control.change_session("spotify", 0.25, true);
        wait();
        assert_eq!(control.calls().sessions, 2, "one read for the new session");
        let rows = run.last_rows().unwrap();
        assert!(rows.iter().any(|r| r.name == "vlc"), "the new app is listed");
        let spotify = rows.iter().find(|r| r.name == "Spotify").unwrap();
        assert!(spotify.muted && (spotify.volume - curve::position_from_scalar(0.25)).abs() < 1e-4);
        control.set_session_state("vlc", SessionState::Expired);
        wait();
        assert!(run.last_rows().unwrap().iter().all(|r| r.name != "vlc"), "ended: gone");
        run.stop();
    }

    #[test]
    fn commands_change_the_app_and_come_back_in_the_rows() {
        let run = start();
        run.send(Command::Visible(true));
        wait();
        let key = r"exe:c:\program files\mozilla firefox\firefox.exe".to_owned();
        // A drag: only the last position matters.
        for position in [0.1, 0.2, 0.5] {
            run.send(Command::SetVolume { key: key.clone(), position });
        }
        run.send(Command::SetMute { key: key.clone(), muted: true });
        thread::sleep(Duration::from_millis(200));
        let rows = run.last_rows().unwrap();
        let firefox = rows.iter().find(|r| r.key == key).unwrap();
        assert!((firefox.volume - 0.5).abs() < 1e-4, "{}", firefox.volume);
        assert!(firefox.muted, "both sessions muted");
        run.stop();
    }

    #[test]
    fn the_master_follows_the_tray_wheel_and_device_commands() {
        let run = start();
        run.send(Command::StepMaster(2));
        wait();
        let tray = run.frames.lock().unwrap().iter().rev().find_map(|f| f.tray.clone()).unwrap();
        assert_eq!(tray.volume, 60, "56 % and two notches of 2 %");
        run.send(Command::SetDeviceMute { id: "{0.0.0.00000000}.{speakers}".into(), muted: true });
        wait();
        let tray = run.frames.lock().unwrap().iter().rev().find_map(|f| f.tray.clone()).unwrap();
        assert!(tray.muted, "the tray shows the master muted");
        run.send(Command::SetDefault { id: "{0.0.0.00000000}.{headphones}".into() });
        wait();
        let tray = run.frames.lock().unwrap().iter().rev().find_map(|f| f.tray.clone()).unwrap();
        assert_eq!(tray.device, "Headphones (WH-1000XM4)");
        run.stop();
    }

    #[test]
    fn a_new_session_gets_the_volume_remembered_for_its_app() {
        let audio = MockAudio::demo();
        let control = audio.clone();
        let mut memory = Memory::default();
        memory.remember(&AppKey::Exe(r"c:\vlc\vlc.exe".into()), Remembered { position: 0.4, muted: false });
        let run = start_with(audio, Some(memory));
        // Started and read once: an app already playing then is not a new session.
        wait();
        control.add_session("{0.0.0.00000000}.{speakers}", MockAudio::session("vlc", 9, r"C:\VLC\vlc.exe"));
        wait();
        run.send(Command::Visible(true));
        wait();
        let rows = run.last_rows().unwrap();
        let vlc = rows.iter().find(|r| r.name == "vlc").unwrap();
        assert!((vlc.volume - 0.4).abs() < 1e-3, "remembered 40 %, got {}", vlc.volume);
        // Changed in Windows' mixer while it plays: Volute does not fight it.
        control.change_session("vlc", 0.9, false);
        wait();
        let vlc = run.last_rows().unwrap().into_iter().find(|r| r.name == "vlc").unwrap();
        assert!((vlc.volume - curve::position_from_scalar(0.9)).abs() < 1e-3);
        run.stop();
    }

    #[test]
    fn a_backend_that_fails_reports_and_still_quits() {
        let frames = Arc::new(Mutex::new(Vec::new()));
        let record = frames.clone();
        let (tx, rx) = flume::unbounded();
        let thread = spawn(
            Box::new(|| Err(volute_audio::Error::Unsupported)),
            Box::new(NoLooks),
            Options { icon_size: 32, memory: None, save: None },
            Box::new(move |frame, _| {
                record.lock().unwrap().push(frame);
                true
            }),
            (tx.clone(), rx),
        );
        tx.send(Input::Ui(Command::Visible(true))).unwrap();
        thread::sleep(Duration::from_millis(100));
        tx.send(Input::Ui(Command::Quit)).unwrap();
        thread.join().unwrap();
        let frames = frames.lock().unwrap();
        assert_eq!(frames[0].error.as_deref(), Some("not supported yet"));
    }

    #[test]
    fn one_frame_in_flight_at_most() {
        let delivered = Arc::new(Mutex::new(Vec::<InFlight>::new()));
        let held = delivered.clone();
        let (tx, rx) = flume::unbounded();
        let thread = spawn(
            Box::new(|| Ok(Box::new(MockAudio::demo()) as Backend)),
            Box::new(NoLooks),
            Options { icon_size: 32, memory: None, save: None },
            // A stalled UI: it never applies, so the guard stays held.
            Box::new(move |_, in_flight| {
                held.lock().unwrap().push(in_flight);
                true
            }),
            (tx.clone(), rx),
        );
        tx.send(Input::Ui(Command::Visible(true))).unwrap();
        thread::sleep(Duration::from_millis(200));
        assert_eq!(delivered.lock().unwrap().len(), 1, "nothing piles up behind a stalled UI");
        delivered.lock().unwrap().clear();
        thread::sleep(Duration::from_millis(100));
        assert_eq!(delivered.lock().unwrap().len(), 1, "the next goes once the first is applied");
        tx.send(Input::Ui(Command::Quit)).unwrap();
        thread.join().unwrap();
    }
}
