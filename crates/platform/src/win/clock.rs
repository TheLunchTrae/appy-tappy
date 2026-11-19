//! The local wall-clock time.

use windows::Win32::System::SystemInformation::GetLocalTime;

use crate::LocalTime;

impl LocalTime {
    /// The current local time, or `None` where AppyTappy does not run.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "the portable signature returns None where the runtime does not run"
    )]
    #[must_use]
    pub fn now() -> Option<Self> {
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the call takes no arguments and returns the time by value.
        let time = unsafe { GetLocalTime() };
        Some(Self {
            year: time.wYear,
            month: time.wMonth,
            day: time.wDay,
            hour: time.wHour,
            minute: time.wMinute,
            second: time.wSecond,
        })
    }
}
