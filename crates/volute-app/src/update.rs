//! Updates from the app (decision 0005), as Vigie and Recto do them. Checks run on a thread of
//! their own, 3 s after start and every 6 hours; the result goes to the UI thread, which shows
//! it in the flyout and in Settings and installs on demand.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::Thread;
use std::time::{Duration, Instant};

use volute_core::update::{self, Version};
use volute_shell::update::Install;

use crate::ui;

/// `Updates.state` in `ui/settings.slint`.
pub const CHECKING: i32 = 1;
pub const UP_TO_DATE: i32 = 2;
pub const AVAILABLE: i32 = 3;
pub const DOWNLOADING: i32 = 4;
pub const FAILED: i32 = 5;
pub const INSTALLING: i32 = 6;
pub const INSTALLED: i32 = 7;

/// `Updates.mode`.
pub const INSTALL: i32 = 0;
pub const RELEASE_PAGE: i32 = 1;
pub const ELEVATED: i32 = 2;
pub const UNSUPPORTED: i32 = 3;

/// `Updates.error-kind`: what went wrong, worded in the UI.
const CHECK_FAILED: i32 = 1;
const NO_CHECKSUM: i32 = 2;
const MISMATCH: i32 = 3;
const DOWNLOAD_FAILED: i32 = 4;
const NOT_STARTED: i32 = 5;
const TEST_RUN: i32 = 6;

/// The first check, once the UI is up: an update shows when the user first opens Volute.
pub const FIRST_CHECK: Duration = Duration::from_secs(3);
const EVERY: Duration = Duration::from_secs(6 * 3600);

/// No window at all, no prompts, no reboot, and the installer never closes a running Volute
/// (it waits for this one to quit); then it starts Volute again (`volute.iss`).
const INSTALLER_ARGS: [&str; 5] =
    ["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/NOCLOSEAPPLICATIONS", "/relaunch=1"];

/// An error to show: its kind (worded by the UI) and a technical detail.
type Failure = (i32, String);

/// What the UI shows of the updater.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct View {
    pub state: i32,
    pub version: String,
    pub mode: i32,
    pub error_kind: i32,
    pub error: String,
}

struct Shared {
    enabled: AtomicBool,
    /// "Check now" was clicked.
    asked: AtomicBool,
    /// A download or an install is under way.
    busy: AtomicBool,
}

/// The checking thread, and what the UI tells it.
pub struct Updater {
    thread: Thread,
    shared: Arc<Shared>,
    /// A test run downloads and checks, and never runs the installer.
    test_run: bool,
}

thread_local! {
    /// The single instance, released just before the installer starts (it waits for that).
    static INSTANCE: RefCell<Option<volute_shell::Instance>> = const { RefCell::new(None) };
}

/// Keeps the single instance until an update or the end of the process releases it.
pub fn hold_instance(instance: Option<volute_shell::Instance>) {
    INSTANCE.with_borrow_mut(|held| *held = instance);
}

/// Releases the single instance: a launch from now on starts a new Volute.
pub fn release_instance() {
    INSTANCE.with_borrow_mut(|held| *held = None);
}

/// What the button can do in this process.
pub fn mode() -> i32 {
    if !volute_shell::update::supported() {
        UNSUPPORTED
    } else if volute_shell::update::is_elevated() {
        // A downloaded file never runs with administrator rights (decision 0005).
        ELEVATED
    } else if volute_shell::update::install_kind() == Install::PerUser {
        INSTALL
    } else {
        RELEASE_PAGE
    }
}

/// Starts checking: the first check after `first`, then every 6 hours while `enabled`.
/// `current` is this Volute's version (`--pretend-version` may give an older one).
pub fn start(enabled: bool, current: String, first: Duration, test_run: bool) -> Option<Updater> {
    let shared = Arc::new(Shared {
        enabled: AtomicBool::new(enabled),
        asked: AtomicBool::new(false),
        busy: AtomicBool::new(false),
    });
    let thread_shared = shared.clone();
    let thread = std::thread::Builder::new()
        .name("volute-update".into())
        .spawn(move || run(&thread_shared, &current, first))
        .ok()?
        .thread()
        .clone();
    Some(Updater { thread, shared, test_run })
}

fn run(shared: &Shared, current: &str, first: Duration) {
    let mut next = Instant::now() + first;
    // The version a notification already announced: once per version.
    let mut announced = None;
    loop {
        let asked = shared.asked.swap(false, Ordering::Relaxed);
        let due = shared.enabled.load(Ordering::Relaxed) && Instant::now() >= next;
        if (asked || due) && !shared.busy.load(Ordering::Relaxed) {
            check(current, asked, &mut announced);
            next = Instant::now() + EVERY;
        }
        // Woken early by "Check now" or by the setting turned on. The thread lives as long as
        // the process: it holds nothing, and quitting does not wait for it.
        let wait = next.saturating_duration_since(Instant::now());
        std::thread::park_timeout(wait.max(Duration::from_secs(1)));
    }
}

/// One check. A background check that fails stays quiet (being offline is normal); one the
/// user asked for says why.
fn check(current: &str, asked: bool, announced: &mut Option<Version>) {
    if asked {
        post(CHECKING, None, (0, String::new()));
    }
    let latest = volute_shell::update::redirect_of(update::LATEST_URL)
        .and_then(|to| Version::from_release_url(&to).ok_or_else(|| format!("unexpected address {to}")));
    match latest {
        Ok(latest) => match update::newer(current, latest) {
            Some(newer) => {
                post(AVAILABLE, Some(newer.to_string()), (0, String::new()));
                // A notification, once per version, unless the user is looking already.
                if !asked && *announced != Some(newer) {
                    *announced = Some(newer);
                    let version = newer.to_string();
                    let _ = slint::invoke_from_event_loop(move || ui::notify_update(&version));
                }
            }
            None => post(UP_TO_DATE, None, (0, String::new())),
        },
        Err(error) if asked => post(FAILED, None, (CHECK_FAILED, error)),
        Err(_) => {}
    }
}

/// Sends a state to the UI thread.
fn post(state: i32, version: Option<String>, (kind, detail): Failure) {
    let _ = slint::invoke_from_event_loop(move || {
        ui::set_update(|view| {
            // A download under way keeps its state.
            if matches!(view.state, DOWNLOADING | INSTALLING) {
                return;
            }
            view.state = state;
            if let Some(version) = version {
                view.version = version;
            }
            view.error_kind = kind;
            view.error = detail;
        });
    });
}

impl Updater {
    /// The setting changed.
    pub fn set_enabled(&self, on: bool) {
        if !self.shared.enabled.swap(on, Ordering::Relaxed) && on {
            self.thread.unpark();
        }
    }

    /// "Check now".
    pub fn check_now(&self) {
        self.shared.asked.store(true, Ordering::Relaxed);
        self.thread.unpark();
    }

    /// "Update", on the UI thread: installs the version offered, or opens its release page.
    pub fn update(&self, view: &View) {
        let Some(version) = Version::parse(&view.version) else {
            return;
        };
        match view.mode {
            RELEASE_PAGE | ELEVATED => {
                volute_shell::open_url(&update::release_page(version));
            }
            INSTALL if !self.shared.busy.swap(true, Ordering::Relaxed) => {
                ui::set_update(|view| view.state = DOWNLOADING);
                let (shared, test_run) = (self.shared.clone(), self.test_run);
                let started = std::thread::Builder::new().name("volute-download".into()).spawn(move || {
                    let result = fetch(version);
                    let _ = slint::invoke_from_event_loop(move || {
                        install(result, test_run);
                        shared.busy.store(false, Ordering::Relaxed);
                    });
                });
                if started.is_err() {
                    self.shared.busy.store(false, Ordering::Relaxed);
                    fail((DOWNLOAD_FAILED, String::new()));
                }
            }
            _ => {}
        }
    }
}

fn fail((kind, detail): Failure) {
    ui::set_update(|view| {
        view.state = FAILED;
        view.error_kind = kind;
        view.error = detail;
    });
}

/// Downloads the installer of `version` into a new folder and checks it against the release's
/// `SHA256SUMS`. A missing checksum refuses the update.
fn fetch(version: Version) -> Result<PathBuf, Failure> {
    let name = version.setup_name();
    let sums = volute_shell::update::text(&version.download_url("SHA256SUMS")).map_err(|e| (NO_CHECKSUM, e))?;
    let expected = update::checksum_for(&sums, &name).ok_or_else(|| (NO_CHECKSUM, name.clone()))?;
    let folder = volute_shell::update::fresh_folder()
        .ok_or_else(|| (DOWNLOAD_FAILED, std::env::temp_dir().display().to_string()))?;
    let path = folder.join(&name);
    let checked = volute_shell::update::download(&version.download_url(&name), &path)
        .map_err(|e| (DOWNLOAD_FAILED, e))
        .and_then(|()| {
            (volute_shell::update::sha256_file(&path) == Some(expected))
                .then_some(())
                .ok_or((MISMATCH, String::new()))
        });
    if let Err(failure) = checked {
        let _ = std::fs::remove_dir_all(&folder);
        return Err(failure);
    }
    Ok(path)
}

/// On the UI thread: runs the checked installer, then quits so it can replace Volute.
fn install(result: Result<PathBuf, Failure>, test_run: bool) {
    let path = match result {
        Ok(path) => path,
        Err(failure) => return fail(failure),
    };
    if test_run {
        return fail((TEST_RUN, path.display().to_string()));
    }
    ui::set_update(|view| view.state = INSTALLING);
    // The installer waits until no Volute holds the single instance.
    release_instance();
    if volute_shell::update::start_installer(&path, &INSTALLER_ARGS) {
        let _ = slint::quit_event_loop();
    } else {
        fail((NOT_STARTED, path.display().to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Online, so by hand once a release exists: `cargo test -p volute -- --ignored published`.
    #[test]
    #[ignore = "network"]
    fn a_published_release_downloads_and_checks() {
        let latest = volute_shell::update::redirect_of(update::LATEST_URL).expect("a release");
        let version = Version::from_release_url(&latest).expect("a version");
        let path = fetch(version).expect("downloaded and checked");
        assert!(path.ends_with(version.setup_name()));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_build_from_source_never_installs_itself() {
        // `cargo test` runs from target/, which no installer recorded.
        assert_ne!(mode(), INSTALL);
    }
}
