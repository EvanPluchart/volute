//! Keeps winit's hidden message window alive when another program asks the process to close.
//!
//! `taskkill` without `/F`, like any program, sends WM_CLOSE to every top-level window of a
//! process. winit's "Winit Thread Event Target" is a hidden top-level window that leaves WM_CLOSE
//! to `DefWindowProc`, which destroys it, and every event posted to the UI thread afterwards is
//! lost. Hidden in the tray, Volute would then keep its icon but never answer it, nor a second
//! launch: the process nobody can reach that decision 0004 rules out. Volute's own windows decide
//! for themselves (the main window quits, the tray window ignores it); this one now ignores it.

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{EnumThreadWindows, GetClassNameW, WM_CLOSE};

/// winit's class name for the window it posts the UI thread's events to (winit 0.30).
const EVENT_TARGET_CLASS: &str = "Winit Thread Event Target";
const SUBCLASS_ID: usize = 0x766f_6c75;

/// Guards the event target windows of the calling thread; how many were found.
pub(crate) fn guard_event_target() -> usize {
    let mut count = 0usize;
    // SAFETY: enumerating this thread's windows; the callback only writes `count`, which lives
    // through the call.
    unsafe {
        EnumThreadWindows(GetCurrentThreadId(), Some(each), (&raw mut count) as LPARAM);
    }
    count
}

unsafe extern "system" fn each(hwnd: HWND, data: LPARAM) -> i32 {
    let mut class = [0u16; 64];
    // SAFETY: a buffer that outlives the call, its length passed.
    let len = unsafe { GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32) };
    if String::from_utf16_lossy(&class[..len.max(0) as usize]) == EVENT_TARGET_CLASS {
        // SAFETY: a window of this thread, subclassed with a static procedure; `data` is the
        // counter given by `guard_event_target`.
        unsafe {
            if SetWindowSubclass(hwnd, Some(ignore_close), SUBCLASS_ID, 0) != 0 {
                *(data as *mut usize) += 1;
            }
        }
    }
    1
}

unsafe extern "system" fn ignore_close(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if msg == WM_CLOSE {
        return 0;
    }
    // SAFETY: everything else goes on to winit.
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}
