//! The tray icon on Windows (decision 0004), ported from Vigie: `Shell_NotifyIconW` with
//! `NOTIFYICON_VERSION_4`, driven by a hidden window on a thread of its own. The thread sleeps
//! in `GetMessageW`, so the icon costs nothing between events. Notifications are balloons on
//! the same icon, which Windows shows as toasts.

use std::cell::Cell;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use windows_sys::Win32::Foundation::{ERROR_NO_MORE_ITEMS, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::UI::Input::{
    GetRawInputData, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER, RID_INPUT,
    RIDEV_INPUTSINK, RIDEV_REMOVE, RIM_TYPEMOUSE, RegisterRawInputDevices,
};
use windows_sys::Win32::System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleW};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, REG_DWORD, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegCloseKey,
    RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, RegSetKeyValueW,
};
use windows_sys::Win32::UI::HiDpi::{GetDpiForSystem, GetSystemMetricsForDpi};
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIIF_LARGE_ICON, NIIF_USER, NIM_ADD,
    NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NIN_BALLOONUSERCLICK, NIN_SELECT, NOTIFYICON_VERSION_4, NOTIFYICONDATAW, NOTIFYICONIDENTIFIER,
    Shell_NotifyIconGetRect, Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu,
    DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetMessageW, GetWindowLongPtrW, HICON, KillTimer,
    IDI_APPLICATION, IMAGE_ICON, LR_DEFAULTCOLOR, LoadIconW, LoadImageW, MF_CHECKED, MF_SEPARATOR, MF_STRING,
    MSG, PostMessageW, PostQuitMessage, RegisterClassW, RegisterWindowMessageW, SM_CXICON, SM_CXSMICON,
    SetForegroundWindow, SetTimer, SetWindowLongPtrW, TPM_BOTTOMALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    TrackPopupMenu, TranslateMessage, WM_APP, WM_CLOSE, WM_CONTEXTMENU, WM_DESTROY,
    WM_ENDSESSION, WM_INPUT, WM_LBUTTONDBLCLK, WM_MOUSEMOVE, WM_NULL, WM_SETTINGCHANGE, WM_TIMER, WNDCLASSW, WS_OVERLAPPED,
    GetCursorPos, RI_MOUSE_WHEEL,
};

use crate::{TrayEvent, TrayGlyph, TrayMenu};

/// Messages the icon sends to the window, and the ones the app posts to it.
const WM_TRAY: u32 = WM_APP + 1;
const WM_UPDATE: u32 = WM_APP + 2;
const WM_NOTIFY_USER: u32 = WM_APP + 3;
/// Volute removes its icon. Not WM_CLOSE: other programs send that one (`taskkill` without
/// `/F` does), and the icon must not vanish while Volute runs hidden behind it.
const WM_SHUTDOWN: u32 = WM_APP + 4;
const ICON_ID: u32 = 1;
/// The application icon in the executable's resources (for notifications), and the first of the
/// tray's speakers: white from 10, ink from 20, in `TrayGlyph::index` order (decision 0013).
const ICON_RESOURCE: u16 = 1;
const SPEAKER_WHITE: u16 = 10;
const SPEAKER_INK: u16 = 20;
/// Where Windows says whether the taskbar is light.
const PERSONALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
const MENU_OPEN: usize = 1;
const MENU_QUIT: usize = 2;
const MENU_MUTE: usize = 3;
const MENU_SETTINGS: usize = 4;
/// The timer that looks for Windows' record of the icon, to show it next to the clock once.
const PROMOTE_TIMER: usize = 1;
/// How often, and how many times, the record is looked for: Explorer writes it after the icon
/// is added.
const PROMOTE_EVERY_MS: u32 = 1500;
const PROMOTE_TRIES: u8 = 5;

type Callback = Arc<dyn Fn(TrayEvent) + Send + Sync>;

/// What the app changes from its threads; the tray thread applies it on `WM_UPDATE`.
struct Shared {
    tooltip: String,
    menu: TrayMenu,
    /// The icon is in the notification area (adding it fails while Explorer is down).
    shown: bool,
    /// What the speaker shows; `Muted` also checks "Mute all" in the menu.
    glyph: TrayGlyph,
    /// A notification to show (title, body), taken by the tray thread.
    notification: Option<(String, String)>,
    /// Volute is removing its icon: `WM_SHUTDOWN` from anyone else (a broadcast) is ignored.
    closing: bool,
}

/// Lives on the tray thread, pointed to by the window's user data.
struct State {
    shared: Arc<Mutex<Shared>>,
    callback: Callback,
    /// The speakers: white (for a dark taskbar), then ink, each in `TrayGlyph::index` order.
    speakers: [[HICON; 5]; 2],
    /// The taskbar is light: the ink speaker shows.
    light: Cell<bool>,
    /// The large application icon, for notifications.
    balloon_icon: HICON,
    taskbar_created: u32,
    /// Raw mouse input is registered: the pointer is over the icon (decision 0007).
    listening: Cell<bool>,
    /// Where the icon was, last time the pointer moved over it.
    rect: Cell<RECT>,
    /// Looks left for Windows' record of the icon, to promote it (0: not looking).
    promote_tries: Cell<u8>,
}

pub(crate) struct Tray {
    /// The window handle as an integer, so the handle can cross threads.
    hwnd: isize,
    shared: Arc<Mutex<Shared>>,
    thread: Option<JoinHandle<()>>,
}

fn wide_into<const N: usize>(text: &str, out: &mut [u16; N]) {
    out.fill(0);
    for (slot, unit) in out.iter_mut().take(N - 1).zip(text.encode_utf16()) {
        *slot = unit;
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

impl Tray {
    pub(crate) fn new(tooltip: &str, menu: TrayMenu, promote: bool, callback: Callback) -> Option<Tray> {
        let shared = Arc::new(Mutex::new(Shared {
            tooltip: tooltip.to_owned(),
            menu,
            shown: false,
            glyph: TrayGlyph::default(),
            notification: None,
            closing: false,
        }));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<isize>();
        let thread_shared = shared.clone();
        let thread = std::thread::Builder::new()
            .name("volute-tray".into())
            .spawn(move || run(thread_shared, callback, promote, ready_tx))
            .ok()?;
        let hwnd = ready_rx.recv().ok().filter(|&h| h != 0)?;
        Some(Tray {
            hwnd,
            shared,
            thread: Some(thread),
        })
    }

    fn post(&self, message: u32) {
        // SAFETY: posting to a window owned by the tray thread; harmless if it is gone.
        unsafe { PostMessageW(self.hwnd as HWND, message, 0, 0) };
    }

    pub(crate) fn set_tooltip(&self, tooltip: &str) {
        if let Ok(mut shared) = self.shared.lock() {
            if shared.tooltip == tooltip {
                return;
            }
            tooltip.clone_into(&mut shared.tooltip);
        }
        self.post(WM_UPDATE);
    }

    pub(crate) fn set_glyph(&self, glyph: TrayGlyph) {
        if let Ok(mut shared) = self.shared.lock() {
            if shared.glyph == glyph {
                return;
            }
            shared.glyph = glyph;
        }
        self.post(WM_UPDATE);
    }

    /// Shows a notification from the icon; a click on it reports `NotificationClicked`.
    pub(crate) fn notify(&self, title: &str, body: &str) {
        if let Ok(mut shared) = self.shared.lock() {
            shared.notification = Some((title.to_owned(), body.to_owned()));
        }
        self.post(WM_NOTIFY_USER);
    }

    pub(crate) fn rect(&self) -> Option<crate::Rect> {
        let id = NOTIFYICONIDENTIFIER {
            cbSize: std::mem::size_of::<NOTIFYICONIDENTIFIER>() as u32,
            hWnd: self.hwnd as HWND,
            uID: ICON_ID,
            ..NOTIFYICONIDENTIFIER::default()
        };
        let mut rect = RECT::default();
        // SAFETY: a sized identifier of our own icon, and a rectangle to fill.
        (unsafe { Shell_NotifyIconGetRect(&id, &mut rect) } == 0).then(|| crate::Rect {
            x: rect.left,
            y: rect.top,
            width: rect.right - rect.left,
            height: rect.bottom - rect.top,
        })
    }

    /// Whether the icon is on screen: hiding Volute behind a missing icon would lose it.
    pub(crate) fn is_shown(&self) -> bool {
        self.shared.lock().is_ok_and(|s| s.shown)
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        if let Ok(mut shared) = self.shared.lock() {
            shared.closing = true;
        }
        self.post(WM_SHUTDOWN);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn load_icon(metric: i32, resource: u16) -> HICON {
    // SAFETY: plain system calls; the resource ID is passed as MAKEINTRESOURCE.
    unsafe {
        let size = GetSystemMetricsForDpi(metric, GetDpiForSystem());
        let module = GetModuleHandleW(std::ptr::null());
        let icon = LoadImageW(
            module,
            resource as usize as *const u16,
            IMAGE_ICON,
            size,
            size,
            LR_DEFAULTCOLOR,
        );
        if icon.is_null() {
            // No icon resource yet: the generic application icon.
            LoadIconW(std::ptr::null_mut(), IDI_APPLICATION)
        } else {
            icon
        }
    }
}

fn icon_data(hwnd: HWND, state: &State) -> NOTIFYICONDATAW {
    let mut data = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: ICON_ID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
        uCallbackMessage: WM_TRAY,
        ..NOTIFYICONDATAW::default()
    };
    if let Ok(shared) = state.shared.lock() {
        data.hIcon = state.speakers[usize::from(state.light.get())][shared.glyph.index()];
        wide_into(&shared.tooltip, &mut data.szTip);
    }
    data
}

fn add_icon(hwnd: HWND, state: &State) {
    let mut data = icon_data(hwnd, state);
    // SAFETY: a fully initialised structure for our own window.
    let shown = unsafe {
        let shown = Shell_NotifyIconW(NIM_ADD, &data) != 0;
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        Shell_NotifyIconW(NIM_SETVERSION, &data);
        shown
    };
    if let Ok(mut shared) = state.shared.lock() {
        shared.shown = shown;
    }
}

fn run(shared: Arc<Mutex<Shared>>, callback: Callback, promote: bool, ready: std::sync::mpsc::Sender<isize>) {
    let class = wide("VoluteTrayWindow");
    // SAFETY: registering a class with a static window procedure; failing again (already
    // registered by an earlier tray in this process) is fine.
    let hwnd = unsafe {
        let module = GetModuleHandleW(std::ptr::null());
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: module,
            lpszClassName: class.as_ptr(),
            ..WNDCLASSW::default()
        };
        RegisterClassW(&wc);
        // A hidden top-level window, not a message-only one: only top-level windows get the
        // TaskbarCreated broadcast sent when Explorer restarts.
        CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            module,
            std::ptr::null(),
        )
    };
    if hwnd.is_null() {
        let _ = ready.send(0);
        return;
    }
    let taskbar = wide("TaskbarCreated");
    let state = Box::new(State {
        shared,
        callback,
        speakers: [SPEAKER_WHITE, SPEAKER_INK]
            .map(|first| std::array::from_fn(|i| load_icon(SM_CXSMICON, first + i as u16))),
        light: Cell::new(light_taskbar()),
        balloon_icon: load_icon(SM_CXICON, ICON_RESOURCE),
        listening: Cell::new(false),
        rect: Cell::new(RECT::default()),
        promote_tries: Cell::new(if promote { PROMOTE_TRIES } else { 0 }),
        // SAFETY: a NUL-terminated name.
        taskbar_created: unsafe { RegisterWindowMessageW(taskbar.as_ptr()) },
    });
    let first_run = promote && promote_before_adding();
    add_icon(hwnd, &state);
    if first_run {
        // SAFETY: a timer on our own window, killed in WM_TIMER.
        unsafe { SetTimer(hwnd, PROMOTE_TIMER, PROMOTE_EVERY_MS, None) };
    }
    let state = Box::into_raw(state);
    // SAFETY: the window proc reads this pointer until WM_DESTROY, on this thread only.
    unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize) };
    let _ = ready.send(hwnd as isize);
    let mut msg = MSG::default();
    // SAFETY: a standard message loop for this thread's window.
    unsafe {
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        // The window is destroyed: free what it pointed to.
        let state = Box::from_raw(state);
        for icon in state.speakers.iter().flatten() {
            DestroyIcon(*icon);
        }
        DestroyIcon(state.balloon_icon);
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: set by `run` before any of our messages can arrive; null before that.
    let state = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const State;
    if state.is_null() {
        // SAFETY: default handling.
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }
    // SAFETY: the state outlives the window (freed after the loop ends).
    let state = unsafe { &*state };
    match msg {
        WM_TRAY => {
            // Version 4: the event in the low word of lparam, the anchor point in wparam.
            let event = (lparam as u32) & 0xffff;
            let x = (wparam & 0xffff) as u16 as i16 as i32;
            let y = ((wparam >> 16) & 0xffff) as u16 as i16 as i32;
            match event {
                NIN_SELECT | 0x401 /* NIN_KEYSELECT */ => (state.callback)(TrayEvent::Clicked { x, y }),
                WM_MOUSEMOVE => hovered(hwnd, state),
                WM_LBUTTONDBLCLK => (state.callback)(TrayEvent::Open),
                WM_CONTEXTMENU => show_menu(hwnd, state, x, y),
                NIN_BALLOONUSERCLICK => (state.callback)(TrayEvent::NotificationClicked),
                _ => {}
            }
            0
        }
        WM_UPDATE => {
            let mut data = icon_data(hwnd, state);
            data.uFlags = NIF_ICON | NIF_TIP | NIF_SHOWTIP;
            // SAFETY: modifying our own icon.
            unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) };
            0
        }
        WM_NOTIFY_USER => {
            let notification = state.shared.lock().ok().and_then(|mut s| s.notification.take());
            if let Some((title, body)) = notification {
                let mut data = icon_data(hwnd, state);
                data.uFlags = NIF_INFO;
                wide_into(&title, &mut data.szInfoTitle);
                wide_into(&body, &mut data.szInfo);
                data.dwInfoFlags = NIIF_USER | NIIF_LARGE_ICON;
                data.hBalloonIcon = state.balloon_icon;
                // SAFETY: modifying our own icon.
                unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) };
            }
            0
        }
        WM_TIMER if wparam == PROMOTE_TIMER => {
            promote_once(hwnd, state);
            0
        }
        // The taskbar may have turned light or dark: the speaker follows it.
        WM_SETTINGCHANGE => {
            let light = light_taskbar();
            if state.light.replace(light) != light {
                let mut data = icon_data(hwnd, state);
                data.uFlags = NIF_ICON;
                // SAFETY: modifying our own icon.
                unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) };
            }
            0
        }
        // Someone else's request to close this hidden window: Volute decides when its icon goes.
        WM_CLOSE => 0,
        // Windows is signing out, or an installer asks Volute to close (Restart Manager).
        WM_ENDSESSION if wparam != 0 => {
            (state.callback)(TrayEvent::Quit);
            0
        }
        WM_INPUT => {
            wheel(hwnd, lparam, state);
            // SAFETY: default handling, which also lets the system free the input.
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        // Only Volute's own request: the message number is not secret, and a broadcast of it
        // must not leave Volute running without its icon.
        WM_SHUTDOWN if state.shared.lock().is_ok_and(|s| s.closing) => {
            listen(hwnd, state, false);
            if let Ok(mut shared) = state.shared.lock() {
                shared.shown = false;
            }
            let data = icon_data(hwnd, state);
            // SAFETY: removing our icon, then destroying our window.
            unsafe {
                Shell_NotifyIconW(NIM_DELETE, &data);
                DestroyWindow(hwnd);
            }
            0
        }
        WM_DESTROY => {
            // SAFETY: ends this thread's message loop.
            unsafe { PostQuitMessage(0) };
            0
        }
        // Explorer restarted: the icon is gone, add it again.
        m if m == state.taskbar_created => {
            add_icon(hwnd, state);
            0
        }
        // SAFETY: default handling.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Windows 11 puts a new icon among the hidden ones, where a mixer is lost: Volute's goes next
/// to the clock, as "Other system tray icons" in the taskbar settings would put it. Once Windows
/// has a choice recorded (the user's, or this one), it is never touched again (decision 0012).
/// The record is Explorer's, per program, written when the program first adds an icon and read
/// when a process adds one, so the first run records the choice for the runs after it, and
/// those make it before adding the icon. Whether the timer is needed for the first run.
fn promote_before_adding() -> bool {
    match icon_record() {
        Some((key, false)) => {
            set_promoted(&key);
            false
        }
        Some((_, true)) => false,
        None => true,
    }
}

/// The first run: Explorer has written its record since the icon was added; the choice is
/// recorded there for the next runs.
fn promote_once(hwnd: HWND, state: &State) {
    let tries = state.promote_tries.get().saturating_sub(1);
    state.promote_tries.set(tries);
    let done = match icon_record() {
        Some((key, promoted)) => {
            if !promoted {
                set_promoted(&key);
            }
            true
        }
        None => tries == 0,
    };
    if done {
        state.promote_tries.set(0);
        // SAFETY: our own timer.
        unsafe { KillTimer(hwnd, PROMOTE_TIMER) };
    }
}

const ICON_SETTINGS: &str = r"Control Panel\NotifyIconSettings";

/// Whether the taskbar is light (Settings > Personalisation > Colours): dark when unknown, as
/// Windows 11's default.
fn light_taskbar() -> bool {
    read_dword(PERSONALIZE, "SystemUsesLightTheme") == Some(1)
}

/// Windows' record of this program's icon: its key under `ICON_SETTINGS`, and whether a
/// choice is recorded in it (`IsPromoted` present, whatever its value).
fn icon_record() -> Option<(String, bool)> {
    let exe = current_exe()?;
    let parent = wide(ICON_SETTINGS);
    let mut root: HKEY = std::ptr::null_mut();
    // SAFETY: a NUL-terminated path and a key handle to fill, closed below.
    if unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, parent.as_ptr(), 0, KEY_READ, &mut root) } != 0 {
        return None;
    }
    let mut found = None;
    for index in 0.. {
        let mut name = [0u16; 64];
        let mut len = name.len() as u32;
        // SAFETY: an open key, a buffer and its length in characters.
        let code = unsafe {
            RegEnumKeyExW(
                root,
                index,
                name.as_mut_ptr(),
                &mut len,
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        // Only the end of the list stops the walk: a key that cannot be read is skipped.
        if code == ERROR_NO_MORE_ITEMS {
            break;
        }
        if code != 0 {
            continue;
        }
        let name = String::from_utf16_lossy(&name[..len as usize]);
        let key = format!(r"{ICON_SETTINGS}\{name}");
        let ours = read_dword(&key, "UID") == Some(ICON_ID)
            && read_string(&key, "ExecutablePath").is_some_and(|path| same_exe(&path, &exe));
        if ours {
            found = Some((key.clone(), read_dword(&key, "IsPromoted").is_some()));
            break;
        }
    }
    // SAFETY: the key opened above.
    unsafe { RegCloseKey(root) };
    found
}

/// Whether Explorer's `recorded` path names `exe`, ignoring case. Explorer writes a path under a
/// known folder as the folder's GUID and the rest (`{GUID}\Volute\volute.exe`): the GUID is
/// resolved first, so only the very same path matches, never another copy of Volute.
fn same_exe(recorded: &str, exe: &str) -> bool {
    let resolved = resolve_known_folder(recorded);
    // Compared as characters, never cut at a byte offset: paths may hold any letter.
    resolved.as_deref().unwrap_or(recorded).to_lowercase() == exe.to_lowercase()
}

/// `{GUID}\rest` with the known folder's path in place of its GUID.
fn resolve_known_folder(recorded: &str) -> Option<String> {
    let (guid, rest) = recorded.strip_prefix('{')?.split_once('}')?;
    Some(format!("{}{rest}", known_folder(&parse_guid(guid)?)?))
}

/// `6D809377-6AF0-444B-8957-A3773F02200E`, the form between braces.
fn parse_guid(text: &str) -> Option<windows_sys::core::GUID> {
    let dashes = text.char_indices().filter(|&(_, c)| c == '-').map(|(i, _)| i).collect::<Vec<_>>();
    if text.len() != 36 || dashes != [8, 13, 18, 23] {
        return None;
    }
    let hex: String = text.chars().filter(|&c| c != '-').collect();
    u128::from_str_radix(&hex, 16).ok().map(windows_sys::core::GUID::from_u128)
}

fn known_folder(id: &windows_sys::core::GUID) -> Option<String> {
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::SHGetKnownFolderPath;
    let mut path: *mut u16 = std::ptr::null_mut();
    // SAFETY: a GUID and a pointer to fill with a NUL-terminated string, which the shell
    // allocates and CoTaskMemFree releases, even on failure.
    unsafe {
        let ok = SHGetKnownFolderPath(id, 0 as _, std::ptr::null_mut(), &mut path) >= 0 && !path.is_null();
        let text = ok.then(|| {
            let len = (0..).take_while(|&i| *path.add(i) != 0).count();
            String::from_utf16_lossy(std::slice::from_raw_parts(path, len))
        });
        CoTaskMemFree(path.cast());
        text
    }
}

fn current_exe() -> Option<String> {
    let mut path = [0u16; 1024];
    // SAFETY: our own module's path into a buffer of the length given.
    let len =
        unsafe { GetModuleFileNameW(std::ptr::null_mut(), path.as_mut_ptr(), path.len() as u32) }
            as usize;
    (len > 0 && len < path.len()).then(|| String::from_utf16_lossy(&path[..len]))
}

fn read_dword(key: &str, value: &str) -> Option<u32> {
    let (key, value) = (wide(key), wide(value));
    let mut data = 0u32;
    let mut len = 4u32;
    // SAFETY: NUL-terminated names and a DWORD with its size.
    let code = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&raw mut data).cast(),
            &mut len,
        )
    };
    (code == 0).then_some(data)
}

fn read_string(key: &str, value: &str) -> Option<String> {
    let (key, value) = (wide(key), wide(value));
    let mut data = [0u16; 1024];
    let mut len = (data.len() * 2) as u32;
    // SAFETY: NUL-terminated names; the buffer and its size in bytes match.
    let code = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            data.as_mut_ptr().cast(),
            &mut len,
        )
    };
    if code != 0 {
        return None;
    }
    let units = &data[..(len as usize / 2).min(data.len())];
    Some(String::from_utf16_lossy(
        units.split(|&u| u == 0).next().unwrap_or(units),
    ))
}

fn set_promoted(key: &str) -> bool {
    let (key, value) = (wide(key), wide("IsPromoted"));
    let one = 1u32;
    // SAFETY: NUL-terminated names and a DWORD with its size.
    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            REG_DWORD,
            (&raw const one).cast(),
            4,
        ) == 0
    }
}

/// The pointer moved over the icon: listen to the wheel while it stays there (decision 0007).
fn hovered(hwnd: HWND, state: &State) {
    let id = NOTIFYICONIDENTIFIER {
        cbSize: std::mem::size_of::<NOTIFYICONIDENTIFIER>() as u32,
        hWnd: hwnd,
        uID: ICON_ID,
        ..NOTIFYICONIDENTIFIER::default()
    };
    let mut rect = RECT::default();
    // SAFETY: a sized identifier of our own icon, and a rectangle to fill.
    if unsafe { Shell_NotifyIconGetRect(&id, &mut rect) } == 0 {
        state.rect.set(rect);
        if pointer_in(rect) {
            // Registered again on every hover rather than trusted from the flag: another
            // registration in this process (winit's, at start) may have taken the mouse since.
            state.listening.set(false);
            listen(hwnd, state, true);
        }
    }
}

fn pointer_in(rect: RECT) -> bool {
    let mut p = POINT::default();
    // SAFETY: a point to fill.
    let ok = unsafe { GetCursorPos(&mut p) } != 0;
    ok && p.x >= rect.left && p.x < rect.right && p.y >= rect.top && p.y < rect.bottom
}

/// Registers this window for raw mouse input, or removes the registration.
fn listen(hwnd: HWND, state: &State, on: bool) {
    if state.listening.get() == on {
        return;
    }
    let device = RAWINPUTDEVICE {
        // The generic desktop page, mouse usage.
        usUsagePage: 0x01,
        usUsage: 0x02,
        dwFlags: if on { RIDEV_INPUTSINK } else { RIDEV_REMOVE },
        hwndTarget: if on { hwnd } else { std::ptr::null_mut() },
    };
    // SAFETY: one sized device description.
    let size = std::mem::size_of::<RAWINPUTDEVICE>() as u32;
    if unsafe { RegisterRawInputDevices(&device, 1, size) } != 0 {
        state.listening.set(on);
    }
}

/// Raw mouse input while the pointer is over the icon: only the wheel is read, and only while
/// the pointer is still inside; the first input outside stops the listening (decision 0007).
fn wheel(hwnd: HWND, lparam: LPARAM, state: &State) {
    if !pointer_in(state.rect.get()) {
        listen(hwnd, state, false);
        return;
    }
    let mut raw = RAWINPUT::default();
    let mut size = std::mem::size_of::<RAWINPUT>() as u32;
    // SAFETY: the input handle of this WM_INPUT, read into a buffer of the size given.
    let read = unsafe {
        GetRawInputData(
            lparam as HRAWINPUT,
            RID_INPUT,
            (&raw mut raw).cast(),
            &mut size,
            std::mem::size_of::<RAWINPUTHEADER>() as u32,
        )
    };
    if read == u32::MAX || raw.header.dwType != RIM_TYPEMOUSE {
        return;
    }
    // SAFETY: a mouse input, so the union's mouse member is the one written.
    let (flags, data) = unsafe {
        let buttons = raw.data.mouse.Anonymous.Anonymous;
        (buttons.usButtonFlags, buttons.usButtonData)
    };
    if u32::from(flags) & RI_MOUSE_WHEEL != 0 {
        (state.callback)(TrayEvent::Wheel {
            delta: i32::from(data as i16),
        });
    }
}

fn show_menu(hwnd: HWND, state: &State, x: i32, y: i32) {
    let Ok((labels, muted)) = state.shared.lock().map(|s| (s.menu.clone(), s.glyph == TrayGlyph::Muted)) else {
        return;
    };
    let (open, mute, settings, quit) =
        (wide(&labels.open), wide(&labels.mute), wide(&labels.settings), wide(&labels.quit));
    // SAFETY: a menu owned and destroyed here. Our window must be in the foreground for the
    // menu to close when the user clicks elsewhere, and WM_NULL afterwards lets it close
    // cleanly (documented TrackPopupMenu behaviour).
    let command = unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return;
        }
        AppendMenuW(menu, MF_STRING, MENU_OPEN, open.as_ptr());
        // Mute all: the master output, checked while it is muted.
        AppendMenuW(menu, MF_STRING | if muted { MF_CHECKED } else { 0 }, MENU_MUTE, mute.as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        AppendMenuW(menu, MF_STRING, MENU_SETTINGS, settings.as_ptr());
        AppendMenuW(menu, MF_STRING, MENU_QUIT, quit.as_ptr());
        SetForegroundWindow(hwnd);
        let command = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
            x,
            y,
            0,
            hwnd,
            std::ptr::null(),
        );
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);
        command
    };
    match command as usize {
        MENU_OPEN => (state.callback)(TrayEvent::Open),
        MENU_QUIT => (state.callback)(TrayEvent::Quit),
        MENU_MUTE => (state.callback)(TrayEvent::ToggleMute),
        MENU_SETTINGS => (state.callback)(TrayEvent::Settings),
        _ => {}
    }
}

/// The work area (screen minus taskbar) of the monitor holding (`x`, `y`).
pub(crate) fn work_area(x: i32, y: i32) -> Option<crate::Rect> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
    };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..MONITORINFO::default()
    };
    // SAFETY: plain queries into a sized structure.
    let ok = unsafe {
        let monitor = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        GetMonitorInfoW(monitor, &mut info)
    };
    (ok != 0).then(|| crate::Rect {
        x: info.rcWork.left,
        y: info.rcWork.top,
        width: info.rcWork.right - info.rcWork.left,
        height: info.rcWork.bottom - info.rcWork.top,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Adds an icon for a moment and removes it: the tray thread starts and stops cleanly.
    #[test]
    fn starts_and_stops() {
        let Some(tray) = Tray::new(
            "Volute test",
            TrayMenu {
                open: "Open".into(),
                mute: "Mute all".into(),
                settings: "Settings".into(),
                quit: "Quit".into(),
            },
            false,
            Arc::new(|_| {}),
        ) else {
            panic!("no tray window");
        };
        tray.set_tooltip("Volute test · 50 %");
        tray.set_glyph(TrayGlyph::Muted);
        tray.set_glyph(TrayGlyph::Waves(2));
        drop(tray);
    }

    #[test]
    fn knows_its_own_icon_record() {
        let exe = r"C:\Users\a\AppData\Local\Programs\Volute\volute.exe";
        assert!(same_exe(exe, exe));
        assert!(same_exe(&exe.to_uppercase(), exe));
        assert!(!same_exe(r"C:\Elsewhere\volute.exe", exe));
        // A known folder written as its GUID (Program Files) is resolved, then compared whole.
        let program_files = known_folder(&parse_guid("905E63B6-C1BF-494E-B29C-65B732D3D21A").unwrap()).unwrap();
        let installed = format!(r"{program_files}\Volute\volute.exe");
        assert!(same_exe(r"{905E63B6-C1BF-494E-B29C-65B732D3D21A}\Volute\volute.exe", &installed));
        // Another copy of Volute ending the same way is not this one.
        assert!(!same_exe(r"{905E63B6-C1BF-494E-B29C-65B732D3D21A}\Volute\volute.exe", r"D:\Volute\volute.exe"));
        assert!(!same_exe(r"{not-a-guid}\Volute\volute.exe", r"D:\Volute\volute.exe"));
        // Any letter in the path, wherever the lengths fall: nothing is cut inside a character.
        for tail in ["", "x", "\\Volute\\volute.exe", "é", "日本"] {
            let _ = same_exe(&format!("{{6D809377-6AF0-444B-8957-A3773F02200E}}{tail}"), r"C:\Users\Clément\日本\volute.exe");
        }
        assert!(parse_guid("6D809377-6AF0-444B-8957-A3773F02200").is_none());
    }
}
