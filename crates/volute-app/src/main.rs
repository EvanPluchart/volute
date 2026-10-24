//! Volute binary: the Slint windows, fed by the audio thread. See docs/ARCHITECTURE.md.

// No console window behind the app in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cli;
mod mixer;
mod model;
mod translations;
mod ui;

use std::time::Duration;

use anyhow::Context;
use slint::ComponentHandle;
use volute_audio::MockAudio;
use volute_core::memory::Memory;
use volute_core::settings::{Settings, Theme};
use volute_shell::settings_file;

use crate::mixer::{
    Backend, Command, ExeLooks, Frame, InFlight, Input, Looks, MakeBackend, NoLooks, Options, SaveMemory,
};

slint::include_modules!();

/// How long quitting may take before the process ends anyway (decision 0004).
const EXIT_WATCHDOG: Duration = Duration::from_secs(3);
/// App icons are read at this size (physical pixels) and drawn at 24 logical pixels: sharp up
/// to 200 % scaling.
const ICON_SIZE: u32 = 48;

fn main() -> anyhow::Result<()> {
    let options = match cli::parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("volute: {message}\n{}", cli::USAGE);
            std::process::exit(2);
        }
    };

    // A test run (--no-settings, or --mock, whose demo apps have real executable paths) takes
    // no single instance and neither reads nor writes remembered volumes: it never reaches the
    // user's Volute or a real app.
    let test_run = options.no_settings || options.mock;
    // One Volute per session: a second launch opens the first one's flyout and exits
    // (decision 0004).
    let instance = if test_run {
        None
    } else {
        match volute_shell::single_instance(|request| {
            // Confirmed from the UI thread once shown: a hung Volute never confirms, and the
            // new launch then starts instead of exiting.
            let _ = slint::invoke_from_event_loop(move || {
                ui::show_flyout_for_launch();
                request.done();
            });
        }) {
            Some(instance) => Some(instance),
            None => return Ok(()),
        }
    };

    // Settings from the file, flags on top for this session only (never written back).
    let settings = if options.no_settings {
        Settings::default()
    } else {
        settings_file::load().map(|text| Settings::parse(&text)).unwrap_or_default()
    };
    let theme = match options.dark {
        Some(true) => Theme::Dark,
        Some(false) => Theme::Light,
        None => settings.theme,
    };
    let scheme = match theme {
        Theme::System => 0,
        Theme::Dark => 1,
        Theme::Light => 2,
    };

    let app = AppWindow::new().context("creating the main window")?;
    // The event loop exists now: keep its hidden message window from closing on a foreign
    // WM_CLOSE, or the tray and a second launch could no longer reach this thread.
    if volute_shell::guard_ui_thread() == 0 && cfg!(windows) {
        eprintln!("volute: no event target window to guard; a close request may cut off the tray");
    }

    // The audio thread stops on Command::Quit; Core Audio's callbacks share its channel.
    let (inputs, receiver) = flume::unbounded::<Input>();
    ui::install(&app, inputs.clone(), scheme, !options.no_tray);
    app.set_tab(options.tab);

    let make: MakeBackend = if options.mock {
        Box::new(|| Ok(Box::new(MockAudio::demo()) as Backend))
    } else {
        Box::new(make_windows_backend)
    };
    let looks: Box<dyn Looks> = if options.mock { Box::new(NoLooks) } else { Box::new(ExeLooks) };
    // Volumes the user set in Volute, per app; a test run neither reads nor writes them.
    let memory = (!test_run).then(|| {
        settings_file::load_named(settings_file::VOLUMES)
            .map(|text| Memory::parse(&text))
            .unwrap_or_default()
    });
    let save: Option<SaveMemory> = (!test_run).then(|| {
        Box::new(|memory: &Memory| {
            if let Err(err) = settings_file::save_named(settings_file::VOLUMES, &memory.to_text()) {
                eprintln!("volute: saving volumes: {err}");
            }
        }) as SaveMemory
    });
    let deliver = Box::new(move |frame: Frame, in_flight: InFlight| {
        slint::invoke_from_event_loop(move || {
            ui::apply(frame);
            // Applied: the next frame may go.
            drop(in_flight);
        })
        .is_ok()
    });
    let audio = mixer::spawn(
        make,
        looks,
        Options { icon_size: ICON_SIZE, memory, save },
        deliver,
        (inputs.clone(), receiver),
    );

    // A process with neither a window nor a tray icon is a bug (decision 0004): without an
    // icon on screen, the window shows.
    if options.open || !ui::in_tray() {
        ui::show_main();
    }
    if options.flyout {
        // As a click at the far bottom right would: placement keeps it inside the work area.
        slint::Timer::single_shot(Duration::from_millis(300), || ui::open_flyout(i32::MAX / 2, i32::MAX / 2));
    }
    if options.about {
        ui::open_about();
    }

    // Runs until Quit (tray menu, sign-out) or the main window closes.
    slint::run_event_loop_until_quit().context("running the event loop")?;

    // Decision 0004, in this order. 1: the watchdog first, so whatever hangs below, the
    // process ends.
    std::thread::spawn(|| {
        std::thread::sleep(EXIT_WATCHDOG);
        std::process::exit(0);
    });
    // 2: every thread gets an explicit Quit (the tray's is its removal, step 5).
    let _ = inputs.send(Input::Ui(Command::Quit));
    // 3: a launch from now on starts a new Volute instead of signalling this one.
    drop(instance);
    // 4: the windows go.
    let _ = app.hide();
    // 5: the tray icon goes (and the flyout with it).
    ui::uninstall();
    let _ = audio.join();
    Ok(())
}

#[cfg(windows)]
fn make_windows_backend() -> volute_audio::Result<Backend> {
    Ok(Box::new(volute_audio::WindowsAudio::new()?))
}

#[cfg(not(windows))]
fn make_windows_backend() -> volute_audio::Result<Backend> {
    Err(volute_audio::Error::Unsupported)
}
