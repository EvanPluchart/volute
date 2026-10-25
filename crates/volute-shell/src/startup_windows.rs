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

/// The flag Windows starts Volute with at sign-in: it then waits behind its tray icon instead of
/// opening the flyout, as a launch by hand does.
pub(crate) const STARTUP_FLAG: &str = "--startup";

/// The program a `Run` command starts: `"C:\Programs\Volute\volute.exe" --startup` gives the
/// path.
pub(crate) fn program(command: &str) -> &str {
    let command = command.trim();
    match command.strip_prefix('"') {
        Some(rest) => rest.split('"').next().unwrap_or(rest),
        None => command
            .strip_suffix(STARTUP_FLAG)
            .unwrap_or(command)
            .trim_end(),
    }
}

/// Starts `exe` at sign-in, or stops starting Volute.
pub(crate) fn set(exe: Option<&std::path::Path>) -> bool {
    let (key, value) = (wide(RUN), wide(VALUE));
    match exe {
        Some(exe) => {
            // Quoted, as the installer writes it: a path with spaces stays one program.
            let data = wide(&format!("\"{}\" {STARTUP_FLAG}", exe.display()));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_program_out_of_a_command() {
        let path = r"C:\Users\A B\AppData\Local\Programs\Volute\volute.exe";
        assert_eq!(program(&format!("\"{path}\" {STARTUP_FLAG}")), path);
        // As M1's installer wrote it, and unquoted.
        assert_eq!(program(&format!("\"{path}\"")), path);
        assert_eq!(
            program(r"C:\Volute\volute.exe --startup"),
            r"C:\Volute\volute.exe"
        );
        assert_eq!(program(r"C:\Volute\volute.exe"), r"C:\Volute\volute.exe");
    }
}
