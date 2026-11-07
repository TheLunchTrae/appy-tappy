//! The local wall-clock time.

use windows::Win32::System::SystemInformation::GetLocalTime;

use crate::LocalTime;

#[expect(
    clippy::unnecessary_wraps,
    reason = "the portable signature returns None where the runtime does not run"
)]
pub(crate) fn local_time() -> Option<LocalTime> {
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: the call takes no arguments and returns the time by value.
    let time = unsafe { GetLocalTime() };
    Some(LocalTime {
        year: time.wYear,
        month: time.wMonth,
        day: time.wDay,
        hour: time.wHour,
        minute: time.wMinute,
        second: time.wSecond,
    })
}
