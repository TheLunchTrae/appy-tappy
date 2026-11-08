//! Autostart at sign-in, through a value under the user's `Run` registry key.

use std::ffi::OsStr;
use std::io;
use std::iter;
use std::mem::size_of_val;
use std::os::windows::ffi::OsStrExt;

use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows::core::HSTRING;

/// The user's `Run` key, relative to `HKEY_CURRENT_USER`.
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

pub(crate) fn enable_autostart(value: &str, command: &OsStr) -> io::Result<()> {
    let command: Vec<u16> = command.encode_wide().chain(iter::once(0)).collect();
    let bytes = u32::try_from(size_of_val(command.as_slice())).map_err(io::Error::other)?;
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: the key and value names are null-terminated wide strings, and
    // `command` holds `bytes` bytes of a null-terminated wide string; all outlive
    // the call, which copies them.
    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(RUN_KEY),
            &HSTRING::from(value),
            REG_SZ.0,
            Some(command.as_ptr().cast()),
            bytes,
        )
    };
    status.ok().map_err(io::Error::from)
}

pub(crate) fn disable_autostart(value: &str) -> io::Result<()> {
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: the key and value names are null-terminated wide strings that
    // outlive the call.
    let status = unsafe {
        RegDeleteKeyValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(RUN_KEY),
            &HSTRING::from(value),
        )
    };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    status.ok().map_err(io::Error::from)
}

pub(crate) fn autostart_enabled(value: &str) -> io::Result<bool> {
    let mut bytes = 0_u32;
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: the key and value names are null-terminated wide strings that
    // outlive the call, which asks only for the data's size and writes it to
    // `bytes`.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(RUN_KEY),
            &HSTRING::from(value),
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&raw mut bytes),
        )
    };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(false);
    }
    status.ok().map(|()| true).map_err(io::Error::from)
}

#[cfg(test)]
mod tests {
    use std::process;

    use super::*;

    fn run_value(value: &str) -> Option<Vec<u16>> {
        let mut data = vec![0_u16; 1024];
        let mut bytes = u32::try_from(size_of_val(data.as_slice())).expect("fits");
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `data` is writable for `bytes` bytes, and the call writes at most
        // that many and stores the count in `bytes`.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                &HSTRING::from(RUN_KEY),
                &HSTRING::from(value),
                RRF_RT_REG_SZ,
                None,
                Some(data.as_mut_ptr().cast()),
                Some(&raw mut bytes),
            )
        };
        if status == ERROR_FILE_NOT_FOUND {
            return None;
        }
        status.ok().expect("read the Run value");
        let units = usize::try_from(bytes).expect("fits") / 2;
        data.truncate(units.saturating_sub(1));
        Some(data)
    }

    #[test]
    fn autostart_writes_the_command_and_removes_it() {
        let value = format!("AppyTappyTest{}", process::id());
        let command = r#""C:\Program Files\AppyTappy\appytappy-runtime.exe""#;
        assert!(!autostart_enabled(&value).expect("read"));
        enable_autostart(&value, OsStr::new(command)).expect("enable");
        let expected: Vec<u16> = command.encode_utf16().collect();
        assert_eq!(run_value(&value), Some(expected));
        assert!(autostart_enabled(&value).expect("read"));
        disable_autostart(&value).expect("disable");
        assert_eq!(run_value(&value), None);
        assert!(!autostart_enabled(&value).expect("read"));
        disable_autostart(&value).expect("an absent value is not an error");
    }
}
