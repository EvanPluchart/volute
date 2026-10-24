//! The audio thread: owns the `Audio` backend (COM lives here only), applies the UI's commands,
//! and sends frames back. While the window is hidden it sleeps on its channel and reads
//! nothing; while shown, it reads peaks at 30 Hz and sessions every second (events come in M1).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use volute_audio::{Audio, Device, MeterId, Session, Target, default_device};
use volute_core::apps::{self, App};
use volute_core::curve;

const METER_EVERY: Duration = Duration::from_millis(33);
const SESSIONS_EVERY: Duration = Duration::from_secs(1);

/// What the UI asks of the audio thread.
#[derive(Debug)]
pub enum Command {
    /// The window was shown (meters run) or hidden (nothing runs).
    Visible(bool),
    /// A slider moved: the app's volume at this slider position.
    SetVolume { key: String, position: f32 },
    SetMute { key: String, muted: bool },
    /// Stop now. Not waiting for every sender to drop, which a callback holding one would
    /// prevent (decision 0004).
    Quit,
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
}

/// What the audio thread sends the UI thread.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Frame {
    /// The rows, when they changed since the last frame delivered.
    pub rows: Option<Vec<RowData>>,
    /// One level per row, in the order of the latest rows.
    pub levels: Vec<f32>,
    /// The error to show, when it changed: empty clears it.
    pub error: Option<String>,
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
/// Hands a frame to the UI thread; `false` when it is gone.
pub type Deliver = Box<dyn Fn(Frame, InFlight) -> bool + Send>;

pub fn spawn(
    make: MakeBackend,
    deliver: Deliver,
    commands: flume::Receiver<Command>,
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("volute-audio".into())
        .spawn(move || {
            let mut mixer = Mixer::new(make, deliver);
            mixer.run(&commands);
        })
        .expect("spawning the audio thread")
}

struct Mixer {
    audio: Option<Backend>,
    deliver: Deliver,
    in_flight: Arc<AtomicBool>,
    visible: bool,
    devices: Vec<Device>,
    sessions: Vec<Session>,
    apps: Vec<App>,
    /// Every session of every app, and the row each belongs to.
    meters: Vec<MeterId>,
    owners: Vec<usize>,
    peaks: Vec<f32>,
    rows_dirty: bool,
    error: String,
    error_dirty: bool,
    next_refresh: Instant,
    next_meter: Instant,
}

impl Mixer {
    fn new(make: MakeBackend, deliver: Deliver) -> Mixer {
        let (audio, error) = match make() {
            Ok(audio) => (Some(audio), String::new()),
            Err(err) => (None, err.to_string()),
        };
        let now = Instant::now();
        Mixer {
            audio,
            deliver,
            in_flight: Arc::new(AtomicBool::new(false)),
            visible: false,
            devices: Vec::new(),
            sessions: Vec::new(),
            apps: Vec::new(),
            meters: Vec::new(),
            owners: Vec::new(),
            peaks: Vec::new(),
            rows_dirty: true,
            error_dirty: !error.is_empty(),
            error,
            next_refresh: now,
            next_meter: now,
        }
    }

    fn run(&mut self, commands: &flume::Receiver<Command>) {
        let mut volumes: Vec<(String, f32)> = Vec::new();
        loop {
            let received = if self.visible {
                commands.recv_deadline(self.next_meter)
            } else {
                // Hidden: sleep until the UI says something. Nothing ticks.
                commands
                    .recv()
                    .map_err(|_| flume::RecvTimeoutError::Disconnected)
            };
            match received {
                Ok(command) => {
                    // Take everything waiting, so a fast drag applies its last position once.
                    let mut next = Some(command);
                    while let Some(command) = next.take().or_else(|| commands.try_recv().ok()) {
                        match command {
                            Command::Quit => return,
                            Command::SetVolume { key, position } => {
                                match volumes.iter_mut().find(|(k, _)| *k == key) {
                                    Some(pending) => pending.1 = position,
                                    None => volumes.push((key, position)),
                                }
                            }
                            command => self.handle(command),
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
                self.tick(now);
                self.next_meter = (self.next_meter + METER_EVERY).max(now);
            }
        }
    }

    fn handle(&mut self, command: Command) {
        match command {
            Command::Visible(visible) => {
                if visible && !self.visible {
                    let now = Instant::now();
                    self.next_refresh = now;
                    self.next_meter = now;
                }
                self.visible = visible;
            }
            Command::SetMute { key, muted } => {
                let error = self.with_sessions(&key, |audio, target| audio.set_mute(target, muted));
                self.report(error);
                // Show the new state at once.
                self.next_refresh = Instant::now();
            }
            Command::SetVolume { key, position } => self.set_volume(&key, position),
            Command::Quit => {}
        }
    }

    fn set_volume(&mut self, key: &str, position: f32) {
        let scalar = curve::scalar_from_position(position);
        let error = self.with_sessions(key, |audio, target| audio.set_volume(target, scalar));
        self.report(error);
        if let Some(app) = self.apps.iter_mut().find(|a| a.key.to_text() == key) {
            app.volume = scalar;
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

    /// Reads the sessions of the default device again, and groups them into rows.
    fn refresh(&mut self) {
        let Some(audio) = self.audio.as_deref_mut() else {
            return;
        };
        let result = audio.devices(&mut self.devices).and_then(|()| {
            match default_device(&self.devices) {
                Some(device) => audio.sessions(&device.id.clone(), &mut self.sessions),
                None => {
                    self.sessions.clear();
                    Ok(())
                }
            }
        });
        let error = result.err().map(|e| e.to_string());
        if error.is_some() {
            self.sessions.clear();
        }
        self.report(error);
        let before = std::mem::take(&mut self.apps);
        apps::group(&self.sessions, &mut self.apps);
        if self.apps != before {
            self.rows_dirty = true;
        }
        self.meters.clear();
        self.owners.clear();
        for (row, app) in self.apps.iter().enumerate() {
            for session in &app.sessions {
                self.meters.push(Target::Session(session.clone()));
                self.owners.push(row);
            }
        }
    }

    fn tick(&mut self, now: Instant) {
        if now >= self.next_refresh {
            self.refresh();
            self.next_refresh = now + SESSIONS_EVERY;
        }
        // The UI has not applied the last frame yet: skip this one, keep what changed.
        if self.in_flight.load(Ordering::Acquire) {
            return;
        }
        let mut levels = vec![0.0; self.apps.len()];
        if let Some(audio) = self.audio.as_deref_mut()
            && audio.peaks(&self.meters, &mut self.peaks).is_ok()
        {
            for (peak, &row) in self.peaks.iter().zip(&self.owners) {
                levels[row] = f32::max(levels[row], *peak);
            }
        }
        let frame = Frame {
            rows: self.rows_dirty.then(|| self.apps.iter().map(row_data).collect()),
            levels,
            error: self.error_dirty.then(|| self.error.clone()),
        };
        self.in_flight.store(true, Ordering::Release);
        if (self.deliver)(frame, InFlight(self.in_flight.clone())) {
            self.rows_dirty = false;
            self.error_dirty = false;
        }
    }
}

fn row_data(app: &App) -> RowData {
    RowData {
        key: app.key.to_text(),
        name: app.name.clone(),
        system: app.system_sounds(),
        volume: curve::position_from_scalar(app.volume),
        muted: app.muted,
        playing: app.playing,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use volute_audio::MockAudio;

    use super::*;

    /// Runs a mixer on the demo mock, records the frames it delivers, and hands back the
    /// command sender with the recorded frames.
    fn start() -> (flume::Sender<Command>, Arc<Mutex<Vec<Frame>>>, thread::JoinHandle<()>) {
        let frames = Arc::new(Mutex::new(Vec::new()));
        let record = frames.clone();
        let (tx, rx) = flume::unbounded();
        let thread = spawn(
            Box::new(|| Ok(Box::new(MockAudio::demo()) as Backend)),
            Box::new(move |frame, in_flight| {
                record.lock().unwrap().push(frame);
                drop(in_flight);
                true
            }),
            rx,
        );
        (tx, frames, thread)
    }

    #[test]
    fn hidden_sends_nothing_and_shown_sends_rows_then_levels() {
        let (tx, frames, thread) = start();
        thread::sleep(Duration::from_millis(150));
        assert!(frames.lock().unwrap().is_empty(), "nothing runs while hidden");

        tx.send(Command::Visible(true)).unwrap();
        thread::sleep(Duration::from_millis(250));
        tx.send(Command::Visible(false)).unwrap();
        thread::sleep(Duration::from_millis(50));
        let count = frames.lock().unwrap().len();
        thread::sleep(Duration::from_millis(150));
        tx.send(Command::Quit).unwrap();
        thread.join().unwrap();

        let frames = frames.lock().unwrap();
        assert_eq!(frames.len(), count, "hidden again: nothing more");
        assert!((4..=10).contains(&count), "about 30 frames a second, got {count}");
        let rows = frames[0].rows.as_ref().expect("the first frame has the rows");
        let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["Discord", "firefox", "Spotify", "hollow_knight", ""]);
        assert!(rows[4].system);
        assert!(frames[1..].iter().all(|f| f.rows.is_none()), "rows only when they change");
        assert!(frames.iter().all(|f| f.levels.len() == rows.len()));
        assert!(frames.iter().any(|f| f.levels[2] > 0.0), "Spotify's meter moves");
    }

    #[test]
    fn commands_change_the_app_and_come_back_in_the_rows() {
        let (tx, frames, thread) = start();
        tx.send(Command::Visible(true)).unwrap();
        thread::sleep(Duration::from_millis(100));
        let key = r"exe:c:\program files\mozilla firefox\firefox.exe".to_owned();
        // A drag: only the last position matters.
        for position in [0.1, 0.2, 0.5] {
            tx.send(Command::SetVolume { key: key.clone(), position }).unwrap();
        }
        tx.send(Command::SetMute { key: key.clone(), muted: true }).unwrap();
        thread::sleep(Duration::from_millis(200));
        tx.send(Command::Quit).unwrap();
        thread.join().unwrap();

        let frames = frames.lock().unwrap();
        let last = frames.iter().rev().find_map(|f| f.rows.as_ref()).unwrap();
        let firefox = last.iter().find(|r| r.key == key).unwrap();
        assert!((firefox.volume - 0.5).abs() < 1e-4, "{}", firefox.volume);
        assert!(firefox.muted, "both sessions muted");
    }

    #[test]
    fn a_backend_that_fails_reports_and_still_quits() {
        let frames = Arc::new(Mutex::new(Vec::new()));
        let record = frames.clone();
        let (tx, rx) = flume::unbounded();
        let thread = spawn(
            Box::new(|| Err(volute_audio::Error::Unsupported)),
            Box::new(move |frame, _| {
                record.lock().unwrap().push(frame);
                true
            }),
            rx,
        );
        tx.send(Command::Visible(true)).unwrap();
        thread::sleep(Duration::from_millis(100));
        tx.send(Command::Quit).unwrap();
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
            // A stalled UI: it never applies, so the guard stays held.
            Box::new(move |_, in_flight| {
                held.lock().unwrap().push(in_flight);
                true
            }),
            rx,
        );
        tx.send(Command::Visible(true)).unwrap();
        thread::sleep(Duration::from_millis(200));
        assert_eq!(delivered.lock().unwrap().len(), 1, "nothing piles up behind a stalled UI");
        delivered.lock().unwrap().clear();
        thread::sleep(Duration::from_millis(100));
        assert_eq!(delivered.lock().unwrap().len(), 1, "the next goes once the first is applied");
        tx.send(Command::Quit).unwrap();
        thread.join().unwrap();
    }
}
