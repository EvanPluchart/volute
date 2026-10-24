//! Volute's desktop integration (docs/ARCHITECTURE.md, decision 0004): the tray icon, the
//! single instance and the settings file. No UI here; the app gives labels and callbacks.

#[cfg(windows)]
use std::sync::Arc;

#[cfg(windows)]
mod close_guard_windows;
#[cfg(windows)]
mod instance_windows;
pub mod settings_file;
#[cfg(windows)]
mod tray_windows;

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
}
