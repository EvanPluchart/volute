//! The tray icon on Windows (decision 0004), ported from Vigie: `Shell_NotifyIconW` with
//! `NOTIFYICON_VERSION_4`, driven by a hidden window on a thread of its own. The thread sleeps
//! in `GetMessageW`, so the icon costs nothing between events.

use std::cell::Cell;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::UI::Input::{
    GetRawInputData, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER, RID_INPUT,
    RIDEV_INPUTSINK, RIDEV_REMOVE, RIM_TYPEMOUSE, RegisterRawInputDevices,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::{GetDpiForSystem, GetSystemMetricsForDpi};
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION,
    NIN_SELECT, NOTIFYICON_VERSION_4, NOTIFYICONDATAW, NOTIFYICONIDENTIFIER,
    Shell_NotifyIconGetRect, Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu,
    DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetMessageW, GetWindowLongPtrW, HICON,
    IDI_APPLICATION, IMAGE_ICON, LR_DEFAULTCOLOR, LoadIconW, LoadImageW, MF_SEPARATOR, MF_STRING,
    MSG, PostMessageW, PostQuitMessage, RegisterClassW, RegisterWindowMessageW, SM_CXSMICON,
    SetForegroundWindow, SetWindowLongPtrW, TPM_BOTTOMALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    TrackPopupMenu, TranslateMessage, WM_APP, WM_CLOSE, WM_CONTEXTMENU, WM_DESTROY,
    WM_ENDSESSION, WM_INPUT, WM_LBUTTONDBLCLK, WM_MOUSEMOVE, WM_NULL, WNDCLASSW, WS_OVERLAPPED,
    GetCursorPos, RI_MOUSE_WHEEL,
};

use crate::{TrayEvent, TrayMenu};

/// Messages the icon sends to the window, and the ones the app posts to it.
const WM_TRAY: u32 = WM_APP + 1;
const WM_UPDATE: u32 = WM_APP + 2;
/// Volute removes its icon. Not WM_CLOSE: other programs send that one (`taskkill` without
/// `/F` does), and the icon must not vanish while Volute runs hidden behind it.
const WM_SHUTDOWN: u32 = WM_APP + 4;
const ICON_ID: u32 = 1;
/// The application icon in the executable's resources, and the muted tray icon.
const ICON_RESOURCE: u16 = 1;
const MUTED_RESOURCE: u16 = 2;
const MENU_OPEN: usize = 1;
const MENU_QUIT: usize = 2;

type Callback = Arc<dyn Fn(TrayEvent) + Send + Sync>;

/// What the app changes from its threads; the tray thread applies it on `WM_UPDATE`.
struct Shared {
    tooltip: String,
    menu: TrayMenu,
    /// The icon is in the notification area (adding it fails while Explorer is down).
    shown: bool,
    /// Show the muted icon.
    muted: bool,
}

/// Lives on the tray thread, pointed to by the window's user data.
struct State {
    shared: Arc<Mutex<Shared>>,
    callback: Callback,
    icon: HICON,
    /// The muted icon, resource 2 (the application icon when there is none).
    muted_icon: HICON,
    taskbar_created: u32,
    /// Raw mouse input is registered: the pointer is over the icon (decision 0007).
    listening: Cell<bool>,
    /// Where the icon was, last time the pointer moved over it.
    rect: Cell<RECT>,
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
    pub(crate) fn new(tooltip: &str, menu: TrayMenu, callback: Callback) -> Option<Tray> {
        let shared = Arc::new(Mutex::new(Shared {
            tooltip: tooltip.to_owned(),
            menu,
            shown: false,
            muted: false,
        }));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<isize>();
        let thread_shared = shared.clone();
        let thread = std::thread::Builder::new()
            .name("volute-tray".into())
            .spawn(move || run(thread_shared, callback, ready_tx))
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

    pub(crate) fn set_muted(&self, muted: bool) {
        if let Ok(mut shared) = self.shared.lock() {
            if shared.muted == muted {
                return;
            }
            shared.muted = muted;
        }
        self.post(WM_UPDATE);
    }

    /// Whether the icon is on screen: hiding Volute behind a missing icon would lose it.
    pub(crate) fn is_shown(&self) -> bool {
        self.shared.lock().is_ok_and(|s| s.shown)
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
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
        hIcon: state.icon,
        ..NOTIFYICONDATAW::default()
    };
    if let Ok(shared) = state.shared.lock() {
        if shared.muted {
            data.hIcon = state.muted_icon;
        }
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

fn run(shared: Arc<Mutex<Shared>>, callback: Callback, ready: std::sync::mpsc::Sender<isize>) {
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
        icon: load_icon(SM_CXSMICON, ICON_RESOURCE),
        muted_icon: load_icon(SM_CXSMICON, MUTED_RESOURCE),
        listening: Cell::new(false),
        rect: Cell::new(RECT::default()),
        // SAFETY: a NUL-terminated name.
        taskbar_created: unsafe { RegisterWindowMessageW(taskbar.as_ptr()) },
    });
    add_icon(hwnd, &state);
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
        DestroyIcon(state.icon);
        DestroyIcon(state.muted_icon);
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
        WM_SHUTDOWN => {
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
    let Ok(labels) = state.shared.lock().map(|s| s.menu.clone()) else {
        return;
    };
    let (open, quit) = (wide(&labels.open), wide(&labels.quit));
    // SAFETY: a menu owned and destroyed here. Our window must be in the foreground for the
    // menu to close when the user clicks elsewhere, and WM_NULL afterwards lets it close
    // cleanly (documented TrackPopupMenu behaviour).
    let command = unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return;
        }
        AppendMenuW(menu, MF_STRING, MENU_OPEN, open.as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
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
                quit: "Quit".into(),
            },
            Arc::new(|_| {}),
        ) else {
            panic!("no tray window");
        };
        tray.set_tooltip("Volute test · 50 %");
        drop(tray);
    }
}
