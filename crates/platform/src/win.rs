//! The Windows adapter: each function turns Windows API calls into a typed
//! value or an error and makes no decisions of its own.
//!
//! The window event hook is out of context: Windows delivers each event to
//! the thread that installed the hook, inside that thread's message loop, and
//! never loads code into another process. The callback carries no pointer
//! back to the watch, so it posts a thread message to its own thread, and
//! the message loop forwards the event over the channel.

use std::ffi::OsStr;
use std::io;
use std::iter;
use std::mem::size_of_val;
use std::os::windows::ffi::OsStrExt;
use std::sync::mpsc::{self, Sender, SyncSender};
use std::thread::{self, JoinHandle};

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, GetLastError, HANDLE, HWND, LPARAM,
    WPARAM,
};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW,
};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::System::Threading::{CreateMutexW, GetCurrentThreadId};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    CHILDID_SELF, EVENT_OBJECT_CREATE, EVENT_OBJECT_DESTROY, GA_ROOT, GetAncestor, GetMessageW,
    MSG, OBJID_WINDOW, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, WINEVENT_OUTOFCONTEXT,
    WM_APP, WM_QUIT,
};
use windows::core::HSTRING;

use crate::{LocalTime, WindowEvent};

/// The thread message the callback posts for a created top-level window.
const CREATED: u32 = WM_APP;

/// The thread message the callback posts for a destroyed window.
const DESTROYED: u32 = WM_APP + 1;

/// The user's `Run` key, relative to `HKEY_CURRENT_USER`.
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// The window event hook's thread.
#[derive(Debug)]
pub(crate) struct WindowWatch {
    thread_id: u32,
    thread: JoinHandle<io::Result<()>>,
}

impl WindowWatch {
    pub(crate) fn start(events: Sender<WindowEvent>) -> io::Result<Self> {
        let (ready, started) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("window events".to_owned())
            .spawn(move || watch_windows(&events, &ready))?;
        match started.recv() {
            Ok(Ok(thread_id)) => Ok(Self { thread_id, thread }),
            Ok(Err(error)) => {
                // The thread reported the failure and returns right after.
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the thread's own result is Ok once it reported the start error, which is returned instead"
                )]
                let _ = thread.join();
                Err(error)
            },
            Err(_) => Err(thread.join().map_or_else(
                |_| io::Error::other("the window events thread panicked"),
                |result| {
                    result.err().unwrap_or_else(|| {
                        io::Error::other("the window events thread ended before it started")
                    })
                },
            )),
        }
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
        self.thread.join().unwrap_or_else(|payload| {
            Err(io::Error::other(format!(
                "the window events thread panicked: {}",
                crate::panic_reason(payload.as_ref())
            )))
        })
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
fn watch_windows(
    events: &Sender<WindowEvent>,
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
        if events.send(event).is_err() {
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

#[cfg(test)]
mod tests {
    use std::process;
    use std::time::{Duration, Instant};

    use windows::Win32::System::Registry::{RRF_RT_REG_SZ, RegGetValueW};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_OVERLAPPED,
    };
    use windows::core::w;

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
        let command = r#""C:\Program Files\AppyTappy\appytappy-runtime.exe" --watch "Game.exe""#;
        enable_autostart(&value, OsStr::new(command)).expect("enable");
        let expected: Vec<u16> = command.encode_utf16().collect();
        assert_eq!(run_value(&value), Some(expected));
        disable_autostart(&value).expect("disable");
        assert_eq!(run_value(&value), None);
        disable_autostart(&value).expect("an absent value is not an error");
    }

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
