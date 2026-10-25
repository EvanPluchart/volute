//! "Start with Windows" (decision 0004): the `Volute` value under the user's `Run` key, the one
//! the installer sets. Only that value is ever read or written.

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "Volute";

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

/// The command Windows runs at sign-in for Volute, if any.
pub(crate) fn command() -> Option<String> {
    let (key, value) = (wide(RUN), wide(VALUE));
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
    if code != ERROR_SUCCESS {
        return None;
    }
    let units = &data[..(len as usize / 2).min(data.len())];
    Some(String::from_utf16_lossy(units.split(|&u| u == 0).next().unwrap_or(units)))
}

/// Starts `exe` at sign-in, or stops starting Volute.
pub(crate) fn set(exe: Option<&std::path::Path>) -> bool {
    let (key, value) = (wide(RUN), wide(VALUE));
    match exe {
        Some(exe) => {
            // Quoted, as the installer writes it: a path with spaces stays one program.
            let data = wide(&format!("\"{}\"", exe.display()));
            // SAFETY: NUL-terminated names and data, its size in bytes with the NUL.
            unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    value.as_ptr(),
                    REG_SZ,
                    data.as_ptr().cast(),
                    (data.len() * 2) as u32,
                ) == ERROR_SUCCESS
            }
        }
        None => {
            // SAFETY: NUL-terminated names.
            let code = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr()) };
            code == ERROR_SUCCESS || code == ERROR_FILE_NOT_FOUND
        }
    }
}
