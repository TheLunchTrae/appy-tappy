//! The Windows adapter: each function turns Windows API calls into a typed
//! value or an error and makes no decisions of its own.
//!
//! Each host service has its own module here. The services that run a thread
//! of their own start it with [`wait_for_start`] and end it with [`join`].

use std::io;
use std::ptr;
use std::sync::mpsc::Receiver;
use std::thread::JoinHandle;

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::{
    GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_LOWEST,
};
use windows::Win32::UI::WindowsAndMessaging::{
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SetWindowPos,
};

mod autostart;
mod clock;
mod instance;
mod reload;
mod tray;
mod window_watch;

pub(crate) use autostart::{autostart_enabled, disable_autostart, enable_autostart};
pub(crate) use clock::local_time;
pub(crate) use instance::InstanceGuard;
pub(crate) use reload::{ReloadWatch, request_reload};
pub(crate) use tray::Tray;
pub(crate) use window_watch::WindowWatch;

/// Waits for a new thread to report that it started and returns what it
/// sent. When it could not start, waits for it to end and returns why.
fn wait_for_start<T>(
    started: &Receiver<io::Result<T>>,
    thread: JoinHandle<io::Result<()>>,
    name: &str,
) -> io::Result<(T, JoinHandle<io::Result<()>>)> {
    match started.recv() {
        Ok(Ok(value)) => Ok((value, thread)),
        Ok(Err(error)) => {
            // The thread reported the failure and returns right after.
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the thread's own result is Ok once it reported the start error, which is returned instead"
            )]
            let _ = thread.join();
            Err(error)
        },
        Err(_closed) => Err(join(thread, name).err().unwrap_or_else(|| {
            io::Error::other(format!("the {name} thread ended before it started"))
        })),
    }
}

/// Waits for a thread and returns its result, or an error that names it
/// when it panicked.
fn join(thread: JoinHandle<io::Result<()>>, name: &str) -> io::Result<()> {
    thread.join().unwrap_or_else(|payload| {
        Err(io::Error::other(format!(
            "the {name} thread panicked: {}",
            crate::panic_reason(payload.as_ref())
        )))
    })
}

pub(crate) fn show_without_focus(window: usize) -> io::Result<()> {
    let window = HWND(ptr::without_provenance_mut(window));
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: the call takes any handle by value and fails for one that names
    // no window.
    unsafe {
        SetWindowPos(
            window,
            None,
            0,
            0,
            0,
            0,
            SWP_SHOWWINDOW | SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
        )
    }
    .map_err(io::Error::from)
}

pub(crate) fn lower_thread_priority() -> io::Result<()> {
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: the pseudo handle of the current thread is always valid, and the
    // priority is one of the documented levels.
    unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_LOWEST) }
        .map_err(io::Error::from)
}
