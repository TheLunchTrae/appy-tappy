//! The window event hook, which reports windows created and destroyed on the
//! desktop.
//!
//! The hook is out of context: Windows delivers each event to the thread that
//! installed the hook, inside that thread's message loop, and never loads code
//! into another process. The callback carries no pointer back to the watch, so
//! it posts a thread message to its own thread, and the message loop forwards
//! the event over the channel.

use std::io;
use std::sync::mpsc::{self, Sender, SyncSender};
use std::thread::{self, JoinHandle};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    CHILDID_SELF, EVENT_OBJECT_CREATE, EVENT_OBJECT_DESTROY, GA_ROOT, GetAncestor, GetMessageW,
    MSG, OBJID_WINDOW, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, WINEVENT_OUTOFCONTEXT,
    WM_APP, WM_QUIT,
};

use crate::WindowEvent;

use super::{join, wait_for_start};

/// The thread message the callback posts for a created top-level window.
const CREATED: u32 = WM_APP;

/// The thread message the callback posts for a destroyed window.
const DESTROYED: u32 = WM_APP + 1;

/// The window event hook's thread.
#[derive(Debug)]
pub(crate) struct WindowWatch {
    thread_id: u32,
    thread: JoinHandle<io::Result<()>>,
}

impl WindowWatch {
    pub(crate) fn start<E>(events: Sender<E>) -> io::Result<Self>
    where
        E: From<WindowEvent> + Send + 'static,
    {
        let (ready, started) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("window events".to_owned())
            .spawn(move || watch_windows(&events, &ready))?;
        let (thread_id, thread) = wait_for_start(&started, thread, "window events")?;
        Ok(Self { thread_id, thread })
    }

    pub(crate) fn stop(self) -> io::Result<()> {
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the call takes a thread ID and two integers, and posting to a
        // thread that has already ended fails without effect.
        let posted = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
        // A failed post means the thread already ended; joining reports why.
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a failed post means the loop already ended, and the join below reports its result"
        )]
        let _ = posted;
        join(self.thread, "window events")
    }
}

/// An installed hook, removed on drop on the thread that installed it.
struct Hook(HWINEVENTHOOK);

impl Drop for Hook {
    fn drop(&mut self) {
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the hook came from a successful SetWinEventHook and is removed once.
        let removed = unsafe { UnhookWinEvent(self.0) };
        #[expect(
            clippy::let_underscore_must_use,
            reason = "removing a hook this value owns cannot be retried, and Drop has no caller to report to"
        )]
        let _ = removed;
    }
}

/// The window events thread: installs the hook, reports the start, and
/// forwards events until `WM_QUIT` arrives or nobody receives them.
fn watch_windows<E: From<WindowEvent>>(
    events: &Sender<E>,
    ready: &SyncSender<io::Result<u32>>,
) -> io::Result<()> {
    let mut message = MSG::default();
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: `message` is a writable MSG. Peeking makes sure this thread has a
    // message queue before its ID is published for `stop` to post to.
    let peeked = unsafe { PeekMessageW(&raw mut message, None, 0, 0, PM_NOREMOVE) };
    #[expect(
        clippy::let_underscore_must_use,
        reason = "the peek runs only to create the queue; whether a message waits does not matter"
    )]
    let _ = peeked;
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: the callback is a plain function that lives for the whole program,
    // and the out-of-context flag runs it on this thread only.
    let hook = unsafe {
        SetWinEventHook(
            EVENT_OBJECT_CREATE,
            EVENT_OBJECT_DESTROY,
            None,
            Some(on_window_event),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        )
    };
    if hook.is_invalid() {
        let error = io::Error::other("SetWinEventHook could not install the window event hook");
        return ready.send(Err(error)).map_err(io::Error::other);
    }
    let _hook = Hook(hook);
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: the call takes no arguments.
    let thread_id = unsafe { GetCurrentThreadId() };
    ready.send(Ok(thread_id)).map_err(io::Error::other)?;
    loop {
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `message` is a writable MSG, and the hook's callbacks run inside
        // this call, on this thread.
        let got = unsafe { GetMessageW(&raw mut message, None, 0, 0) };
        let event = match got.0 {
            0 => return Ok(()),
            -1 => return Err(io::Error::last_os_error()),
            _ => match message.message {
                CREATED => WindowEvent::Created,
                DESTROYED => WindowEvent::Destroyed,
                _ => continue,
            },
        };
        if events.send(E::from(event)).is_err() {
            return Ok(());
        }
    }
}

/// The hook's callback. It keeps only events about a window itself, keeps a
/// created window only when it is top-level, and posts the event to this
/// thread's message loop. It never panics, because it runs inside a Windows
/// call.
extern "system" fn on_window_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    window: HWND,
    object: i32,
    child: i32,
    _event_thread: u32,
    _time: u32,
) {
    if object != OBJID_WINDOW.0 || u32::try_from(child) != Ok(CHILDID_SELF) {
        return;
    }
    let message = match event {
        EVENT_OBJECT_CREATE => {
            #[expect(unsafe_code, reason = "a Win32 call")]
            // SAFETY: the call reads a window handle, and a handle that is no longer
            // valid returns a null root.
            let root = unsafe { GetAncestor(window, GA_ROOT) };
            if root != window {
                return;
            }
            CREATED
        },
        EVENT_OBJECT_DESTROY => DESTROYED,
        _ => return,
    };
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: both calls take plain integers; the callback runs on the hook's thread,
    // whose message queue exists.
    let posted = unsafe { PostThreadMessageW(GetCurrentThreadId(), message, WPARAM(0), LPARAM(0)) };
    #[expect(
        clippy::let_underscore_must_use,
        reason = "a full queue drops one event; the next event or the lifecycle's rescan covers it"
    )]
    let _ = posted;
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_OVERLAPPED,
    };
    use windows::core::w;

    use super::*;

    fn next_event(events: &mpsc::Receiver<WindowEvent>, wanted: WindowEvent) {
        let deadline = Instant::now()
            .checked_add(Duration::from_secs(10))
            .expect("a near deadline");
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match events.recv_timeout(left) {
                Ok(event) if event == wanted => return,
                Ok(_) => {},
                Err(error) => panic!("no {wanted:?} event: {error}"),
            }
        }
    }

    #[test]
    fn the_watch_reports_a_top_level_window_created_and_destroyed() {
        let (sender, events) = mpsc::channel();
        let watch = WindowWatch::start(sender).expect("start");
        // The callback checks a created window's root after the fact, so the
        // window stays until its creation is seen.
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the class is a system class, the title is a static wide
        // string, and the window has no parent, menu, or creation data.
        let window = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                w!("AppyTappy test"),
                WS_OVERLAPPED,
                0,
                0,
                10,
                10,
                None,
                None,
                None,
                None,
            )
        }
        .expect("create a window");
        next_event(&events, WindowEvent::Created);
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: this thread created the window and destroys it once.
        let destroyed = unsafe { DestroyWindow(window) };
        destroyed.expect("destroy the window");
        next_event(&events, WindowEvent::Destroyed);
        watch.stop().expect("stop");
    }
}
