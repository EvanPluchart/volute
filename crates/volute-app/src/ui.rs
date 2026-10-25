//! The UI thread's side: the windows (main and flyout), the tray, and the globals they share.
//! Frames from the audio thread are patched into one set of models that both windows show;
//! what the hand does goes back to the audio thread as commands. Settings and updates live here
//! too, since every window shows them.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use slint::winit_030::winit::platform::windows::{CornerPreference, WindowExtWindows};
use slint::winit_030::{EventResult, WinitWindowAccessor};
use slint::{ComponentHandle, Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};
use volute_core::settings::{Settings, Theme};
use volute_shell::{Tray, TrayEvent};

use crate::cli::TAB_SETTINGS;
use crate::mixer::{Command, Frame, Input};
use crate::model::{self, Held};
use crate::update::{self, Updater};
use crate::{AppRow, AppWindow, DeviceRow, Flyout, Mixer, Prefs, TrayText, Updates};

/// A tray click right after the flyout closed on focus loss is the same click: it must not
/// open it again.
const REOPEN_GUARD: Duration = Duration::from_millis(400);

struct Models {
    rows: Rc<VecModel<AppRow>>,
    levels: Rc<VecModel<f32>>,
    icons: Rc<VecModel<Image>>,
    outputs: Rc<VecModel<DeviceRow>>,
    inputs: Rc<VecModel<DeviceRow>>,
}

/// The values every window shows, kept to fill a flyout when it opens.
#[derive(Default)]
struct Shown {
    master: Option<DeviceRow>,
    mic: Option<DeviceRow>,
    master_level: f32,
    mic_level: f32,
    error: String,
    loaded: bool,
}

struct Ui {
    app: AppWindow,
    flyout: Option<Flyout>,
    /// Counts flyouts opened: a closing request from an older one leaves a newer one alone.
    flyout_gen: u64,
    /// Where the flyout was opened from (physical pixels), to keep it there as it grows.
    anchor: (i32, i32),
    closed_at: Option<Instant>,
    tray: Option<Tray>,
    commands: flume::Sender<Input>,
    models: Models,
    held: Held,
    shown: Shown,
    /// 0 follows the system, 1 dark, 2 light.
    scheme: i32,
    /// The settings as the user set them; saved on every change unless this is a test run.
    settings: Settings,
    test_run: bool,
    update: update::View,
    main_visible: bool,
    main_shown_once: bool,
    main_occluded: bool,
    main_minimized: bool,
    /// Wheel delta over the tray icon not yet worth a notch (precision touchpads).
    wheel: i32,
    /// Clears the notice of a failed command.
    notice_timer: slint::Timer,
    /// Watches which window is in front while the flyout is open.
    flyout_watch: slint::Timer,
    /// What the audio thread was last told.
    audio_visible: bool,
}

thread_local! {
    static UI: RefCell<Option<Ui>> = const { RefCell::new(None) };
}

fn with_ui<R>(f: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    UI.with_borrow_mut(|ui| ui.as_mut().map(f))
}

fn send(command: Command) {
    with_ui(|ui| {
        let _ = ui.commands.send(Input::Ui(command));
    });
}

/// What `install` needs besides the window.
pub struct Setup {
    pub commands: flume::Sender<Input>,
    /// The theme for this session: the settings', or a test flag's.
    pub scheme: i32,
    pub with_tray: bool,
    pub settings: Settings,
    pub test_run: bool,
}

/// Sets up the main window and the tray. The main window is not shown yet.
pub fn install(app: &AppWindow, setup: Setup) {
    let Setup { commands, scheme, with_tray, settings, test_run } = setup;
    let models = Models {
        rows: Rc::new(VecModel::default()),
        levels: Rc::new(VecModel::default()),
        icons: Rc::new(VecModel::default()),
        outputs: Rc::new(VecModel::default()),
        inputs: Rc::new(VecModel::default()),
    };
    if scheme != 0 {
        app.invoke_set_scheme(scheme);
    }
    bind(&app.global::<Mixer>(), &models);
    bind_prefs(&app.global::<Prefs>(), &settings, scheme, test_run);
    bind_updates(&app.global::<Updates>());
    push_update(&app.global::<Updates>(), &update::View { mode: update::mode(), ..update::View::default() });
    // Closing the main window quits (decision 0004), unless the user keeps Volute in the tray
    // and the icon is really there (decision 0009).
    app.window().on_close_requested(|| {
        let keep = with_ui(|ui| ui.settings.close_to_tray && ui.tray.as_ref().is_some_and(Tray::is_shown));
        if keep == Some(true) {
            with_ui(|ui| ui.main_visible = false);
            update_visibility();
        } else {
            let _ = slint::quit_event_loop();
        }
        slint::CloseRequestResponse::HideWindow
    });
    // Minimised or covered: its meters stop. Windows reports minimising as a resize to 0 × 0,
    // not as occlusion.
    app.window().on_winit_window_event(|_, event| {
        use slint::winit_030::winit::event::WindowEvent;
        match event {
            WindowEvent::Occluded(occluded) => {
                with_ui(|ui| ui.main_occluded = *occluded);
                update_visibility();
            }
            WindowEvent::Resized(size) => {
                let minimized = size.width == 0 || size.height == 0;
                let restored = with_ui(|ui| std::mem::replace(&mut ui.main_minimized, minimized) && !minimized);
                if restored == Some(true) {
                    repaint_main();
                }
                update_visibility();
            }
            _ => {}
        }
        EventResult::Propagate
    });
    let tray = with_tray
        .then(|| {
            let text = app.global::<TrayText>();
            let menu = volute_shell::TrayMenu {
                open: text.invoke_open().into(),
                mute: text.invoke_mute().into(),
                settings: text.invoke_settings().into(),
                quit: text.invoke_quit().into(),
            };
            // Next to the clock the first time (decision 0012), never for a test run's icon.
            Tray::new("Volute", menu, !test_run, |event| {
                let _ = slint::invoke_from_event_loop(move || on_tray_event(event));
            })
        })
        .flatten();
    UI.with_borrow_mut(|ui| {
        *ui = Some(Ui {
            app: app.clone_strong(),
            flyout: None,
            flyout_gen: 0,
            anchor: (0, 0),
            closed_at: None,
            tray,
            commands,
            models,
            held: Held::default(),
            shown: Shown::default(),
            scheme,
            settings,
            test_run,
            update: update::View { mode: update::mode(), ..update::View::default() },
            main_visible: false,
            main_shown_once: false,
            main_occluded: false,
            main_minimized: false,
            wheel: 0,
            notice_timer: slint::Timer::default(),
            flyout_watch: slint::Timer::default(),
            audio_visible: false,
        });
    });
}

/// The main window is back on screen: all of it is drawn again, not only what changed.
fn repaint_main() {
    with_ui(|ui| {
        ui.app.set_repaint(ui.app.get_repaint().wrapping_add(1));
        ui.app.window().request_redraw();
    });
}

/// Whether the tray icon is on screen (Volute may then start hidden behind it).
pub fn in_tray() -> bool {
    with_ui(|ui| ui.tray.as_ref().is_some_and(Tray::is_shown)).unwrap_or(false)
}

/// Removes the tray icon and closes the flyout (decision 0004's last steps).
pub fn uninstall() {
    let (tray, flyout) = with_ui(|ui| (ui.tray.take(), ui.flyout.take())).unwrap_or_default();
    if let Some(flyout) = flyout {
        let _ = flyout.hide();
    }
    drop(tray);
}

/// The models and callbacks of one window's `Mixer` global.
fn bind(m: &Mixer, models: &Models) {
    m.set_rows(ModelRc::from(models.rows.clone()));
    m.set_levels(ModelRc::from(models.levels.clone()));
    m.set_icons(ModelRc::from(models.icons.clone()));
    m.set_outputs(ModelRc::from(models.outputs.clone()));
    m.set_inputs(ModelRc::from(models.inputs.clone()));
    m.on_volume_changed(|key, position| {
        with_ui(|ui| ui.held.touch(key.clone()));
        send(Command::SetVolume { key: key.to_string(), position });
    });
    m.on_volume_released(|key, position| {
        with_ui(|ui| ui.held.touch(key.clone()));
        send(Command::SetVolume { key: key.to_string(), position });
    });
    m.on_mute_toggled(|key, muted| send(Command::SetMute { key: key.to_string(), muted }));
    m.on_device_volume_changed(device_volume);
    m.on_device_volume_released(device_volume);
    m.on_device_mute_toggled(|id, muted| send(Command::SetDeviceMute { id: id.to_string(), muted }));
    m.on_default_picked(|id| send(Command::SetDefault { id: id.to_string() }));
    m.on_open_main(|| {
        close_flyout();
        show_main();
    });
}

fn device_volume(id: slint::SharedString, volume: f32) {
    with_ui(|ui| {
        ui.held.touch(id.clone());
        if let Some(master) = ui.shown.master.as_mut().filter(|m| m.id == id) {
            master.volume = volume;
        }
    });
    send(Command::SetDeviceVolume { id: id.to_string(), volume });
}

/// Every window's `Mixer` global, the main window's first.
fn globals(ui: &Ui) -> Vec<Mixer<'_>> {
    let mut all = vec![ui.app.global::<Mixer>()];
    if let Some(flyout) = &ui.flyout {
        all.push(flyout.global::<Mixer>());
    }
    all
}

fn push_shown(m: &Mixer, shown: &Shown) {
    m.set_has_master(shown.master.is_some());
    if let Some(master) = &shown.master {
        m.set_master(master.clone());
    }
    m.set_has_mic(shown.mic.is_some());
    if let Some(mic) = &shown.mic {
        m.set_mic(mic.clone());
    }
    m.set_master_level(shown.master_level);
    m.set_mic_level(shown.mic_level);
    m.set_error(shown.error.as_str().into());
    m.set_loaded(shown.loaded);
}

/// Patches a frame from the audio thread into the models, on the UI thread.
pub fn apply(frame: Frame) {
    let reposition = with_ui(|ui| {
        for icon in frame.icons {
            let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&icon.rgba, icon.width, icon.height);
            // Ids are given in order: the icon's id is its index.
            while ui.models.icons.row_count() < icon.id as usize {
                ui.models.icons.push(Image::default());
            }
            ui.models.icons.push(Image::from_rgba8(buffer));
        }
        let mut changed = false;
        if let Some(rows) = &frame.rows {
            model::patch_rows(&ui.models.rows, rows, &ui.held);
            ui.shown.loaded = true;
            changed = true;
        }
        if let Some(devices) = &frame.devices {
            let (outputs, inputs): (Vec<_>, Vec<_>) = devices.iter().cloned().partition(|d| d.output);
            model::patch_devices(&ui.models.outputs, &outputs, &ui.held);
            model::patch_devices(&ui.models.inputs, &inputs, &ui.held);
            let mut master = outputs.iter().find(|d| d.default).map(model::to_device);
            if let (Some(master), Some(old)) = (master.as_mut(), ui.shown.master.as_ref())
                && master.id == old.id
                && ui.held.holds(&master.id)
            {
                master.volume = old.volume;
            }
            ui.shown.master = master;
            ui.shown.mic = inputs.iter().find(|d| d.default).map(model::to_device);
            changed = true;
        }
        if !frame.levels.is_empty() || frame.rows.is_some() {
            model::patch_levels(&ui.models.levels, &frame.levels);
        }
        if (frame.master_level - ui.shown.master_level).abs() > 0.004
            || (frame.mic_level - ui.shown.mic_level).abs() > 0.004
        {
            ui.shown.master_level = frame.master_level;
            ui.shown.mic_level = frame.mic_level;
            changed = true;
        }
        if let Some(error) = frame.error {
            ui.shown.error = error;
            changed = true;
        }
        if let Some(notice) = &frame.notice {
            show_notice(ui, notice);
        }
        if changed {
            for m in globals(ui) {
                push_shown(&m, &ui.shown);
            }
        }
        if let (Some(tray), Some(state)) = (&ui.tray, &frame.tray) {
            tray.set_glyph(volute_shell::TrayGlyph::for_volume(state.volume, state.muted));
            let text = ui.app.global::<TrayText>().invoke_tooltip(
                state.device.as_str().into(),
                state.volume as i32,
                state.muted,
            );
            tray.set_tooltip(&text);
        }
        frame.rows.is_some() && ui.flyout.is_some()
    });
    if reposition == Some(true) {
        // The flyout's height follows its rows: keep it in place once it has been laid out.
        slint::Timer::single_shot(Duration::ZERO, refit_flyout);
    }
}

/// How long a failed command's notice stays.
const NOTICE_FOR: Duration = Duration::from_secs(5);

fn show_notice(ui: &mut Ui, notice: &str) {
    for m in globals(ui) {
        m.set_notice(notice.into());
    }
    ui.notice_timer.start(slint::TimerMode::SingleShot, NOTICE_FOR, || {
        with_ui(|ui| {
            for m in globals(ui) {
                m.set_notice("".into());
            }
        });
    });
}

/// Tells the audio thread whether any mixer is on screen, when that changes.
fn update_visibility() {
    with_ui(|ui| {
        let visible = (ui.main_visible && !ui.main_occluded && !ui.main_minimized) || ui.flyout.is_some();
        if visible != ui.audio_visible {
            ui.audio_visible = visible;
            let _ = ui.commands.send(Input::Ui(Command::Visible(visible)));
        }
    });
}

/// The main window's size the first time it opens, in logical pixels: Slint would otherwise
/// open it at its minimum.
const MAIN_SIZE: (f32, f32) = (780.0, 600.0);

/// Shows the main window on one of its views.
pub fn show_main_at(tab: i32) {
    with_ui(|ui| ui.app.set_tab(tab));
    show_main();
}

/// Shows the main window and brings it forward.
pub fn show_main() {
    let app = with_ui(|ui| {
        let first = !ui.main_shown_once;
        ui.main_shown_once = true;
        ui.main_visible = true;
        ui.main_occluded = false;
        (ui.app.clone_strong(), first)
    });
    let Some((app, first)) = app else { return };
    // Sized before it first shows, so it opens at its size instead of growing on screen.
    if first {
        app.window().set_size(slint::LogicalSize::new(MAIN_SIZE.0, MAIN_SIZE.1));
    }
    let _ = app.show();
    app.window().with_winit_window(|w| {
        w.set_minimized(false);
        w.focus_window();
    });
    update_visibility();
}

fn on_tray_event(event: TrayEvent) {
    match event {
        TrayEvent::Clicked { x, y } => toggle_flyout(x, y),
        TrayEvent::Open => show_main(),
        TrayEvent::Settings | TrayEvent::NotificationClicked => show_main_at(TAB_SETTINGS),
        TrayEvent::ToggleMute => {
            let master = with_ui(|ui| ui.shown.master.clone()).flatten();
            if let Some(master) = master {
                send(Command::SetDeviceMute { id: master.id.to_string(), muted: !master.muted });
            }
        }
        TrayEvent::Wheel { delta } => {
            let steps = with_ui(|ui| {
                let (steps, rest) = notches(ui.wheel, delta);
                ui.wheel = rest;
                steps
            });
            if let Some(steps) = steps.filter(|&s| s != 0) {
                send(Command::StepMaster(steps));
            }
        }
        TrayEvent::Quit => {
            let _ = slint::quit_event_loop();
        }
    }
}

/// Opens the flyout from a tray click at (`x`, `y`), or closes it if it is open.
pub fn toggle_flyout(x: i32, y: i32) {
    let open = with_ui(|ui| ui.flyout.is_some()).unwrap_or(false);
    if open {
        close_flyout();
        return;
    }
    let just_closed = with_ui(|ui| ui.closed_at.is_some_and(|at| at.elapsed() < REOPEN_GUARD)).unwrap_or(false);
    if !just_closed {
        open_flyout(x, y);
    }
}

/// A launch of Volute by hand asks for it: the flyout, from the tray icon as a click on it would
/// open it (on the screen with the taskbar's clock), unless it is open already.
pub fn show_flyout_for_launch() {
    let open = with_ui(|ui| ui.flyout.is_none().then(|| ui.tray.as_ref().and_then(Tray::rect)));
    let Some(icon) = open.flatten() else { return };
    let (x, y) = match icon {
        Some(r) => (r.x + r.width / 2, r.y + r.height / 2),
        // No icon: the bottom right of the main screen.
        None => volute_shell::work_area_at(0, 0).map_or((0, 0), |a| (a.x + a.width - 1, a.y + a.height - 1)),
    };
    open_flyout(x, y);
}

/// Opens the flyout as a click on the tray at (`x`, `y`) would.
pub fn open_flyout(x: i32, y: i32) {
    let Ok(flyout) = Flyout::new() else { return };
    let generation = with_ui(|ui| {
        ui.flyout_gen += 1;
        ui.flyout_gen
    })
    .unwrap_or_default();
    with_ui(|ui| {
        if ui.scheme != 0 {
            flyout.invoke_set_scheme(ui.scheme);
        }
        let m = flyout.global::<Mixer>();
        bind(&m, &ui.models);
        push_shown(&m, &ui.shown);
        let u = flyout.global::<Updates>();
        bind_updates(&u);
        push_update(&u, &ui.update);
        ui.anchor = (x, y);
    });
    flyout.on_dismiss(move || close_flyout_later(generation));
    // It closes when it loses focus, like the system's own flyouts.
    flyout.window().on_winit_window_event(move |_, event| {
        if let slint::winit_030::winit::event::WindowEvent::Focused(false) = event {
            with_ui(|ui| {
                if ui.flyout_gen == generation {
                    ui.closed_at = Some(Instant::now());
                }
            });
            close_flyout_later(generation);
        }
        EventResult::Propagate
    });
    // Another program's close request (taskkill): just close the flyout.
    flyout.window().on_close_requested(move || {
        close_flyout_later(generation);
        slint::CloseRequestResponse::HideWindow
    });
    let front_at_open = volute_shell::foreground_window();
    let _ = flyout.show();
    flyout.window().with_winit_window(|w| {
        w.set_skip_taskbar(true);
        w.set_corner_preference(CornerPreference::Round);
    });
    // The window only appears on the next turn of the event loop: before that, asking for the
    // front does nothing, and appearing gives it a taskbar button again. Both once it is there.
    slint::Timer::single_shot(Duration::from_millis(50), move || {
        let hwnd = with_ui(|ui| {
            let flyout = ui.flyout.as_ref().filter(|_| ui.flyout_gen == generation)?;
            flyout.window().with_winit_window(|w| {
                w.set_skip_taskbar(true);
                window_handle(w)
            })
        })
        .flatten()
        .flatten();
        if let Some(hwnd) = hwnd {
            volute_shell::bring_forward(hwnd);
            watch_flyout(generation, hwnd, front_at_open);
        }
    });
    with_ui(|ui| ui.flyout = Some(flyout));
    place_flyout();
    update_visibility();
}

/// A window's handle as an integer, for `volute_shell`.
fn window_handle(w: &slint::winit_030::winit::window::Window) -> Option<isize> {
    use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match w.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
        _ => None,
    }
}

/// How often the open flyout checks which window is in front.
const FLYOUT_WATCH: Duration = Duration::from_millis(150);

/// The flyout closes when another window comes to the front. Losing the focus says so when the
/// flyout had it; when Windows refused it the front (a launch it did not allow), a click
/// elsewhere still changes the window in front, which this sees. Only while the flyout is open.
fn watch_flyout(generation: u64, hwnd: isize, front_at_open: isize) {
    let mut was_front = false;
    with_ui(|ui| {
        ui.flyout_watch.start(slint::TimerMode::Repeated, FLYOUT_WATCH, move || {
            let front = volute_shell::foreground_window();
            if front == hwnd {
                was_front = true;
            } else if front != 0 && (was_front || front != front_at_open) {
                with_ui(|ui| {
                    if ui.flyout_gen == generation {
                        ui.closed_at = Some(Instant::now());
                    }
                });
                close_flyout_later(generation);
            }
        });
    });
}

/// Puts the flyout next to where it was opened, inside the work area.
fn place_flyout() {
    with_ui(|ui| {
        if let Some(flyout) = &ui.flyout {
            let size = flyout.window().size();
            let (x, y) = volute_shell::place_near(ui.anchor.0, ui.anchor.1, size.width as i32, size.height as i32);
            flyout.window().set_position(slint::PhysicalPosition::new(x, y));
        }
    });
}

/// The rows changed while the flyout is open. Its height follows them, but it grows downwards,
/// past the bottom of the screen, before it can be moved up; Slint 1.18 redraws only what
/// changed, so the part that was off screen stays blank. A flyout whose height must change is
/// opened again instead, at once, where it was.
fn refit_flyout() {
    let reopen = with_ui(|ui| {
        let flyout = ui.flyout.as_ref()?;
        let window = flyout.window();
        let drawn = window.size().to_logical(window.scale_factor()).height;
        ((drawn - flyout.get_wanted_height()).abs() > 0.5).then_some(ui.anchor)
    })
    .flatten();
    match reopen {
        Some((x, y)) => {
            close_flyout();
            open_flyout(x, y);
        }
        None => place_flyout(),
    }
}

/// Closing from inside the flyout's own event handler: do it once the handler returns, and only
/// if that flyout is still the one open.
fn close_flyout_later(generation: u64) {
    slint::Timer::single_shot(Duration::ZERO, move || {
        if with_ui(|ui| ui.flyout_gen == generation).unwrap_or(false) {
            close_flyout();
        }
    });
}

/// Hides and drops the flyout, so its frame buffer is freed.
pub fn close_flyout() {
    let flyout = with_ui(|ui| {
        ui.flyout_watch.stop();
        ui.flyout.take()
    })
    .flatten();
    if let Some(flyout) = flyout {
        let _ = flyout.hide();
    }
    update_visibility();
}

/// The Settings view's values and what its controls do.
fn bind_prefs(p: &Prefs, settings: &Settings, scheme: i32, test_run: bool) {
    p.set_theme(scheme);
    p.set_close_to_tray(settings.close_to_tray);
    p.set_check_updates(settings.check_updates);
    p.set_start_with_windows(volute_shell::startup::enabled());
    // A test run changes nothing on the machine, so not what Windows starts at sign-in.
    p.set_can_change_system(!test_run);
    p.set_version(env!("CARGO_PKG_VERSION").into());
    p.on_theme_picked(|scheme| {
        let theme = match scheme {
            1 => Theme::Dark,
            2 => Theme::Light,
            _ => Theme::System,
        };
        change_settings(|s| s.theme = theme);
        set_scheme(scheme);
    });
    p.on_close_to_tray_toggled(|on| change_settings(|s| s.close_to_tray = on));
    p.on_check_updates_toggled(|on| {
        change_settings(|s| s.check_updates = on);
        UPDATER.with_borrow(|u| {
            if let Some(updater) = u {
                updater.set_enabled(on);
            }
        });
    });
    p.on_start_with_windows_toggled(|on| {
        let Some((test_run, app)) = with_ui(|ui| (ui.test_run, ui.app.clone_strong())) else { return };
        if !test_run && !volute_shell::startup::set(on) {
            eprintln!("volute: could not change starting with Windows");
        }
        // Whatever happened, show what Windows will do.
        app.global::<Prefs>().set_start_with_windows(volute_shell::startup::enabled());
    });
    p.on_open_link(|which| {
        let repo = env!("CARGO_PKG_REPOSITORY");
        let url = match which.as_str() {
            "notes" => format!("{repo}/releases"),
            "issues" => format!("{repo}/issues"),
            "license" => format!("{repo}/blob/main/LICENSE"),
            _ => repo.to_owned(),
        };
        volute_shell::open_url(&url);
    });
}

/// Changes the settings and saves them, unless this is a test run.
fn change_settings(change: impl FnOnce(&mut Settings)) {
    let save = with_ui(|ui| {
        change(&mut ui.settings);
        (!ui.test_run).then(|| ui.settings.to_text())
    })
    .flatten();
    if let Some(text) = save
        && let Err(err) = volute_shell::settings_file::save(&text)
    {
        eprintln!("volute: saving settings: {err}");
    }
}

/// Switches every window to a theme: 0 follows the system, 1 dark, 2 light.
fn set_scheme(scheme: i32) {
    with_ui(|ui| {
        ui.scheme = scheme;
        ui.app.invoke_set_scheme(scheme);
        if let Some(flyout) = &ui.flyout {
            flyout.invoke_set_scheme(scheme);
        }
    });
}

/// What the update line's buttons do, in any window.
fn bind_updates(u: &Updates) {
    u.on_check_now(|| {
        UPDATER.with_borrow(|u| {
            if let Some(updater) = u {
                updater.check_now();
            }
        });
    });
    u.on_update(|| {
        let view = with_ui(|ui| ui.update.clone());
        // Outside the borrow: the updater sets the state as it starts.
        let Some(view) = view else { return };
        let updater = UPDATER.with_borrow(|u| u.clone());
        if let Some(updater) = updater {
            updater.update(&view);
        }
    });
    u.on_whats_new(|| {
        let version = with_ui(|ui| ui.update.version.clone()).unwrap_or_default();
        if let Some(version) = volute_core::update::Version::parse(&version) {
            volute_shell::open_url(&volute_core::update::release_page(version));
        }
    });
}

fn push_update(u: &Updates, view: &update::View) {
    u.set_state(view.state);
    u.set_version(view.version.as_str().into());
    u.set_mode(view.mode);
    u.set_error_kind(view.error_kind);
    u.set_error(view.error.as_str().into());
}

thread_local! {
    /// The updater, shared outside `Ui`'s borrow: starting an update sets the UI state.
    static UPDATER: RefCell<Option<Rc<Updater>>> = const { RefCell::new(None) };
}

/// Starts looking for updates (decision 0005): `enabled` is the setting, `current` this
/// version or the one pretended. `installed`: the installer just started this Volute again.
pub fn start_updates(enabled: bool, current: String, installed: bool) {
    if installed {
        let version = current.clone();
        set_update(|view| {
            view.state = update::INSTALLED;
            view.version = version;
        });
    }
    if update::mode() == update::UNSUPPORTED {
        return;
    }
    let test_run = with_ui(|ui| ui.test_run).unwrap_or(true);
    if let Some(updater) = update::start(enabled, current, update::FIRST_CHECK, test_run) {
        UPDATER.with_borrow_mut(|u| *u = Some(Rc::new(updater)));
    }
}

/// Changes what the windows show of the updater.
pub fn set_update(change: impl FnOnce(&mut update::View)) {
    with_ui(|ui| {
        let before = ui.update.clone();
        change(&mut ui.update);
        if ui.update != before {
            push_update(&ui.app.global::<Updates>(), &ui.update);
            if let Some(flyout) = &ui.flyout {
                push_update(&flyout.global::<Updates>(), &ui.update);
            }
        }
    });
}

/// A new version, found by a background check: one notification from the tray icon.
pub fn notify_update(version: &str) {
    with_ui(|ui| {
        if let Some(tray) = &ui.tray {
            let text = ui.app.global::<TrayText>();
            tray.notify(&text.invoke_update_title(version.into()), &text.invoke_update_body());
        }
    });
}

/// Whole wheel notches (120 each) in `pending + delta`, and what is left over for the next turn.
fn notches(pending: i32, delta: i32) -> (i32, i32) {
    let total = pending + delta;
    (total / 120, total % 120)
}

#[cfg(test)]
mod tests {
    use super::notches;

    #[test]
    fn wheel_deltas_add_up_to_notches() {
        assert_eq!(notches(0, 120), (1, 0));
        assert_eq!(notches(0, -240), (-2, 0));
        // A touchpad: small deltas add up, the rest waits.
        assert_eq!(notches(0, 40), (0, 40));
        assert_eq!(notches(40, 40), (0, 80));
        assert_eq!(notches(80, 50), (1, 10));
        assert_eq!(notches(10, -30), (0, -20));
    }
}
