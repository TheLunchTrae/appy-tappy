//! The single-instance guard: a named mutex in the user's session.

use std::io;

use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::System::RemoteDesktop::{
    ProcessIdToSessionId, WTS_PROCESS_INFOW, WTSEnumerateProcessesExW, WTSFreeMemoryExW,
    WTSTypeProcessInfoLevel0,
};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::{HSTRING, PWSTR};

pub(crate) fn process_runs_in_session(executable: &str) -> io::Result<bool> {
    let mut session = 0;
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: the PID is this process's and the output lives through the call.
    unsafe { ProcessIdToSessionId(std::process::id(), &raw mut session) }?;
    let mut level = 0;
    let mut buffer = PWSTR::null();
    let mut count = 0;
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: each output is writable. Level zero requests WTS_PROCESS_INFOW
    // entries for this session on the local server.
    unsafe {
        WTSEnumerateProcessesExW(
            None,
            &raw mut level,
            session,
            &raw mut buffer,
            &raw mut count,
        )
    }?;
    if buffer.is_null() {
        return Ok(false);
    }
    #[expect(unsafe_code, reason = "reads the process records allocated by Windows")]
    // SAFETY: the successful call returned count level-zero entries with
    // null-terminated names, all valid until WTSFreeMemoryExW below.
    let found =
        unsafe { std::slice::from_raw_parts(buffer.0.cast::<WTS_PROCESS_INFOW>(), count as usize) }
            .iter()
            .any(|entry| {
                if entry.pProcessName.is_null() {
                    return false;
                }
                #[expect(unsafe_code, reason = "reads a process name allocated by Windows")]
                // SAFETY: the checked name belongs to the live buffer.
                let name = unsafe { entry.pProcessName.as_wide() };
                String::from_utf16_lossy(name).eq_ignore_ascii_case(executable)
            });
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: this is the buffer, information level and count from the successful enumeration.
    unsafe { WTSFreeMemoryExW(WTSTypeProcessInfoLevel0, buffer.0.cast(), count) }?;
    Ok(found)
}

/// A named mutex that marks the one running copy.
#[derive(Debug)]
pub(crate) struct InstanceGuard(HANDLE);

impl InstanceGuard {
    pub(crate) fn acquire(name: &str) -> io::Result<Option<Self>> {
        let name = HSTRING::from(name);
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `name` is a null-terminated wide string that outlives the call, and
        // the default security attributes need no pointer.
        let handle = unsafe { CreateMutexW(None, false, &name) }?;
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the call takes no arguments and reads this thread's last error,
        // which CreateMutexW set on success.
        let existed = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        let guard = Self(handle);
        Ok((!existed).then_some(guard))
    }
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the handle came from a successful CreateMutexW and is closed once.
        let closed = unsafe { CloseHandle(self.0) };
        #[expect(
            clippy::let_underscore_must_use,
            reason = "closing a handle this value owns cannot be retried, and Drop has no caller to report to"
        )]
        let _ = closed;
    }
}

#[cfg(test)]
mod tests {
    use std::process;

    use super::*;

    #[test]
    fn the_current_test_process_is_visible_in_its_session() {
        let executable = std::env::current_exe().expect("test executable");
        let name = executable
            .file_name()
            .expect("file name")
            .to_string_lossy()
            .to_ascii_uppercase();
        assert!(process_runs_in_session(&name).expect("session processes"));
        assert!(
            !process_runs_in_session("appytappy-no-such-process.exe").expect("session processes")
        );
    }

    #[test]
    fn a_second_guard_with_the_same_name_is_refused_until_the_first_drops() {
        let name = format!(r"Local\AppyTappy.Test.{}", process::id());
        let first = InstanceGuard::acquire(&name)
            .expect("acquire")
            .expect("first guard");
        assert!(InstanceGuard::acquire(&name).expect("acquire").is_none());
        drop(first);
        assert!(InstanceGuard::acquire(&name).expect("acquire").is_some());
    }
}
