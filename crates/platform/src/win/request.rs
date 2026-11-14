//! The setup app's requests: a named event for each kind, which the setup
//! app sets and the runtime's request thread waits on.

use std::io;
use std::iter;
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

use crate::Request;

use super::{join, wait_for_start};

/// Every request, in the order the watch sends those that wait together: a
/// reload first, so what the runtime then runs once is what was saved.
const REQUESTS: [Request; 2] = [Request::Reload, Request::RunOnce];

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

/// The thread that waits for requests.
#[derive(Debug)]
pub(crate) struct RequestWatch {
    thread: Option<JoinHandle<io::Result<()>>>,
    /// Ends the thread's wait. The thread waits on its own handle to the
    /// same event, so this one can close without waiting for the thread.
    stop: Event,
}

impl RequestWatch {
    pub(crate) fn start<E>(scope: &str, events: Sender<E>) -> io::Result<Self>
    where
        E: From<Request> + Send + 'static,
    {
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the default security attributes and no name need no pointers.
        let stop = Event(unsafe { CreateEventW(None, true, false, None) }?);
        let stop_address = stop.address();
        let names = REQUESTS.map(|request| HSTRING::from(request.event(scope)));
        let (ready, started) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("requests".to_owned())
            .spawn(move || watch_requests(&names, stop_address, &events, &ready))?;
        let ((), thread) = wait_for_start(&started, thread, "requests")?;
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
        let joined = join(thread, "requests");
        set.and(joined)
    }
}

impl Drop for RequestWatch {
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

/// The request thread: opens the named event of each request in `names`,
/// which are in the order of [`REQUESTS`], reports the start, and sends
/// a request each time its event is set, until the stop event is set or
/// nobody receives requests.
fn watch_requests<E: From<Request>>(
    names: &[HSTRING],
    stop: usize,
    events: &Sender<E>,
    ready: &SyncSender<io::Result<()>>,
) -> io::Result<()> {
    // The caller keeps its handle open until this thread reports its start.
    let stop = match Event::duplicate(stop) {
        Ok(stop) => stop,
        Err(error) => return ready.send(Err(error)).map_err(io::Error::other),
    };
    let mut asked = Vec::with_capacity(names.len());
    for name in names {
        // An auto-reset event: each wait that returns it clears it, so a
        // burst of requests while one is handled becomes one more request.
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `name` is a null-terminated wide string that outlives the call,
        // and the default security attributes need no pointer.
        let opened = unsafe { CreateEventW(None, false, false, name) };
        match opened {
            Ok(handle) => asked.push(Event(handle)),
            Err(error) => return ready.send(Err(error.into())).map_err(io::Error::other),
        }
    }
    ready.send(Ok(())).map_err(io::Error::other)?;
    // The wait returns the first handle that is set, so of the requests
    // that wait together the first in `REQUESTS` is sent first.
    let handles: Vec<HANDLE> = iter::once(stop.0)
        .chain(asked.iter().map(|event| event.0))
        .collect();
    loop {
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: every handle is this thread's own and stays open until it drops
        // when the thread returns.
        let woken = unsafe { WaitForMultipleObjects(&handles, false, INFINITE) };
        let set = woken
            .0
            .checked_sub(WAIT_OBJECT_0.0)
            .and_then(|position| usize::try_from(position).ok());
        let request = match set {
            Some(0) => return Ok(()),
            Some(position) => position
                .checked_sub(1)
                .and_then(|position| REQUESTS.get(position)),
            None => None,
        };
        let Some(request) = request else {
            return Err(io::Error::last_os_error());
        };
        if events.send(E::from(*request)).is_err() {
            return Ok(());
        }
    }
}

/// Sets the event named `name`, and answers whether it exists.
pub(crate) fn request(name: &str) -> io::Result<bool> {
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

    /// A scope no other test or program uses.
    fn scope(name: &str) -> String {
        format!(r"Local\AppyTappy.Test.{name}.{}", process::id())
    }

    /// Sends `request` to `scope`, and answers whether a watch waited.
    fn ask(scope: &str, request: Request) -> bool {
        crate::request(scope, request).expect("ask")
    }

    #[test]
    fn each_request_reaches_the_watch_and_stops_with_it() {
        let scope = scope("Requests");
        assert!(!ask(&scope, Request::Reload), "nobody waits yet");
        let (sender, requests) = mpsc::channel::<Request>();
        let watch = RequestWatch::start(&scope, sender).expect("start");
        for request in [Request::RunOnce, Request::Reload] {
            assert!(ask(&scope, request), "the watch waits");
            assert_eq!(requests.recv_timeout(Duration::from_secs(10)), Ok(request));
        }
        watch.stop().expect("stop");
        for request in REQUESTS {
            assert!(!ask(&scope, request), "the event is gone");
        }
    }

    #[test]
    fn of_requests_that_wait_together_the_reload_is_sent_first() {
        let scope = scope("Together");
        // The events exist and are set before the watch opens them, as when
        // both requests come while the watch sends another.
        let held = REQUESTS.map(|request| {
            let name = HSTRING::from(request.event(&scope));
            #[expect(unsafe_code, reason = "a Win32 call")]
            // SAFETY: `name` is a null-terminated wide string that outlives the
            // call, and the default security attributes need no pointer.
            let event = unsafe { CreateEventW(None, false, false, &name) };
            Event(event.expect("create"))
        });
        assert!(ask(&scope, Request::RunOnce));
        assert!(ask(&scope, Request::Reload));
        let (sender, requests) = mpsc::channel::<Request>();
        let watch = RequestWatch::start(&scope, sender).expect("start");
        let sent = [(); 2].map(|()| requests.recv_timeout(Duration::from_secs(10)));
        assert_eq!(sent, [Ok(Request::Reload), Ok(Request::RunOnce)]);
        watch.stop().expect("stop");
        drop(held);
    }

    #[test]
    fn a_dropped_watch_ends_its_thread_without_waiting_for_it() {
        let scope = scope("Dropped");
        let (sender, requests) = mpsc::channel::<Request>();
        let watch = RequestWatch::start(&scope, sender).expect("start");
        drop(watch);
        // The thread drops its sender when it ends.
        assert_eq!(
            requests.recv_timeout(Duration::from_secs(10)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        );
        assert!(!ask(&scope, Request::Reload), "the event is gone");
    }
}
