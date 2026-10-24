//! Where the settings file lives, and reading and writing it (decision 0004). The format is
//! `volute_core::settings`; this module only moves text.

use std::path::PathBuf;

/// `%APPDATA%\Volute\settings.ini` on Windows, `~/Library/Application Support/Volute` on macOS,
/// `$XDG_CONFIG_HOME/volute` (or `~/.config/volute`) elsewhere. `None` when the system says
/// nothing about where configuration goes.
pub fn path() -> Option<PathBuf> {
    let dir = if cfg!(windows) {
        PathBuf::from(std::env::var_os("APPDATA")?).join("Volute")
    } else if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support/Volute")
    } else {
        match std::env::var_os("XDG_CONFIG_HOME") {
            Some(dir) => PathBuf::from(dir).join("volute"),
            None => PathBuf::from(std::env::var_os("HOME")?).join(".config/volute"),
        }
    };
    Some(dir.join("settings.ini"))
}

/// The file's text, or `None` when there is none yet (or it cannot be read).
pub fn load() -> Option<String> {
    std::fs::read_to_string(path()?).ok()
}

/// Writes the file atomically: a temporary file next to it, then a rename, so a crash never
/// leaves half a file.
pub fn save(text: &str) -> std::io::Result<()> {
    let path = path().ok_or_else(|| std::io::Error::other("no configuration folder"))?;
    save_to(&path, text)
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
        assert!(path.ends_with("settings.ini"));
    }
}
