//! Opening a web page in the default browser.

use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

/// `ShellExecute` of an `https:` URI: the browser registered for it opens the page. No program
/// is looked up by name (decision 0005).
pub(crate) fn open_url(url: &str) -> bool {
    let (verb, target) = (wide("open"), wide(url));
    // SAFETY: NUL-terminated strings; a null window, parameters and directory are allowed.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // Values above 32 mean success.
    result as usize > 32
}
