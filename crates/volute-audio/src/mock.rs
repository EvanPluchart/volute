//! A scripted sound system: devices, sessions and peaks set up by the caller. Tests and
//! screenshots use it, so they never touch the volume of a real app.
//!
//! A `MockAudio` is also its own remote control: clone it before handing it to the audio thread,
//! and the clone can add sessions or change them "from another program", which sends the same
//! events Core Audio would.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::{
    Audio, AudioEvent, Device, DeviceId, Error, EventSink, Flow, MeterId, Result, Role, Session,
    SessionId, SessionState, Target,
};

/// How often each read was called: tests check that nothing polls.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Calls {
    pub devices: usize,
    pub sessions: usize,
    pub peaks: usize,
}

#[derive(Default)]
struct State {
    devices: Vec<Device>,
    /// Sessions per device, in the order they were added.
    sessions: Vec<(DeviceId, Session)>,
    /// Peak sequences, each played in a loop: one value per call to `peaks`.
    peaks: HashMap<MeterId, (Vec<f32>, usize)>,
    sink: Option<EventSink>,
    calls: Calls,
}

#[derive(Clone, Default)]
pub struct MockAudio {
    state: Arc<Mutex<State>>,
}

impl MockAudio {
    pub fn new() -> MockAudio {
        MockAudio::default()
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Sends `event` to the listener, outside the lock.
    fn emit(&self, event: AudioEvent) {
        let sink = self.state().sink.clone();
        if let Some(sink) = sink {
            sink(event);
        }
    }

    fn with(self, flow: Flow, id: &str, name: &str, volume: f32) -> MockAudio {
        {
            let mut state = self.state();
            let default = !state.devices.iter().any(|d| d.flow == flow);
            state.devices.push(Device {
                id: DeviceId(id.into()),
                name: name.into(),
                flow,
                volume,
                muted: false,
                default,
            });
        }
        self
    }

    /// Adds an output; the first one added is the default.
    pub fn with_device(self, id: &str, name: &str, volume: f32) -> MockAudio {
        self.with(Flow::Render, id, name, volume)
    }

    /// Adds an input; the first one added is the default.
    pub fn with_input(self, id: &str, name: &str, volume: f32) -> MockAudio {
        self.with(Flow::Capture, id, name, volume)
    }

    pub fn with_session(self, device: &str, session: Session) -> MockAudio {
        self.state().sessions.push((DeviceId(device.into()), session));
        self
    }

    /// The peaks `meter` reports, one per call to `peaks`, in a loop.
    pub fn with_peaks(self, meter: MeterId, sequence: Vec<f32>) -> MockAudio {
        self.state().peaks.insert(meter, (sequence, 0));
        self
    }

    /// An active session of `exe`, at full volume.
    pub fn session(id: &str, pid: u32, exe: &str) -> Session {
        Session {
            id: SessionId(id.into()),
            pid,
            exe: Some(PathBuf::from(exe)),
            display_name: String::new(),
            volume: 1.0,
            muted: false,
            state: SessionState::Active,
            system_sounds: false,
        }
    }

    /// How often each read has been called so far.
    pub fn calls(&self) -> Calls {
        self.state().calls
    }

    /// Another program starts playing: a session appears on `device`.
    pub fn add_session(&self, device: &str, session: Session) {
        self.state().sessions.push((DeviceId(device.into()), session));
        self.emit(AudioEvent::SessionAdded {
            device: DeviceId(device.into()),
        });
    }

    /// Another program (Windows' mixer, say) changes a session's volume or mute.
    pub fn change_session(&self, id: &str, volume: f32, muted: bool) {
        let found = self.state().sessions.iter_mut().find(|(_, s)| s.id.0 == id).map(|(_, s)| {
            s.volume = volume;
            s.muted = muted;
        });
        if found.is_some() {
            self.emit(AudioEvent::SessionVolume {
                id: SessionId(id.into()),
                volume,
                muted,
                ours: false,
            });
        }
    }

    /// A session starts or stops playing, or ends.
    pub fn set_session_state(&self, id: &str, state: SessionState) {
        let found = self.state().sessions.iter_mut().find(|(_, s)| s.id.0 == id).map(|(_, s)| {
            s.state = state;
        });
        if found.is_some() {
            self.emit(AudioEvent::SessionState {
                id: SessionId(id.into()),
                state,
            });
        }
    }

    /// Another program changes a device's volume or mute.
    pub fn change_device(&self, id: &str, volume: f32, muted: bool) {
        let found = self.state().devices.iter_mut().find(|d| d.id.0 == id).map(|d| {
            d.volume = volume;
            d.muted = muted;
        });
        if found.is_some() {
            self.emit(AudioEvent::DeviceVolume {
                id: DeviceId(id.into()),
                volume,
                muted,
                ours: false,
            });
        }
    }

    /// A believable machine for screenshots and `--mock`: speakers, headphones and a microphone,
    /// a music player, a browser with two sessions, a call app, a silent game and system sounds.
    pub fn demo() -> MockAudio {
        const SPEAKERS: &str = "{0.0.0.00000000}.{speakers}";
        let mut call = MockAudio::session("discord", 3100, r"C:\Users\demo\AppData\Local\Discord\app-1.0\Discord.exe");
        call.volume = 0.42;
        let mut game = MockAudio::session("game", 4200, r"D:\Games\Hollow Knight\hollow_knight.exe");
        game.state = SessionState::Inactive;
        game.volume = 0.7;
        let mut tab = MockAudio::session("firefox-2", 5202, r"C:\Program Files\Mozilla Firefox\firefox.exe");
        tab.volume = 0.8;
        let mut first_tab = MockAudio::session("firefox-1", 5201, r"C:\Program Files\Mozilla Firefox\firefox.exe");
        first_tab.volume = 0.8;
        let system = Session {
            id: SessionId("system".into()),
            pid: 0,
            exe: None,
            display_name: "@%SystemRoot%\\System32\\AudioSrv.Dll,-202".into(),
            volume: 1.0,
            muted: true,
            state: SessionState::Inactive,
            system_sounds: true,
        };
        let session = |id: &str| Target::Session(SessionId(id.into()));
        MockAudio::new()
            .with_device(SPEAKERS, "Speakers (Realtek Audio)", 0.56)
            .with_device("{0.0.0.00000000}.{headphones}", "Headphones (WH-1000XM4)", 0.3)
            .with_input("{0.0.1.00000000}.{microphone}", "Microphone (Blue Yeti)", 0.8)
            .with_session(SPEAKERS, MockAudio::session("spotify", 2100, r"C:\Users\demo\AppData\Roaming\Spotify\Spotify.exe"))
            .with_session(SPEAKERS, first_tab)
            .with_session(SPEAKERS, tab)
            .with_session(SPEAKERS, call)
            .with_session(SPEAKERS, game)
            .with_session(SPEAKERS, system)
            .with_peaks(session("spotify"), wave(0.62, 0.3, 45))
            .with_peaks(session("firefox-1"), wave(0.35, 0.2, 31))
            .with_peaks(session("firefox-2"), wave(0.15, 0.1, 17))
            .with_peaks(session("discord"), wave(0.25, 0.25, 23))
            .with_peaks(Target::Device(DeviceId(SPEAKERS.into())), wave(0.6, 0.3, 40))
            .with_peaks(Target::Device(DeviceId("{0.0.1.00000000}.{microphone}".into())), wave(0.2, 0.2, 29))
    }
}

/// `period` values of a level moving around `centre` by `swing`, never below zero: a meter
/// that looks like music.
fn wave(centre: f32, swing: f32, period: usize) -> Vec<f32> {
    (0..period)
        .map(|i| {
            let t = i as f32 / period as f32 * std::f32::consts::TAU;
            let beat = (t * 3.0).sin().abs() * 0.4;
            (centre + swing * (t.sin() * 0.6 + beat)).clamp(0.0, 1.0)
        })
        .collect()
}

impl Audio for MockAudio {
    fn listen(&mut self, sink: EventSink) -> Result<()> {
        self.state().sink = Some(sink);
        Ok(())
    }

    fn devices(&mut self, out: &mut Vec<Device>) -> Result<()> {
        let mut state = self.state();
        state.calls.devices += 1;
        out.clear();
        out.extend(state.devices.iter().cloned());
        Ok(())
    }

    fn sessions(&mut self, device: &DeviceId, out: &mut Vec<Session>) -> Result<()> {
        let mut state = self.state();
        state.calls.sessions += 1;
        if !state.devices.iter().any(|d| &d.id == device) {
            return Err(Error::NotFound(format!("device {}", device.0)));
        }
        out.clear();
        out.extend(
            state
                .sessions
                .iter()
                .filter(|(d, s)| d == device && s.state != SessionState::Expired)
                .map(|(_, s)| s.clone()),
        );
        Ok(())
    }

    fn peaks(&mut self, ids: &[MeterId], out: &mut Vec<f32>) -> Result<()> {
        let mut state = self.state();
        state.calls.peaks += 1;
        out.clear();
        for id in ids {
            let muted = match id {
                Target::Session(s) => state.sessions.iter().any(|(_, x)| &x.id == s && x.muted),
                Target::Device(d) => state.devices.iter().any(|x| &x.id == d && x.muted),
            };
            let peak = match state.peaks.get_mut(id) {
                Some((sequence, at)) if !sequence.is_empty() => {
                    let value = sequence[*at % sequence.len()];
                    *at = (*at + 1) % sequence.len();
                    value
                }
                _ => 0.0,
            };
            // A muted session is silent, as Core Audio reports it.
            out.push(if muted { 0.0 } else { peak });
        }
        Ok(())
    }

    fn set_volume(&mut self, target: &Target, volume: f32) -> Result<()> {
        let volume = volume.clamp(0.0, 1.0);
        let event = {
            let mut state = self.state();
            match target {
                Target::Session(id) => {
                    let s = session_mut(&mut state, id)?;
                    s.volume = volume;
                    AudioEvent::SessionVolume { id: id.clone(), volume, muted: s.muted, ours: true }
                }
                Target::Device(id) => {
                    let d = device_mut(&mut state, id)?;
                    d.volume = volume;
                    AudioEvent::DeviceVolume { id: id.clone(), volume, muted: d.muted, ours: true }
                }
            }
        };
        self.emit(event);
        Ok(())
    }

    fn set_mute(&mut self, target: &Target, muted: bool) -> Result<()> {
        let event = {
            let mut state = self.state();
            match target {
                Target::Session(id) => {
                    let s = session_mut(&mut state, id)?;
                    s.muted = muted;
                    AudioEvent::SessionVolume { id: id.clone(), volume: s.volume, muted, ours: true }
                }
                Target::Device(id) => {
                    let d = device_mut(&mut state, id)?;
                    d.muted = muted;
                    AudioEvent::DeviceVolume { id: id.clone(), volume: d.volume, muted, ours: true }
                }
            }
        };
        self.emit(event);
        Ok(())
    }

    fn set_default_device(&mut self, device: &DeviceId, _role: Role) -> Result<()> {
        {
            let mut state = self.state();
            let flow = state
                .devices
                .iter()
                .find(|d| &d.id == device)
                .map(|d| d.flow)
                .ok_or_else(|| Error::NotFound(format!("device {}", device.0)))?;
            for d in state.devices.iter_mut().filter(|d| d.flow == flow) {
                d.default = &d.id == device;
            }
        }
        self.emit(AudioEvent::DevicesChanged);
        Ok(())
    }
}

fn session_mut<'a>(state: &'a mut State, id: &SessionId) -> Result<&'a mut Session> {
    state
        .sessions
        .iter_mut()
        .map(|(_, s)| s)
        .find(|s| &s.id == id)
        .ok_or_else(|| Error::NotFound(format!("session {}", id.0)))
}

fn device_mut<'a>(state: &'a mut State, id: &DeviceId) -> Result<&'a mut Device> {
    state
        .devices
        .iter_mut()
        .find(|d| &d.id == id)
        .ok_or_else(|| Error::NotFound(format!("device {}", id.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn speakers() -> DeviceId {
        DeviceId("speakers".into())
    }

    fn mock() -> MockAudio {
        MockAudio::new()
            .with_device("speakers", "Speakers", 0.5)
            .with_device("headphones", "Headphones", 0.2)
            .with_input("mic", "Microphone", 0.7)
            .with_session("speakers", MockAudio::session("a", 1, r"C:\a.exe"))
            .with_session("speakers", MockAudio::session("b", 2, r"C:\b.exe"))
            .with_session("headphones", MockAudio::session("c", 3, r"C:\c.exe"))
            .with_peaks(Target::Session(SessionId("a".into())), vec![0.1, 0.5])
    }

    /// A listener that records events.
    fn record(audio: &mut MockAudio) -> Arc<Mutex<Vec<AudioEvent>>> {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        audio
            .listen(Arc::new(move |e| sink.lock().unwrap().push(e)))
            .unwrap();
        events
    }

    #[test]
    fn lists_devices_of_both_flows_and_the_sessions_of_one() {
        let mut audio = mock();
        let mut devices = Vec::new();
        audio.devices(&mut devices).unwrap();
        assert_eq!(devices.len(), 3);
        assert_eq!(crate::default_device(&devices, Flow::Render).unwrap().id, speakers());
        assert_eq!(crate::default_device(&devices, Flow::Capture).unwrap().id.0, "mic");
        let mut sessions = Vec::new();
        audio.sessions(&speakers(), &mut sessions).unwrap();
        let ids: Vec<&str> = sessions.iter().map(|s| s.id.0.as_str()).collect();
        assert_eq!(ids, ["a", "b"]);
        assert!(audio.sessions(&DeviceId("gone".into()), &mut sessions).is_err());
    }

    #[test]
    fn volume_and_mute_round_trip_and_report_as_ours() {
        let mut audio = mock();
        let events = record(&mut audio);
        let a = Target::Session(SessionId("a".into()));
        audio.set_volume(&a, 0.25).unwrap();
        audio.set_mute(&a, true).unwrap();
        audio.set_volume(&Target::Device(speakers()), 0.75).unwrap();
        let mut sessions = Vec::new();
        audio.sessions(&speakers(), &mut sessions).unwrap();
        assert_eq!((sessions[0].volume, sessions[0].muted), (0.25, true));
        // The other session kept its values.
        assert_eq!((sessions[1].volume, sessions[1].muted), (1.0, false));
        let mut devices = Vec::new();
        audio.devices(&mut devices).unwrap();
        assert_eq!(devices[0].volume, 0.75);
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 3);
        assert!(events.iter().all(|e| matches!(e,
            AudioEvent::SessionVolume { ours: true, .. } | AudioEvent::DeviceVolume { ours: true, .. })));
        // Out of range is clamped, unknown targets are errors.
        drop(events);
        audio.set_volume(&a, 3.0).unwrap();
        audio.sessions(&speakers(), &mut sessions).unwrap();
        assert_eq!(sessions[0].volume, 1.0);
        let gone = Target::Session(SessionId("gone".into()));
        assert!(matches!(audio.set_volume(&gone, 0.5), Err(Error::NotFound(_))));
    }

    #[test]
    fn the_remote_control_sends_what_another_program_would() {
        let mut audio = mock();
        let control = audio.clone();
        let events = record(&mut audio);
        control.change_session("b", 0.3, true);
        control.add_session("speakers", MockAudio::session("d", 4, r"C:\d.exe"));
        control.set_session_state("d", SessionState::Expired);
        control.change_device("speakers", 0.1, false);
        assert_eq!(
            *events.lock().unwrap(),
            [
                AudioEvent::SessionVolume { id: SessionId("b".into()), volume: 0.3, muted: true, ours: false },
                AudioEvent::SessionAdded { device: speakers() },
                AudioEvent::SessionState { id: SessionId("d".into()), state: SessionState::Expired },
                AudioEvent::DeviceVolume { id: speakers(), volume: 0.1, muted: false, ours: false },
            ]
        );
        let mut sessions = Vec::new();
        audio.sessions(&speakers(), &mut sessions).unwrap();
        assert!(sessions.iter().all(|s| s.id.0 != "d"), "expired sessions are gone");
        assert_eq!(audio.calls().sessions, 1);
    }

    #[test]
    fn peaks_follow_their_script_and_mute() {
        let mut audio = mock();
        let a = Target::Session(SessionId("a".into()));
        let b = Target::Session(SessionId("b".into()));
        let mut out = Vec::new();
        audio.peaks(&[a.clone(), b.clone()], &mut out).unwrap();
        assert_eq!(out, [0.1, 0.0]);
        audio.peaks(std::slice::from_ref(&a), &mut out).unwrap();
        assert_eq!(out, [0.5]);
        audio.peaks(std::slice::from_ref(&a), &mut out).unwrap();
        assert_eq!(out, [0.1], "the script loops");
        audio.set_mute(&a, true).unwrap();
        audio.peaks(&[a], &mut out).unwrap();
        assert_eq!(out, [0.0], "muted is silent");
    }

    #[test]
    fn the_default_device_changes_within_its_flow() {
        let mut audio = mock();
        let events = record(&mut audio);
        let headphones = DeviceId("headphones".into());
        audio.set_default_device(&headphones, Role::Console).unwrap();
        let mut devices = Vec::new();
        audio.devices(&mut devices).unwrap();
        assert_eq!(crate::default_device(&devices, Flow::Render).unwrap().id, headphones);
        assert_eq!(crate::default_device(&devices, Flow::Capture).unwrap().id.0, "mic", "inputs untouched");
        assert_eq!(*events.lock().unwrap(), [AudioEvent::DevicesChanged]);
    }

    #[test]
    fn the_demo_has_default_devices_with_sessions() {
        let mut audio = MockAudio::demo();
        let mut devices = Vec::new();
        audio.devices(&mut devices).unwrap();
        let default = crate::default_device(&devices, Flow::Render).unwrap().id.clone();
        assert!(crate::default_device(&devices, Flow::Capture).is_some());
        let mut sessions = Vec::new();
        audio.sessions(&default, &mut sessions).unwrap();
        assert!(sessions.len() >= 5);
        assert!(sessions.iter().any(|s| s.system_sounds));
    }
}
