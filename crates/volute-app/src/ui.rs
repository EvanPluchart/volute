//! The UI thread's side: the windows (main, flyout, About), the tray, and the `Mixer` global
//! they share. Frames from the audio thread are patched into one set of models that both
//! windows show; what the hand does goes back to the audio thread as commands.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use slint::winit_030::winit::platform::windows::{CornerPreference, WindowExtWindows};
use slint::winit_030::{EventResult, WinitWindowAccessor};
use slint::{ComponentHandle, Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};
use volute_shell::{Tray, TrayEvent};

use crate::mixer::{Command, Frame, Input};
use crate::model::{self, Held};
use crate::{AboutWindow, AppRow, AppWindow, DeviceRow, Flyout, Mixer, TrayText};

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
    /// Where the flyout was opened from (physical pixels), to keep it there as it grows.
    anchor: (i32, i32),
    closed_at: Option<Instant>,
    about: Option<AboutWindow>,
    tray: Option<Tray>,
    commands: flume::Sender<Input>,
    models: Models,
    held: Held,
    shown: Shown,
    /// 0 follows the system, 1 dark, 2 light.
    scheme: i32,
    main_visible: bool,
    main_shown_once: bool,
    main_occluded: bool,
    /// Wheel delta over the tray icon not yet worth a notch (precision touchpads).
    wheel: i32,
    /// Clears the notice of a failed command.
    notice_timer: slint::Timer,
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

/// Sets up the main window and the tray. The main window is not shown yet.
pub fn install(app: &AppWindow, commands: flume::Sender<Input>, scheme: i32, with_tray: bool) {
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
    app.on_show_about(show_about);
    // Closing the main window quits (decision 0004); the flyout is how Volute lives in the tray.
    app.window().on_close_requested(|| {
        let _ = slint::quit_event_loop();
        slint::CloseRequestResponse::HideWindow
    });
    // Minimised or covered: its meters stop.
    app.window().on_winit_window_event(|_, event| {
        use slint::winit_030::winit::event::WindowEvent;
        if let WindowEvent::Occluded(occluded) = event {
            with_ui(|ui| ui.main_occluded = *occluded);
            update_visibility();
        }
        EventResult::Propagate
    });
    let tray = with_tray
        .then(|| {
            let text = app.global::<TrayText>();
            let menu = volute_shell::TrayMenu {
                open: text.invoke_open().into(),
                mute: text.invoke_mute().into(),
                quit: text.invoke_quit().into(),
            };
            Tray::new("Volute", menu, |event| {
                let _ = slint::invoke_from_event_loop(move || on_tray_event(event));
            })
        })
        .flatten();
    UI.with_borrow_mut(|ui| {
        *ui = Some(Ui {
            app: app.clone_strong(),
            flyout: None,
            anchor: (0, 0),
            closed_at: None,
            about: None,
            tray,
            commands,
            models,
            held: Held::default(),
            shown: Shown::default(),
            scheme,
            main_visible: false,
            main_shown_once: false,
            main_occluded: false,
            wheel: 0,
            notice_timer: slint::Timer::default(),
            audio_visible: false,
        });
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
            tray.set_muted(state.muted);
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
        slint::Timer::single_shot(Duration::ZERO, place_flyout);
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
        let visible = (ui.main_visible && !ui.main_occluded) || ui.flyout.is_some();
        if visible != ui.audio_visible {
            ui.audio_visible = visible;
            let _ = ui.commands.send(Input::Ui(Command::Visible(visible)));
        }
    });
}

/// The main window's size the first time it opens, in logical pixels: Slint would otherwise
/// open it at its minimum.
const MAIN_SIZE: (f32, f32) = (460.0, 640.0);

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
    let _ = app.show();
    if first {
        app.window().set_size(slint::LogicalSize::new(MAIN_SIZE.0, MAIN_SIZE.1));
    }
    app.window().with_winit_window(|w| {
        w.set_minimized(false);
        w.focus_window();
    });
    update_visibility();
}

fn show_about() {
    let about = with_ui(|ui| {
        if ui.about.is_none() {
            ui.about = AboutWindow::new().ok();
            if let Some(about) = &ui.about
                && ui.scheme != 0
            {
                about.invoke_set_scheme(ui.scheme);
            }
        }
        ui.about.as_ref().map(ComponentHandle::clone_strong)
    })
    .flatten();
    // Shown outside the borrow: showing a window may run handlers that need the UI state.
    if let Some(about) = about {
        about.set_version(env!("CARGO_PKG_VERSION").into());
        let _ = about.show();
    }
}

/// Opens the About window at start (`--about`).
pub fn open_about() {
    show_about();
}

fn on_tray_event(event: TrayEvent) {
    match event {
        TrayEvent::Clicked { x, y } => toggle_flyout(x, y),
        TrayEvent::Open => show_main(),
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

/// A second launch of Volute asks for it: the flyout, at the bottom right as a tray click would
/// open it, unless it is open already.
pub fn show_flyout_for_launch() {
    if with_ui(|ui| ui.flyout.is_none()).unwrap_or(false) {
        open_flyout(i32::MAX / 2, i32::MAX / 2);
    }
}

/// Opens the flyout as a click on the tray at (`x`, `y`) would.
pub fn open_flyout(x: i32, y: i32) {
    let Ok(flyout) = Flyout::new() else { return };
    with_ui(|ui| {
        if ui.scheme != 0 {
            flyout.invoke_set_scheme(ui.scheme);
        }
        let m = flyout.global::<Mixer>();
        bind(&m, &ui.models);
        push_shown(&m, &ui.shown);
        ui.anchor = (x, y);
    });
    flyout.on_dismiss(close_flyout_later);
    // It closes when it loses focus, like the system's own flyouts.
    flyout.window().on_winit_window_event(|_, event| {
        if let slint::winit_030::winit::event::WindowEvent::Focused(false) = event {
            with_ui(|ui| ui.closed_at = Some(Instant::now()));
            close_flyout_later();
        }
        EventResult::Propagate
    });
    // Another program's close request (taskkill): just close the flyout.
    flyout.window().on_close_requested(|| {
        close_flyout_later();
        slint::CloseRequestResponse::HideWindow
    });
    let _ = flyout.show();
    flyout.window().with_winit_window(|w| {
        w.set_skip_taskbar(true);
        w.set_corner_preference(CornerPreference::Round);
        w.focus_window();
    });
    with_ui(|ui| ui.flyout = Some(flyout));
    place_flyout();
    update_visibility();
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

/// Closing from inside the flyout's own event handler: do it once the handler returns.
fn close_flyout_later() {
    slint::Timer::single_shot(Duration::ZERO, close_flyout);
}

/// Hides and drops the flyout, so its frame buffer is freed.
pub fn close_flyout() {
    let flyout = with_ui(|ui| ui.flyout.take()).flatten();
    if let Some(flyout) = flyout {
        let _ = flyout.hide();
    }
    update_visibility();
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
