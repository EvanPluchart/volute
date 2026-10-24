//! Sessions grouped into apps: one row per executable, its processes and sessions folded
//! (a browser has many), system sounds on their own (docs/ARCHITECTURE.md).

use std::path::Path;

use volute_audio::{Session, SessionId, SessionState};

/// What identifies an app across sessions and restarts.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AppKey {
    /// Windows' system sounds session.
    SystemSounds,
    /// An executable, by its path in lower case (Windows paths ignore case).
    Exe(String),
    /// A process whose image could not be read: only its PID tells it apart.
    Process(u32),
}

impl AppKey {
    /// A stable text form, for the UI's rows.
    pub fn to_text(&self) -> String {
        match self {
            AppKey::SystemSounds => "system".into(),
            AppKey::Exe(path) => format!("exe:{path}"),
            AppKey::Process(pid) => format!("pid:{pid}"),
        }
    }
}

/// One row of the mixer.
#[derive(Clone, Debug, PartialEq)]
pub struct App {
    pub key: AppKey,
    /// The display name the app gives, or its file name without extension. Empty for system
    /// sounds, which the UI names in its own language.
    pub name: String,
    /// The session scalar shown: the loudest session's.
    pub volume: f32,
    /// Every session is muted.
    pub muted: bool,
    /// At least one session is playing.
    pub playing: bool,
    pub sessions: Vec<SessionId>,
}

impl App {
    pub fn system_sounds(&self) -> bool {
        self.key == AppKey::SystemSounds
    }
}

/// The app a session belongs to.
pub fn key_of(session: &Session) -> AppKey {
    if session.system_sounds {
        AppKey::SystemSounds
    } else if let Some(exe) = &session.exe {
        AppKey::Exe(exe.to_string_lossy().to_lowercase())
    } else {
        AppKey::Process(session.pid)
    }
}

/// The name to show: the session's display name unless it is empty or an indirect resource
/// string ("@%SystemRoot%\...,-202"), else the executable's file stem.
fn name_of(session: &Session) -> String {
    if session.system_sounds {
        return String::new();
    }
    let display = session.display_name.trim();
    if !display.is_empty() && !display.starts_with('@') {
        return display.to_owned();
    }
    session
        .exe
        .as_deref()
        .and_then(Path::file_stem)
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| format!("PID {}", session.pid))
}

/// Groups `sessions` into apps into `out`: expired sessions left out, playing apps first, then
/// by name, system sounds last.
pub fn group(sessions: &[Session], out: &mut Vec<App>) {
    out.clear();
    for session in sessions {
        if session.state == SessionState::Expired {
            continue;
        }
        let key = key_of(session);
        let playing = session.state == SessionState::Active;
        match out.iter_mut().find(|a| a.key == key) {
            Some(app) => {
                // A playing session's name wins over a silent one's.
                if playing && !app.playing {
                    app.name = name_of(session);
                }
                app.volume = app.volume.max(session.volume);
                app.muted &= session.muted;
                app.playing |= playing;
                app.sessions.push(session.id.clone());
            }
            None => out.push(App {
                name: name_of(session),
                key,
                volume: session.volume,
                muted: session.muted,
                playing,
                sessions: vec![session.id.clone()],
            }),
        }
    }
    out.sort_by(|a, b| {
        a.system_sounds()
            .cmp(&b.system_sounds())
            .then(b.playing.cmp(&a.playing))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.key.cmp(&b.key))
    });
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn session(id: &str, pid: u32, exe: Option<&str>, state: SessionState) -> Session {
        Session {
            id: SessionId(id.into()),
            pid,
            exe: exe.map(PathBuf::from),
            display_name: String::new(),
            volume: 1.0,
            muted: false,
            state,
            system_sounds: false,
        }
    }

    fn names(apps: &[App]) -> Vec<&str> {
        apps.iter().map(|a| a.name.as_str()).collect()
    }

    #[test]
    fn one_row_per_executable() {
        let mut a = session("1", 10, Some(r"C:\Program Files\Firefox\firefox.exe"), SessionState::Active);
        a.volume = 0.4;
        let mut b = session("2", 11, Some(r"C:\PROGRAM FILES\FIREFOX\FIREFOX.EXE"), SessionState::Inactive);
        b.volume = 0.6;
        b.muted = true;
        let c = session("3", 12, Some(r"C:\Spotify\Spotify.exe"), SessionState::Active);
        let mut apps = Vec::new();
        group(&[a, b, c], &mut apps);
        assert_eq!(names(&apps), ["firefox", "Spotify"]);
        let firefox = &apps[0];
        assert_eq!(firefox.sessions.len(), 2, "both processes in one row, whatever the case");
        assert_eq!(firefox.volume, 0.6, "the loudest session");
        assert!(!firefox.muted, "muted only when every session is");
        assert!(firefox.playing);
    }

    #[test]
    fn playing_first_then_by_name_and_system_sounds_last() {
        let mut system = session("s", 0, None, SessionState::Active);
        system.system_sounds = true;
        system.display_name = "@%SystemRoot%\\System32\\AudioSrv.Dll,-202".into();
        let sessions = [
            system,
            session("1", 1, Some(r"C:\zoom.exe"), SessionState::Inactive),
            session("2", 2, Some(r"C:\b.exe"), SessionState::Active),
            session("3", 3, Some(r"C:\Apple.exe"), SessionState::Inactive),
            session("4", 4, Some(r"C:\c.exe"), SessionState::Expired),
        ];
        let mut apps = Vec::new();
        group(&sessions, &mut apps);
        assert_eq!(names(&apps), ["b", "Apple", "zoom", ""]);
        assert!(apps[3].system_sounds(), "system sounds keep their own row, last");
    }

    #[test]
    fn display_names_and_unreadable_images() {
        let mut named = session("1", 1, Some(r"C:\x\app.exe"), SessionState::Active);
        named.display_name = "My Player".into();
        let unreadable = session("2", 42, None, SessionState::Active);
        let other_unreadable = session("3", 43, None, SessionState::Active);
        let mut apps = Vec::new();
        group(&[named, unreadable, other_unreadable], &mut apps);
        assert_eq!(names(&apps), ["My Player", "PID 42", "PID 43"]);
        assert_eq!(apps[1].key, AppKey::Process(42));
        assert_eq!(apps[1].key.to_text(), "pid:42");
    }
}
