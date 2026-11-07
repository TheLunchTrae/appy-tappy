//! The single-instance guard: a named mutex in the user's session.

use std::io;

use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::HSTRING;

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
