//! Where Volute's files live, and reading and writing them (decision 0004): `settings.ini`
//! (format in `volute_core::settings`) and `volumes.ini` (`volute_core::memory`). This module
//! only moves text.

use std::path::PathBuf;

/// The settings file.
pub const SETTINGS: &str = "settings.ini";
/// The volumes remembered per app.
pub const VOLUMES: &str = "volumes.ini";

/// `%APPDATA%\Volute`. `None` when the system says nothing about where configuration goes.
pub fn dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join("Volute"))
}

/// The settings file's path.
pub fn path() -> Option<PathBuf> {
    Some(dir()?.join(SETTINGS))
}

/// The text of the file `name` in Volute's folder, or `None` when there is none yet (or it
/// cannot be read).
pub fn load_named(name: &str) -> Option<String> {
    std::fs::read_to_string(dir()?.join(name)).ok()
}

/// The settings file's text.
pub fn load() -> Option<String> {
    load_named(SETTINGS)
}

/// Writes the file `name` in Volute's folder atomically: a temporary file next to it, then a
/// rename, so a crash never leaves half a file.
pub fn save_named(name: &str, text: &str) -> std::io::Result<()> {
    let dir = dir().ok_or_else(|| std::io::Error::other("no configuration folder"))?;
    save_to(&dir.join(name), text)
}

/// Writes the settings file.
pub fn save(text: &str) -> std::io::Result<()> {
    save_named(SETTINGS, text)
}

fn save_to(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let temporary = path.with_extension("ini.tmp");
    std::fs::write(&temporary, text)?;
    std::fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_atomically_and_replaces() {
        let dir = std::env::temp_dir().join(format!("volute-settings-{}", std::process::id()));
        let path = dir.join("nested").join("settings.ini");
        save_to(&path, "a = 1\n").unwrap();
        save_to(&path, "a = 2\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "a = 2\n");
        assert!(!path.with_extension("ini.tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_path_is_under_the_user_profile() {
        let path = path().expect("a configuration folder");
        assert!(path.ends_with("Volute\\settings.ini"));
    }
}
