//! Volute binary: the Slint window, fed by the audio thread. See docs/ARCHITECTURE.md.

// No console window behind the app in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cli;
mod mixer;
mod model;
mod translations;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use anyhow::Context;
use slint::winit_030::{EventResult, WinitWindowAccessor};
use slint::{ComponentHandle, ModelRc, VecModel};
use volute_audio::MockAudio;
use volute_core::settings::{Settings, Theme};
use volute_shell::{Tray, TrayEvent, TrayMenu};

use crate::mixer::{Backend, Command, Frame, InFlight, MakeBackend};
use crate::model::Held;

slint::include_modules!();

/// How long quitting may take before the process ends anyway (decision 0004).
const EXIT_WATCHDOG: Duration = Duration::from_secs(3);

fn main() -> anyhow::Result<()> {
    let options = match cli::parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("volute: {message}\n{}", cli::USAGE);
            std::process::exit(2);
        }
    };

    // One Volute per session: a second launch shows the first and exits. Test runs
    // (--no-settings) may run next to the user's Volute.
    let instance = if options.no_settings {
        None
    } else {
        match volute_shell::single_instance(|request| {
            // Confirmed from the UI thread once shown: a hung Volute never confirms, and the
            // new launch then starts instead of exiting.
            let _ = slint::invoke_from_event_loop(move || {
                show_main();
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
        volute_shell::settings_file::load()
            .map(|text| Settings::parse(&text))
            .unwrap_or_default()
    };
    let theme = match options.dark {
        Some(true) => Theme::Dark,
        Some(false) => Theme::Light,
        None => settings.theme,
    };

    let app = AppWindow::new().context("creating the main window")?;
    // "System" assigns nothing: the palette then follows the system theme live.
    if theme != Theme::System {
        app.invoke_set_scheme(if theme == Theme::Dark { 1 } else { 2 });
    }
    let rows = Rc::new(VecModel::<AppRow>::default());
    let levels = Rc::new(VecModel::<f32>::default());
    app.set_rows(ModelRc::from(rows.clone()));
    app.set_levels(ModelRc::from(levels.clone()));

    // The audio thread stops on Command::Quit.
    let (commands, receiver) = flume::unbounded::<Command>();
    COMMANDS.with_borrow_mut(|c| *c = Some(commands.clone()));
    MAIN.with_borrow_mut(|m| *m = Some(app.as_weak()));

    let held = Rc::new(RefCell::new(Held::default()));
    let (tx, touched) = (commands.clone(), held.clone());
    app.on_volume_changed(move |key, position| {
        touched.borrow_mut().touch(key.clone());
        let _ = tx.send(Command::SetVolume {
            key: key.to_string(),
            position,
        });
    });
    let (tx, touched) = (commands.clone(), held.clone());
    app.on_volume_released(move |key, position| {
        touched.borrow_mut().touch(key.clone());
        let _ = tx.send(Command::SetVolume {
            key: key.to_string(),
            position,
        });
    });
    let tx = commands.clone();
    app.on_mute_toggled(move |key, muted| {
        let _ = tx.send(Command::SetMute {
            key: key.to_string(),
            muted,
        });
    });
    let about: Rc<RefCell<Option<AboutWindow>>> = Rc::default();
    app.on_show_about(move || {
        let mut about = about.borrow_mut();
        if about.is_none() {
            *about = AboutWindow::new().ok();
        }
        if let Some(window) = about.as_ref() {
            window.set_version(env!("CARGO_PKG_VERSION").into());
            let _ = window.show();
        }
    });
    // Closing the window quits: closing to the tray comes with the flyout (M1).
    app.window().on_close_requested(|| {
        let _ = slint::quit_event_loop();
        slint::CloseRequestResponse::HideWindow
    });
    // Minimised or covered: the meters stop.
    let tx = commands.clone();
    app.window().on_winit_window_event(move |_, event| {
        use slint::winit_030::winit::event::WindowEvent;
        if let WindowEvent::Occluded(occluded) = event {
            let _ = tx.send(Command::Visible(!occluded));
        }
        EventResult::Propagate
    });

    let tray = (!options.no_tray)
        .then(|| {
            let text = app.global::<TrayText>();
            let menu = TrayMenu {
                open: text.invoke_open().into(),
                quit: text.invoke_quit().into(),
            };
            Tray::new("Volute", menu, |event| {
                let _ = slint::invoke_from_event_loop(move || on_tray_event(event));
            })
        })
        .flatten();
    // A process with neither a window nor a tray icon is a bug (decision 0004): without an
    // icon on screen, the window shows.
    let in_tray = tray.as_ref().is_some_and(Tray::is_shown);
    if options.open || !in_tray {
        show_main();
    }

    let make: MakeBackend = if options.mock {
        Box::new(|| Ok(Box::new(MockAudio::demo()) as Backend))
    } else {
        Box::new(make_windows_backend)
    };
    let weak = app.as_weak();
    let deliver = Box::new(move |frame: Frame, in_flight: InFlight| {
        let weak = weak.clone();
        slint::invoke_from_event_loop(move || {
            apply(&weak, frame);
            // Applied: the next frame may go.
            drop(in_flight);
        })
        .is_ok()
    });
    MODELS.with_borrow_mut(|m| *m = Some((rows, levels, held)));
    let audio = mixer::spawn(make, deliver, receiver);

    // Runs until Quit (tray menu, sign-out) or the window closes.
    slint::run_event_loop_until_quit().context("running the event loop")?;

    // Decision 0004, in this order. 1: the watchdog first, so whatever hangs below, the
    // process ends.
    std::thread::spawn(|| {
        std::thread::sleep(EXIT_WATCHDOG);
        std::process::exit(0);
    });
    // 2: every thread gets an explicit Quit (the tray's is its removal, step 5).
    let _ = commands.send(Command::Quit);
    // 3: a launch from now on starts a new Volute instead of signalling this one.
    drop(instance);
    // 4: the windows go.
    let _ = app.hide();
    // 5: the tray icon goes.
    drop(tray);
    let _ = audio.join();
    Ok(())
}

thread_local! {
    static MAIN: RefCell<Option<slint::Weak<AppWindow>>> = const { RefCell::new(None) };
    static COMMANDS: RefCell<Option<flume::Sender<Command>>> = const { RefCell::new(None) };
    #[allow(clippy::type_complexity)]
    static MODELS: RefCell<Option<(Rc<VecModel<AppRow>>, Rc<VecModel<f32>>, Rc<RefCell<Held>>)>> =
        const { RefCell::new(None) };
}

#[cfg(windows)]
fn make_windows_backend() -> volute_audio::Result<Backend> {
    Ok(Box::new(volute_audio::WindowsAudio::new()?))
}

#[cfg(not(windows))]
fn make_windows_backend() -> volute_audio::Result<Backend> {
    Err(volute_audio::Error::Unsupported)
}

/// Patches a frame into the models, on the UI thread.
fn apply(weak: &slint::Weak<AppWindow>, frame: Frame) {
    let Some(app) = weak.upgrade() else { return };
    MODELS.with_borrow(|models| {
        let Some((rows, levels, held)) = models else {
            return;
        };
        if let Some(data) = &frame.rows {
            model::patch_rows(rows, data, &held.borrow());
            app.set_loaded(true);
        }
        model::patch_levels(levels, &frame.levels);
    });
    if let Some(error) = frame.error {
        app.set_error(error.into());
    }
}

/// Shows the main window and brings it forward; the meters start.
fn show_main() {
    let Some(app) = MAIN.with_borrow(|m| m.as_ref().and_then(slint::Weak::upgrade)) else {
        return;
    };
    let _ = app.show();
    app.window().with_winit_window(|w| {
        w.set_minimized(false);
        w.focus_window();
    });
    COMMANDS.with_borrow(|c| {
        if let Some(tx) = c {
            let _ = tx.send(Command::Visible(true));
        }
    });
}

fn on_tray_event(event: TrayEvent) {
    match event {
        TrayEvent::Clicked { .. } | TrayEvent::Open => show_main(),
        TrayEvent::Quit => {
            let _ = slint::quit_event_loop();
        }
    }
}
