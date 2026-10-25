//! Volute's desktop integration (docs/ARCHITECTURE.md, decisions 0004 and 0005): the tray icon,
//! the single instance, the settings file, starting with Windows and updates. No UI here; the
//! app gives labels and callbacks.

#[cfg(windows)]
use std::sync::Arc;

#[cfg(windows)]
mod close_guard_windows;
#[cfg(windows)]
mod desktop_windows;
#[cfg(windows)]
mod exe_windows;
#[cfg(windows)]
mod instance_windows;
pub mod settings_file;
#[cfg(windows)]
mod startup_windows;
#[cfg(windows)]
mod tray_windows;
#[cfg(windows)]
mod update_windows;

/// An icon as straight RGBA pixels, row by row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Icon {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// What an executable file says about itself: its icon and names, read from its resources.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExeInfo {
    pub icon: Option<Icon>,
    pub publisher: Option<String>,
    /// A readable name, such as "Brave Browser" for brave.exe.
    pub description: Option<String>,
}

/// The icon (the size closest to `size` physical pixels) and names of an executable. Reads the
/// file's own resources, no shell API; nothing for a file on a network share.
pub fn exe_info(exe: &std::path::Path, size: u32) -> ExeInfo {
    #[cfg(windows)]
    return exe_windows::exe_info(exe, size);
    #[cfg(not(windows))]
    {
        let _ = (exe, size);
        ExeInfo::default()
    }
}

/// Held while this process is the running Volute.
pub struct Instance {
    #[cfg(windows)]
    _inner: instance_windows::Instance,
}

/// A later launch asking for the window. Call `done` from the UI thread once the window is
/// shown: a launch that gets no answer starts a Volute of its own, in case this one hangs.
#[derive(Debug)]
pub struct ShowRequest {
    #[cfg(windows)]
    answer: std::sync::mpsc::Sender<()>,
}

impl ShowRequest {
    pub fn done(self) {
        #[cfg(windows)]
        instance_windows::confirm_shown(self.answer);
    }
}

/// Makes this process the single running Volute: `Some` when it is the first, and `on_show`
/// then runs (on another thread) whenever a later launch wants the window shown. `None` when
/// Volute already runs and showed itself: this process should exit. `Some` too when the running
/// Volute did not answer within 2.5 s.
pub fn single_instance(on_show: impl Fn(ShowRequest) + Send + Sync + 'static) -> Option<Instance> {
    #[cfg(windows)]
    {
        instance_windows::acquire(instance_windows::VOLUTE, Arc::new(on_show))
            .map(|inner| Instance { _inner: inner })
    }
    #[cfg(not(windows))]
    {
        let _ = on_show;
        Some(Instance {})
    }
}

/// Stops other programs' close requests (`taskkill` without `/F`) from destroying the UI
/// toolkit's hidden message window, which would cut the tray and a second launch off from the
/// UI thread. Call on the UI thread once the event loop exists; returns how many windows it
/// guards (1 on Windows).
pub fn guard_ui_thread() -> usize {
    #[cfg(windows)]
    return close_guard_windows::guard_event_target();
    #[cfg(not(windows))]
    0
}

/// What happens on the tray icon, reported to the callback given to `Tray::new`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayEvent {
    /// Left click on the icon, at this screen position (physical pixels).
    Clicked { x: i32, y: i32 },
    /// "Open Volute" in the menu, or a double click.
    Open,
    /// "Quit" in the menu, or Windows is signing out.
    Quit,
    /// "Mute all" in the menu: mute or unmute the master output.
    ToggleMute,
    /// "Settings" in the menu.
    Settings,
    /// A click on a notification the icon showed.
    NotificationClicked,
    /// The mouse wheel over the icon: 120 per notch, positive away from the user (decision 0007).
    Wheel { delta: i32 },
}

/// Menu labels, given by the app so they follow its language.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayMenu {
    pub open: String,
    /// "Mute all", checked while the master output is muted.
    pub mute: String,
    pub settings: String,
    pub quit: String,
}

/// The tray icon. It disappears when dropped.
pub struct Tray {
    #[cfg(windows)]
    inner: tray_windows::Tray,
}

impl Tray {
    /// Adds the icon, with `tooltip`. `on_event` runs on the tray's own thread. `None` where the
    /// system has no tray Volute supports.
    pub fn new(
        tooltip: &str,
        menu: TrayMenu,
        on_event: impl Fn(TrayEvent) + Send + Sync + 'static,
    ) -> Option<Tray> {
        #[cfg(windows)]
        {
            tray_windows::Tray::new(tooltip, menu, Arc::new(on_event)).map(|inner| Tray { inner })
        }
        #[cfg(not(windows))]
        {
            let _ = (tooltip, menu, on_event);
            None
        }
    }

    /// Whether the icon is on screen now (not while Explorer is down or restarting).
    pub fn is_shown(&self) -> bool {
        #[cfg(windows)]
        return self.inner.is_shown();
        #[cfg(not(windows))]
        false
    }

    /// Updates the hover text (truncated to what the system shows).
    pub fn set_tooltip(&self, tooltip: &str) {
        #[cfg(windows)]
        self.inner.set_tooltip(tooltip);
        #[cfg(not(windows))]
        let _ = tooltip;
    }

    /// Shows a notification from the icon (a toast on Windows 10 and 11).
    pub fn notify(&self, title: &str, body: &str) {
        #[cfg(windows)]
        self.inner.notify(title, body);
        #[cfg(not(windows))]
        let _ = (title, body);
    }

    /// Shows the muted icon (the master output is muted) or the normal one.
    pub fn set_muted(&self, muted: bool) {
        #[cfg(windows)]
        self.inner.set_muted(muted);
        #[cfg(not(windows))]
        let _ = muted;
    }
}

/// Opens an `https:` page in the default browser; anything else is refused.
pub fn open_url(url: &str) -> bool {
    if !url.starts_with("https://") {
        return false;
    }
    #[cfg(windows)]
    return desktop_windows::open_url(url);
    #[cfg(not(windows))]
    false
}

/// Starting Volute when the user signs in (decision 0004): the value the installer sets.
pub mod startup {
    /// Whether Windows starts this Volute at sign-in. A value naming another copy counts as
    /// off: turning it on points it here.
    pub fn enabled() -> bool {
        #[cfg(windows)]
        {
            let Ok(exe) = std::env::current_exe() else { return false };
            crate::startup_windows::command().is_some_and(|command| {
                command.trim().trim_matches('"').eq_ignore_ascii_case(&exe.to_string_lossy())
            })
        }
        #[cfg(not(windows))]
        false
    }

    /// Starts this executable at sign-in, or stops starting Volute. `false` if the change failed.
    pub fn set(on: bool) -> bool {
        #[cfg(windows)]
        {
            if on {
                std::env::current_exe().is_ok_and(|exe| crate::startup_windows::set(Some(&exe)))
            } else {
                crate::startup_windows::set(None)
            }
        }
        #[cfg(not(windows))]
        {
            let _ = on;
            false
        }
    }
}

/// Updates from the app (decision 0005): the network and file steps. Which version is newer
/// and which checksum to expect is `volute-core::update`'s part.
pub mod update {
    use std::path::{Path, PathBuf};

    /// How this copy of Volute was installed.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Install {
        /// By the installer, for this account: it can update itself without a prompt.
        PerUser,
        /// By the installer, for every account (Program Files).
        AllUsers,
        /// Not by the installer (the zip, or a build).
        Portable,
    }

    /// Whether this system can check for and install updates.
    pub fn supported() -> bool {
        cfg!(windows)
    }

    /// Where `url` redirects to, without following it.
    pub fn redirect_of(url: &str) -> Result<String, String> {
        #[cfg(windows)]
        return crate::update_windows::redirect_of(url);
        #[cfg(not(windows))]
        {
            let _ = url;
            Err("not supported on this system".into())
        }
    }

    /// The body of `url` as text (redirects followed).
    pub fn text(url: &str) -> Result<String, String> {
        #[cfg(windows)]
        return crate::update_windows::text(url);
        #[cfg(not(windows))]
        {
            let _ = url;
            Err("not supported on this system".into())
        }
    }

    /// Saves the body of `url` (redirects followed) to `path`.
    pub fn download(url: &str, path: &Path) -> Result<(), String> {
        #[cfg(windows)]
        return crate::update_windows::download(url, path);
        #[cfg(not(windows))]
        {
            let _ = (url, path);
            Err("not supported on this system".into())
        }
    }

    /// The SHA-256 of `data`.
    pub fn sha256(data: &[u8]) -> Option<[u8; 32]> {
        #[cfg(windows)]
        return crate::update_windows::sha256(data);
        #[cfg(not(windows))]
        {
            let _ = data;
            None
        }
    }

    /// Whether `signature` is the release key's ECDSA P-256 signature of `data` (`public`:
    /// the key's point, x then y; `signature`: r then s).
    pub fn verify_signature(public: &[u8; 64], data: &[u8], signature: &[u8; 64]) -> bool {
        #[cfg(windows)]
        return sha256(data).is_some_and(|hash| crate::update_windows::verify_p256(public, &hash, signature));
        #[cfg(not(windows))]
        {
            let _ = (public, data, signature);
            false
        }
    }

    /// A downloaded installer whose checksum matched, held open so that nothing can replace it
    /// before it runs: others may read it, never write or delete it.
    #[derive(Debug)]
    pub struct Checked {
        path: PathBuf,
        _held: std::fs::File,
    }

    impl Checked {
        pub fn path(&self) -> &Path {
            &self.path
        }
    }

    /// Opens `path` so that nobody else can change it, and checks its SHA-256 from that handle.
    pub fn open_checked(path: &Path, expected: [u8; 32]) -> Option<Checked> {
        use std::io::Read;
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            // FILE_SHARE_READ only.
            options.share_mode(1);
        }
        let mut file = options.open(path).ok()?;
        let mut data = Vec::new();
        file.read_to_end(&mut data).ok()?;
        (sha256(&data)? == expected).then(|| Checked { path: path.to_path_buf(), _held: file })
    }

    /// This process runs with administrator rights: it must never run a downloaded file.
    pub fn is_elevated() -> bool {
        #[cfg(windows)]
        return crate::update_windows::is_elevated();
        #[cfg(not(windows))]
        false
    }

    /// Which install this process runs from.
    pub fn install_kind() -> Install {
        #[cfg(windows)]
        return crate::update_windows::install_kind();
        #[cfg(not(windows))]
        Install::Portable
    }

    /// Where downloads go: `%LOCALAPPDATA%\Volute\Updates`, which only this user can write to,
    /// unlike a `TEMP` pointed at a shared folder.
    fn downloads() -> Option<PathBuf> {
        let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).filter(|p| p.is_absolute())?;
        Some(base.join("Volute").join("Updates"))
    }

    /// A new, empty folder for one download.
    pub fn fresh_folder() -> Option<PathBuf> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_nanos();
        let base = downloads()?;
        std::fs::create_dir_all(&base).ok()?;
        let dir = base.join(format!("{}-{stamp}", std::process::id()));
        // `create_dir` fails if it exists: the folder is ours alone.
        std::fs::create_dir(&dir).ok()?;
        Some(dir)
    }

    /// Removes the downloads of earlier updates: the installer that ran from there is done.
    pub fn clean_downloads() {
        if let Some(base) = downloads()
            && let Ok(entries) = std::fs::read_dir(&base)
        {
            for entry in entries.flatten() {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }

    /// Starts the checked installer with `args`, detached, by its full path; this process should
    /// then quit. The file stays held until the installer has started.
    pub fn start_installer(installer: Checked, args: &[&str]) -> bool {
        let path = installer.path();
        path.is_absolute() && std::process::Command::new(path).args(args).spawn().is_ok()
    }
}

/// A rectangle in physical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// The work area (the screen minus the taskbar) of the monitor holding (`x`, `y`).
pub fn work_area_at(x: i32, y: i32) -> Option<Rect> {
    #[cfg(windows)]
    return tray_windows::work_area(x, y);
    #[cfg(not(windows))]
    {
        let _ = (x, y);
        None
    }
}

/// Where to put a `width` × `height` window opened from the tray at screen point (`x`, `y`):
/// above the point (below it when the taskbar is at the top), inside the work area of the
/// monitor that holds it, so the taskbar is never covered.
pub fn place_near(x: i32, y: i32, width: i32, height: i32) -> (i32, i32) {
    #[cfg(windows)]
    let area = tray_windows::work_area(x, y);
    #[cfg(not(windows))]
    let area: Option<Rect> = None;
    match area {
        Some(area) => place_in(area, x, y, width, height),
        None => (x, y),
    }
}

/// The window centred on the click, on the side of the work area the click is closest to,
/// then moved fully inside the area.
fn place_in(area: Rect, x: i32, y: i32, width: i32, height: i32) -> (i32, i32) {
    const GAP: i32 = 12;
    let below = y < area.y + area.height / 2;
    let y = if below { y + GAP } else { y - height - GAP };
    clamp_into(area, x - width / 2, y, width, height)
}

/// Moves a rectangle at (`x`, `y`) into `area`.
fn clamp_into(area: Rect, x: i32, y: i32, width: i32, height: i32) -> (i32, i32) {
    let margin = 12;
    let x = x.clamp(area.x + margin, (area.x + area.width - width - margin).max(area.x + margin));
    let y = y.clamp(area.y + margin, (area.y + area.height - height - margin).max(area.y + margin));
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect { x: 0, y: 0, width: 1920, height: 1032 };

    #[test]
    fn opens_above_a_bottom_taskbar_inside_the_work_area() {
        // A click on the tray at the bottom right: above it, kept off the screen edge.
        assert_eq!(place_in(AREA, 1850, 1060, 360, 500), (1548, 520));
    }

    #[test]
    fn opens_below_a_top_taskbar() {
        let area = Rect { y: 48, height: 1032, ..AREA };
        assert_eq!(place_in(area, 1850, 20, 360, 500), (1548, 60));
    }

    #[test]
    fn side_taskbars_and_oversized_windows_stay_inside() {
        // Taskbar on the left: the work area starts at x = 64; a click at x = 30.
        let area = Rect { x: 64, width: 1856, ..AREA };
        assert_eq!(place_in(area, 30, 900, 360, 500), (76, 388));
        // Larger than the area (a high-DPI panel on a small screen): pinned, never a panic.
        assert_eq!(clamp_into(AREA, 500, 500, 4000, 3000), (12, 12));
    }
}
