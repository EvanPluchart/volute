//! Volumes remembered per app (docs/ROADMAP.md, M1).
//!
//! Windows already remembers a volume per executable most of the time. Volute only remembers
//! what the user set in Volute, and applies it only when a new session of that executable
//! appears with a different value: it never fights a change made in Windows' own mixer while
//! the app plays.
//!
//! The file is one line per app: `<slider position> <muted 0|1> <executable path>`, the path last
//! so that any character it holds (spaces, `=`) needs no escaping. Lines that do not parse are
//! dropped; nothing here touches the disk.

use std::collections::BTreeMap;

use crate::apps::AppKey;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Remembered {
    /// Slider position, 0.0 to 1.0.
    pub position: f32,
    pub muted: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Memory {
    /// By executable path in lower case (AppKey::Exe).
    apps: BTreeMap<String, Remembered>,
}

impl Memory {
    pub fn parse(text: &str) -> Memory {
        let mut apps = BTreeMap::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.splitn(3, ' ');
            let (Some(position), Some(muted), Some(path)) = (parts.next(), parts.next(), parts.next()) else {
                continue;
            };
            let (Ok(position), Some(muted)) = (position.parse::<f32>(), match muted {
                "0" => Some(false),
                "1" => Some(true),
                _ => None,
            }) else {
                continue;
            };
            if !(0.0..=1.0).contains(&position) || path.trim().is_empty() {
                continue;
            }
            apps.insert(path.trim().to_lowercase(), Remembered { position, muted });
        }
        Memory { apps }
    }

    pub fn to_text(&self) -> String {
        let mut text = String::from("# Volumes set in Volute, per app: slider position, muted, executable\n");
        for (path, r) in &self.apps {
            text.push_str(&format!("{:.3} {} {}\n", r.position, u8::from(r.muted), path));
        }
        text
    }

    /// The user set this app's volume or mute in Volute. `false` when nothing changed (no
    /// need to save) or the app has no executable to remember it by.
    pub fn remember(&mut self, key: &AppKey, value: Remembered) -> bool {
        let AppKey::Exe(path) = key else {
            return false;
        };
        let value = Remembered {
            position: (value.position * 1000.0).round() / 1000.0,
            ..value
        };
        self.apps.insert(path.clone(), value) != Some(value)
    }

    pub fn get(&self, key: &AppKey) -> Option<Remembered> {
        match key {
            AppKey::Exe(path) => self.apps.get(path).copied(),
            _ => None,
        }
    }

    /// What to apply to a session of `key` that just appeared, showing `position` and `muted`:
    /// the remembered value when it differs, `None` when it matches or nothing is remembered.
    pub fn to_apply(&self, key: &AppKey, position: f32, muted: bool) -> Option<Remembered> {
        let r = self.get(key)?;
        ((r.position - position).abs() > 0.005 || r.muted != muted).then_some(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exe(path: &str) -> AppKey {
        AppKey::Exe(path.to_lowercase())
    }

    #[test]
    fn round_trips_paths_with_spaces_and_equals() {
        let mut memory = Memory::default();
        let odd = exe(r"C:\Program Files\A=B Player\player.exe");
        assert!(memory.remember(&odd, Remembered { position: 0.42, muted: true }));
        assert!(memory.remember(&exe(r"C:\x.exe"), Remembered { position: 1.0, muted: false }));
        let back = Memory::parse(&memory.to_text());
        assert_eq!(back, memory);
        assert_eq!(back.get(&odd), Some(Remembered { position: 0.42, muted: true }));
    }

    #[test]
    fn bad_lines_are_dropped() {
        let memory = Memory::parse("0.5 1 C:\\ok.exe\nnonsense\n2.0 0 C:\\too-loud.exe\n0.3 x C:\\bad.exe\n0.4 0 \n");
        assert_eq!(memory.apps.len(), 1);
        assert!(memory.get(&exe("C:\\OK.EXE")).is_some(), "paths ignore case");
    }

    #[test]
    fn only_executables_are_remembered_and_unchanged_values_need_no_save() {
        let mut memory = Memory::default();
        assert!(!memory.remember(&AppKey::SystemSounds, Remembered { position: 0.5, muted: false }));
        assert!(!memory.remember(&AppKey::Process(4), Remembered { position: 0.5, muted: false }));
        let key = exe(r"C:\a.exe");
        assert!(memory.remember(&key, Remembered { position: 0.5, muted: false }));
        assert!(!memory.remember(&key, Remembered { position: 0.5, muted: false }), "same value");
    }

    #[test]
    fn applies_only_what_differs() {
        let mut memory = Memory::default();
        let key = exe(r"C:\a.exe");
        memory.remember(&key, Remembered { position: 0.3, muted: false });
        assert_eq!(memory.to_apply(&key, 0.3, false), None, "already right");
        assert_eq!(memory.to_apply(&key, 1.0, false), Some(Remembered { position: 0.3, muted: false }));
        assert_eq!(memory.to_apply(&key, 0.3, true), Some(Remembered { position: 0.3, muted: false }));
        assert_eq!(memory.to_apply(&exe(r"C:\b.exe"), 1.0, false), None, "nothing remembered");
    }
}
