//! A scripted sound system: devices, sessions and peaks set up by the caller. Tests and
//! screenshots use it, so they never touch the volume of a real app.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::{
    Audio, Device, DeviceId, Error, MeterId, Result, Role, Session, SessionId, SessionState,
    Target,
};

#[derive(Default)]
pub struct MockAudio {
    devices: Vec<Device>,
    /// Sessions per device, in the order they were added.
    sessions: Vec<(DeviceId, Session)>,
    /// Peak sequences, each played in a loop: one value per call to `peaks`.
    peaks: HashMap<MeterId, (Vec<f32>, usize)>,
}

impl MockAudio {
    pub fn new() -> MockAudio {
        MockAudio::default()
    }

    /// Adds a device; the first one added is the default.
    pub fn with_device(mut self, id: &str, name: &str, volume: f32) -> MockAudio {
        self.devices.push(Device {
            id: DeviceId(id.into()),
            name: name.into(),
            volume,
            muted: false,
            default: self.devices.is_empty(),
        });
        self
    }

    pub fn with_session(mut self, device: &str, session: Session) -> MockAudio {
        self.sessions.push((DeviceId(device.into()), session));
        self
    }

    /// The peaks `meter` reports, one per call to `peaks`, in a loop.
    pub fn with_peaks(mut self, meter: MeterId, sequence: Vec<f32>) -> MockAudio {
        self.peaks.insert(meter, (sequence, 0));
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

    /// A believable machine for screenshots and `--mock`: speakers and headphones, a music
    /// player, a browser with two sessions, a call app, a silent game and system sounds.
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
    }

    fn session_mut(&mut self, id: &SessionId) -> Result<&mut Session> {
        self.sessions
            .iter_mut()
            .map(|(_, s)| s)
            .find(|s| &s.id == id)
            .ok_or_else(|| Error::NotFound(format!("session {}", id.0)))
    }

    fn device_mut(&mut self, id: &DeviceId) -> Result<&mut Device> {
        self.devices
            .iter_mut()
            .find(|d| &d.id == id)
            .ok_or_else(|| Error::NotFound(format!("device {}", id.0)))
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
    fn devices(&mut self, out: &mut Vec<Device>) -> Result<()> {
        out.clear();
        out.extend(self.devices.iter().cloned());
        Ok(())
    }

    fn sessions(&mut self, device: &DeviceId, out: &mut Vec<Session>) -> Result<()> {
        if !self.devices.iter().any(|d| &d.id == device) {
            return Err(Error::NotFound(format!("device {}", device.0)));
        }
        out.clear();
        out.extend(
            self.sessions
                .iter()
                .filter(|(d, _)| d == device)
                .map(|(_, s)| s.clone()),
        );
        Ok(())
    }

    fn peaks(&mut self, ids: &[MeterId], out: &mut Vec<f32>) -> Result<()> {
        out.clear();
        for id in ids {
            let muted = match id {
                Target::Session(s) => self.sessions.iter().any(|(_, x)| &x.id == s && x.muted),
                Target::Device(d) => self.devices.iter().any(|x| &x.id == d && x.muted),
            };
            let peak = match self.peaks.get_mut(id) {
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
        match target {
            Target::Session(id) => self.session_mut(id)?.volume = volume,
            Target::Device(id) => self.device_mut(id)?.volume = volume,
        }
        Ok(())
    }

    fn set_mute(&mut self, target: &Target, muted: bool) -> Result<()> {
        match target {
            Target::Session(id) => self.session_mut(id)?.muted = muted,
            Target::Device(id) => self.device_mut(id)?.muted = muted,
        }
        Ok(())
    }

    fn set_default_device(&mut self, device: &DeviceId, _role: Role) -> Result<()> {
        if !self.devices.iter().any(|d| &d.id == device) {
            return Err(Error::NotFound(format!("device {}", device.0)));
        }
        for d in &mut self.devices {
            d.default = &d.id == device;
        }
        Ok(())
    }
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
            .with_session("speakers", MockAudio::session("a", 1, r"C:\a.exe"))
            .with_session("speakers", MockAudio::session("b", 2, r"C:\b.exe"))
            .with_session("headphones", MockAudio::session("c", 3, r"C:\c.exe"))
            .with_peaks(Target::Session(SessionId("a".into())), vec![0.1, 0.5])
    }

    #[test]
    fn lists_devices_and_the_sessions_of_one() {
        let mut audio = mock();
        let mut devices = Vec::new();
        audio.devices(&mut devices).unwrap();
        assert_eq!(devices.len(), 2);
        assert_eq!(crate::default_device(&devices).unwrap().id, speakers());
        let mut sessions = Vec::new();
        audio.sessions(&speakers(), &mut sessions).unwrap();
        let ids: Vec<&str> = sessions.iter().map(|s| s.id.0.as_str()).collect();
        assert_eq!(ids, ["a", "b"]);
        assert!(audio.sessions(&DeviceId("gone".into()), &mut sessions).is_err());
    }

    #[test]
    fn volume_and_mute_round_trip() {
        let mut audio = mock();
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
        // Out of range is clamped, unknown targets are errors.
        audio.set_volume(&a, 3.0).unwrap();
        audio.sessions(&speakers(), &mut sessions).unwrap();
        assert_eq!(sessions[0].volume, 1.0);
        let gone = Target::Session(SessionId("gone".into()));
        assert!(matches!(audio.set_volume(&gone, 0.5), Err(Error::NotFound(_))));
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
    fn the_default_device_can_change() {
        let mut audio = mock();
        let headphones = DeviceId("headphones".into());
        audio.set_default_device(&headphones, Role::Console).unwrap();
        let mut devices = Vec::new();
        audio.devices(&mut devices).unwrap();
        assert_eq!(crate::default_device(&devices).unwrap().id, headphones);
    }

    #[test]
    fn the_demo_has_a_default_device_with_sessions() {
        let mut audio = MockAudio::demo();
        let mut devices = Vec::new();
        audio.devices(&mut devices).unwrap();
        let default = crate::default_device(&devices).unwrap().id.clone();
        let mut sessions = Vec::new();
        audio.sessions(&default, &mut sessions).unwrap();
        assert!(sessions.len() >= 5);
        assert!(sessions.iter().any(|s| s.system_sounds));
    }
}
