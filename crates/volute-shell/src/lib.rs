//! Volute's desktop integration (docs/ARCHITECTURE.md, decision 0004): the tray icon, the
//! single instance and the settings file. No UI here; the app gives labels and callbacks.

#[cfg(windows)]
use std::sync::Arc;

#[cfg(windows)]
mod close_guard_windows;
#[cfg(windows)]
mod exe_windows;
#[cfg(windows)]
mod instance_windows;
pub mod settings_file;
#[cfg(windows)]
mod tray_windows;

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
    /// The mouse wheel over the icon: 120 per notch, positive away from the user (decision 0007).
    Wheel { delta: i32 },
}

/// Menu labels, given by the app so they follow its language.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayMenu {
    pub open: String,
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

    /// Shows the muted icon (the master output is muted) or the normal one.
    pub fn set_muted(&self, muted: bool) {
        #[cfg(windows)]
        self.inner.set_muted(muted);
        #[cfg(not(windows))]
        let _ = muted;
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
