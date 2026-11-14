//! Reload requests: a named event that the setup app sets and the runtime's
//! reload thread waits on.

use std::io;
use std::ptr;
use std::sync::mpsc::{self, Sender, SyncSender};
use std::thread::{self, JoinHandle};

use windows::Win32::Foundation::{
    CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, ERROR_FILE_NOT_FOUND, HANDLE,
    WAIT_OBJECT_0,
};
use windows::Win32::System::Threading::{
    CreateEventW, EVENT_MODIFY_STATE, GetCurrentProcess, INFINITE, OpenEventW, SetEvent,
    WaitForMultipleObjects,
};
use windows::core::HSTRING;

use crate::ReloadRequest;

use super::{join, wait_for_start};

/// An event handle, closed on drop.
#[derive(Debug)]
struct Event(HANDLE);

impl Event {
    /// The handle as an address, because a handle may not cross threads but
    /// waiting on it or setting it from any thread is safe.
    fn address(&self) -> usize {
        self.0.0.addr()
    }

    /// A second handle to the event at `address`, which this thread owns
    /// and closes. The handle at `address` must stay open until this returns.
    fn duplicate(address: usize) -> io::Result<Self> {
        let source = HANDLE(ptr::without_provenance_mut(address));
        let mut copy = HANDLE::default();
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `source` is an open event handle of this process, as the caller
        // promises, and `copy` is a writable HANDLE.
        unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                source,
                GetCurrentProcess(),
                &raw mut copy,
                0,
                false,
                DUPLICATE_SAME_ACCESS,
            )
        }?;
        Ok(Self(copy))
    }

    /// Signals the event.
    fn set(&self) -> io::Result<()> {
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the handle came from a successful CreateEventW or OpenEventW and
        // stays open until this value drops.
        unsafe { SetEvent(self.0) }.map_err(io::Error::from)
    }
}

impl Drop for Event {
    fn drop(&mut self) {
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the handle came from a successful CreateEventW or OpenEventW and
        // is closed once.
        let closed = unsafe { CloseHandle(self.0) };
        #[expect(
            clippy::let_underscore_must_use,
            reason = "closing a handle this value owns cannot be retried, and Drop has no caller to report to"
        )]
        let _ = closed;
    }
}

/// The thread that waits for reload requests.
#[derive(Debug)]
pub(crate) struct ReloadWatch {
    thread: Option<JoinHandle<io::Result<()>>>,
    /// Ends the thread's wait. The thread waits on its own handle to the
    /// same event, so this one can close without waiting for the thread.
    stop: Event,
}

impl ReloadWatch {
    pub(crate) fn start<E>(name: &str, events: Sender<E>) -> io::Result<Self>
    where
        E: From<ReloadRequest> + Send + 'static,
    {
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the default security attributes and no name need no pointers.
        let stop = Event(unsafe { CreateEventW(None, true, false, None) }?);
        let stop_address = stop.address();
        let name = HSTRING::from(name);
        let (ready, started) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("reload requests".to_owned())
            .spawn(move || watch_reload(&name, stop_address, &events, &ready))?;
        let ((), thread) = wait_for_start(&started, thread, "reload requests")?;
        Ok(Self {
            thread: Some(thread),
            stop,
        })
    }

    pub(crate) fn stop(mut self) -> io::Result<()> {
        let Some(thread) = self.thread.take() else {
            return Ok(());
        };
        let set = self.stop.set();
        let joined = join(thread, "reload requests");
        set.and(joined)
    }
}

impl Drop for ReloadWatch {
    /// Tells the thread to end without waiting for it; `stop` waits.
    fn drop(&mut self) {
        if self.thread.take().is_some() {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "Drop has no caller to report to; `stop` reports the same result"
            )]
            let _ = self.stop.set();
        }
    }
}

/// The reload thread: opens the named event, reports the start, and sends a
/// request each time the event is set, until the stop event is set or nobody
/// receives requests.
fn watch_reload<E: From<ReloadRequest>>(
    name: &HSTRING,
    stop: usize,
    events: &Sender<E>,
    ready: &SyncSender<io::Result<()>>,
) -> io::Result<()> {
    // The caller keeps its handle open until this thread reports its start.
    let stop = match Event::duplicate(stop) {
        Ok(stop) => stop,
        Err(error) => return ready.send(Err(error)).map_err(io::Error::other),
    };
    // An auto-reset event: each wait that returns it clears it, so a burst
    // of requests while one is handled becomes one more request.
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: `name` is a null-terminated wide string that outlives the call, and
    // the default security attributes need no pointer.
    let reload = match unsafe { CreateEventW(None, false, false, name) } {
        Ok(handle) => Event(handle),
        Err(error) => return ready.send(Err(error.into())).map_err(io::Error::other),
    };
    ready.send(Ok(())).map_err(io::Error::other)?;
    let handles = [stop.0, reload.0];
    loop {
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: both handles are this thread's own and stay open until they drop
        // when it returns.
        let woken = unsafe { WaitForMultipleObjects(&handles, false, INFINITE) };
        match woken.0.checked_sub(WAIT_OBJECT_0.0) {
            Some(0) => return Ok(()),
            Some(1) => {
                if events.send(E::from(ReloadRequest)).is_err() {
                    return Ok(());
                }
            },
            _ => return Err(io::Error::last_os_error()),
        }
    }
}

pub(crate) fn request_reload(name: &str) -> io::Result<bool> {
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: the name is a null-terminated wide string that outlives the call.
    let opened = unsafe { OpenEventW(EVENT_MODIFY_STATE, false, &HSTRING::from(name)) };
    match opened {
        Ok(handle) => Event(handle).set().map(|()| true),
        Err(error) if error.code() == ERROR_FILE_NOT_FOUND.to_hresult() => Ok(false),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use std::process;
    use std::time::Duration;

    use super::*;

    #[test]
    fn a_reload_request_reaches_the_watch_and_stops_with_it() {
        let name = format!(r"Local\AppyTappy.Test.Reload.{}", process::id());
        assert!(!request_reload(&name).expect("ask"), "nobody waits yet");
        let (sender, requests) = mpsc::channel::<ReloadRequest>();
        let watch = ReloadWatch::start(&name, sender).expect("start");
        assert!(request_reload(&name).expect("ask"), "the watch waits");
        assert_eq!(
            requests.recv_timeout(Duration::from_secs(10)),
            Ok(ReloadRequest)
        );
        watch.stop().expect("stop");
        assert!(!request_reload(&name).expect("ask"), "the event is gone");
    }

    #[test]
    fn a_dropped_reload_watch_ends_its_thread_without_waiting_for_it() {
        let name = format!(r"Local\AppyTappy.Test.Dropped.{}", process::id());
        let (sender, requests) = mpsc::channel::<ReloadRequest>();
        let watch = ReloadWatch::start(&name, sender).expect("start");
        drop(watch);
        // The thread drops its sender when it ends.
        assert_eq!(
            requests.recv_timeout(Duration::from_secs(10)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        );
        assert!(!request_reload(&name).expect("ask"), "the event is gone");
    }
}
