//! The Windows adapter: each function turns Windows API calls into a typed
//! value or an error and makes no decisions of its own.
//!
//! The window event hook is out of context: Windows delivers each event to
//! the thread that installed the hook, inside that thread's message loop, and
//! never loads code into another process. The callback carries no pointer
//! back to the watch, so it posts a thread message to its own thread, and
//! the message loop forwards the event over the channel.
//!
//! The tray works the same way. Its window procedure keeps no state: it
//! turns the shell's notices into messages posted to its own window, and the
//! tray thread's message loop, which owns the menu, handles them. While the
//! menu is open, its own loop dispatches those posts to the procedure, which
//! drops them, so the tray loop refreshes the icon each time the menu
//! closes. Stopping the tray works even then, because the procedure itself
//! closes the menu and ends the loop.

use std::ffi::OsStr;
use std::io;
use std::iter;
use std::mem::{size_of, size_of_val};
use std::num::NonZeroU16;
use std::os::windows::ffi::OsStrExt;
use std::ptr;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::thread::{self, JoinHandle};

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, ERROR_CLASS_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND,
    GetLastError, HANDLE, HWND, LPARAM, LRESULT, WPARAM,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW,
};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::System::Threading::{CreateMutexW, GetCurrentThreadId};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION,
    NOTIFY_ICON_MESSAGE, NOTIFYICON_VERSION_4, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CHILDID_SELF, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
    DestroyWindow, DispatchMessageW, EVENT_OBJECT_CREATE, EVENT_OBJECT_DESTROY, EndMenu, GA_ROOT,
    GetAncestor, GetMessageW, HMENU, IDI_APPLICATION, LoadIconW, MENU_ITEM_FLAGS, MF_CHECKED,
    MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, MF_UNCHECKED, MSG, OBJID_WINDOW, PM_NOREMOVE,
    PeekMessageW, PostMessageW, PostQuitMessage, PostThreadMessageW, RegisterClassW,
    RegisterWindowMessageW, SetForegroundWindow, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    TrackPopupMenu, WINDOW_EX_STYLE, WINEVENT_OUTOFCONTEXT, WM_APP, WM_CLOSE, WM_CONTEXTMENU,
    WM_DESTROY, WM_ENDSESSION, WM_NULL, WM_QUIT, WNDCLASSW, WS_OVERLAPPED,
};
use windows::core::{HSTRING, PCWSTR, w};

use crate::{LocalTime, MenuItem, TrayEvent, TrayMenu, WindowEvent};

/// The thread message the callback posts for a created top-level window.
const CREATED: u32 = WM_APP;

/// The thread message the callback posts for a destroyed window.
const DESTROYED: u32 = WM_APP + 1;

/// The message the shell sends the tray window about its icon.
const TRAY_CALLBACK: u32 = WM_APP + 2;

/// Posted by the tray window to its own queue when the user right-clicks the
/// icon, so the loop opens the menu. It carries the shell's anchor point.
const OPEN_MENU: u32 = WM_APP + 3;

/// Posted by the tray window when the taskbar restarts and has forgotten
/// the icon.
const ADD_ICON_AGAIN: u32 = WM_APP + 4;

/// Posted by [`Tray::show`] to wake the loop for a new menu.
const MENU_CHANGED: u32 = WM_APP + 5;

/// Posted by [`Tray::stop`] to end the loop.
const STOP_TRAY: u32 = WM_APP + 6;

/// Posted by the tray window when Windows asks the program to close.
const CLOSE_REQUESTED: u32 = WM_APP + 7;

/// The exit code of the tray loop's `WM_QUIT` when [`Tray::stop`] ended it.
/// Any other `WM_QUIT` means the window went away.
const STOPPED: i32 = 1;

/// The tray window's class.
const TRAY_CLASS: PCWSTR = w!("AppyTappy.Tray");

/// The icon's number among the tray window's icons.
const ICON_ID: u32 = 1;

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

/// The tray icon's thread and hidden window.
#[derive(Debug)]
pub(crate) struct Tray {
    /// The window's handle as an address, because a handle may not cross
    /// threads but posting to it from any thread is safe.
    window: usize,
    menus: Sender<TrayMenu>,
    thread: JoinHandle<io::Result<()>>,
}

impl Tray {
    pub(crate) fn start<E>(events: Sender<E>, menu: TrayMenu) -> io::Result<Self>
    where
        E: From<TrayEvent> + Send + 'static,
    {
        let (ready, started) = mpsc::sync_channel(1);
        let (menus, updates) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("tray".to_owned())
            .spawn(move || {
                let result = run_tray(&events, menu, &updates, &ready);
                let reason = match &result {
                    Ok(TrayExit::Stopped | TrayExit::NotStarted | TrayExit::OwnerGone) => None,
                    Ok(TrayExit::WindowGone) => Some("its window was destroyed".to_owned()),
                    Err(error) => Some(error.to_string()),
                };
                if let Some(reason) = reason {
                    #[expect(
                        clippy::let_underscore_must_use,
                        reason = "a failed send means the owner already stopped, so nobody needs to know"
                    )]
                    let _ = events.send(E::from(TrayEvent::Ended(reason)));
                }
                result.map(drop)
            })?;
        let (window, thread) = wait_for_start(&started, thread, "tray")?;
        Ok(Self {
            window,
            menus,
            thread,
        })
    }

    pub(crate) fn show(&self, menu: TrayMenu) -> io::Result<()> {
        self.menus
            .send(menu)
            .map_err(|_closed| io::Error::other("the tray thread has ended"))?;
        post(self.window, MENU_CHANGED)
    }

    pub(crate) fn stop(self) -> io::Result<()> {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a failed post means the loop already ended, and the join below reports its result"
        )]
        let _ = post(self.window, STOP_TRAY);
        join(self.thread, "tray")
    }
}

/// Posts `message` with no arguments to the window at `address`.
fn post(address: usize, message: u32) -> io::Result<()> {
    let window = HWND(ptr::without_provenance_mut(address));
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: the call takes a window handle and integers; posting to a window
    // that no longer exists fails without effect.
    let posted = unsafe { PostMessageW(Some(window), message, WPARAM(0), LPARAM(0)) };
    posted.map_err(io::Error::from)
}

/// Why the tray loop ended without an error.
enum TrayExit {
    /// [`Tray::stop`] ended it.
    Stopped,
    /// The window or the icon could not be made; [`Tray::start`] reports why.
    NotStarted,
    /// Nobody receives the tray's events any more.
    OwnerGone,
    /// The window was destroyed by someone else.
    WindowGone,
}

/// The tray thread: creates the window and the icon, reports the start, and
/// runs the message loop until [`Tray::stop`], the window goes, or nobody
/// receives events.
fn run_tray<E: From<TrayEvent>>(
    events: &Sender<E>,
    mut menu: TrayMenu,
    updates: &Receiver<TrayMenu>,
    ready: &SyncSender<io::Result<usize>>,
) -> io::Result<TrayExit> {
    // The icon is declared after the window, so it is removed before the
    // window is destroyed.
    let window = match TrayWindow::create() {
        Ok(window) => window,
        Err(error) => {
            ready.send(Err(error)).map_err(io::Error::other)?;
            return Ok(TrayExit::NotStarted);
        },
    };
    let mut icon = match TrayIcon::new(window.0) {
        Ok(icon) => icon,
        Err(error) => {
            ready.send(Err(error)).map_err(io::Error::other)?;
            return Ok(TrayExit::NotStarted);
        },
    };
    // Before Explorer runs, as at sign-in, the add fails; the icon is added
    // when the taskbar starts.
    icon.show(&menu.tooltip);
    ready
        .send(Ok(window.0.0.addr()))
        .map_err(io::Error::other)?;
    let mut message = MSG::default();
    loop {
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `message` is a writable MSG, and the window's procedure runs
        // inside this call, on this thread.
        let got = unsafe { GetMessageW(&raw mut message, None, 0, 0) };
        match got.0 {
            0 if usize::try_from(STOPPED) == Ok(message.wParam.0) => return Ok(TrayExit::Stopped),
            0 => return Ok(TrayExit::WindowGone),
            -1 => return Err(io::Error::last_os_error()),
            _ => {},
        }
        let event = match message.message {
            OPEN_MENU => {
                // A menu that cannot be built stays closed; the next
                // right-click tries again.
                let picked = pick(window.0, &menu.items, message.wParam);
                // A taskbar that restarted while the menu was open has
                // forgotten the icon, and that notice was dropped.
                icon.show(&menu.tooltip);
                picked.ok().flatten().map(TrayEvent::Picked)
            },
            CLOSE_REQUESTED => Some(TrayEvent::CloseRequested),
            ADD_ICON_AGAIN => {
                icon.add();
                None
            },
            MENU_CHANGED => None,
            _ => {
                #[expect(unsafe_code, reason = "a Win32 call")]
                // SAFETY: `message` came from GetMessageW on this thread.
                unsafe {
                    DispatchMessageW(&raw const message);
                }
                None
            },
        };
        if let Some(event) = event
            && events.send(E::from(event)).is_err()
        {
            return Ok(TrayExit::OwnerGone);
        }
        let mut changed = false;
        while let Ok(next) = updates.try_recv() {
            menu = next;
            changed = true;
        }
        if changed {
            icon.show(&menu.tooltip);
        }
    }
}

/// The hidden window that receives the shell's notices about the icon. It
/// is top-level, not message-only, because only top-level windows hear that
/// the taskbar restarted.
struct TrayWindow(HWND);

impl TrayWindow {
    fn create() -> io::Result<Self> {
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: no module name asks for the running program's own module.
        let module = unsafe { GetModuleHandleW(None) }?;
        let class = WNDCLASSW {
            lpfnWndProc: Some(on_tray_message),
            hInstance: module.into(),
            lpszClassName: TRAY_CLASS,
            ..WNDCLASSW::default()
        };
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `class` is a filled WNDCLASSW whose class name is a static wide
        // string and whose procedure lives for the whole program.
        let atom = unsafe { RegisterClassW(&raw const class) };
        if atom == 0 {
            // Another tray in this program already registered the class.
            let error = io::Error::last_os_error();
            if error.raw_os_error() != i32::try_from(ERROR_CLASS_ALREADY_EXISTS.0).ok() {
                return Err(error);
            }
        }
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the class was registered above, the title is a static wide
        // string, and the window has no parent, menu, or creation data.
        let window = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                TRAY_CLASS,
                w!("AppyTappy"),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(module.into()),
                None,
            )
        }?;
        Ok(Self(window))
    }
}

impl Drop for TrayWindow {
    fn drop(&mut self) {
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: this thread created the window and destroys it once.
        let destroyed = unsafe { DestroyWindow(self.0) };
        #[expect(
            clippy::let_underscore_must_use,
            reason = "destroying a window this value owns cannot be retried, and Drop has no caller to report to"
        )]
        let _ = destroyed;
    }
}

/// The icon on the taskbar, removed on drop.
struct TrayIcon {
    data: NOTIFYICONDATAW,
    added: bool,
}

impl TrayIcon {
    fn new(window: HWND) -> io::Result<Self> {
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: no module and a predefined icon name load a shared system icon,
        // which is never destroyed.
        let icon = unsafe { LoadIconW(None, IDI_APPLICATION) }?;
        let mut data = NOTIFYICONDATAW {
            cbSize: u32::try_from(size_of::<NOTIFYICONDATAW>()).map_err(io::Error::other)?,
            hWnd: window,
            uID: ICON_ID,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
            uCallbackMessage: TRAY_CALLBACK,
            hIcon: icon,
            ..NOTIFYICONDATAW::default()
        };
        // Version 4 reports a right-click as one WM_CONTEXTMENU.
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        Ok(Self { data, added: false })
    }

    /// Shows `tooltip` on the icon, adding the icon when it is missing.
    fn show(&mut self, tooltip: &str) {
        self.data.szTip = [0; 128];
        for (unit, slot) in crate::tooltip_units(tooltip)
            .into_iter()
            .zip(self.data.szTip.iter_mut())
        {
            *slot = unit;
        }
        if !(self.added && notify(NIM_MODIFY, &self.data)) {
            self.add();
        }
    }

    /// Adds the icon to the taskbar. An add fails when the icon is already
    /// there, and a busy Explorer can report a failed add that worked, so a
    /// working modify counts too.
    fn add(&mut self) {
        self.added = notify(NIM_ADD, &self.data) || notify(NIM_MODIFY, &self.data);
        if self.added {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "without version 4 the shell sends older notices, which open no menu; there is nothing better to do"
            )]
            let _ = notify(NIM_SETVERSION, &self.data);
        }
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        // The delete runs even when no add was seen to work, because the
        // icon may be there anyway; deleting a missing icon does nothing.
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the shell removes a dead window's icon itself, and Drop has no caller to report to"
        )]
        let _ = notify(NIM_DELETE, &self.data);
    }
}

/// Sends `action` about the icon to the shell and returns whether it worked.
#[must_use]
fn notify(action: NOTIFY_ICON_MESSAGE, data: &NOTIFYICONDATAW) -> bool {
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: `data` is a filled NOTIFYICONDATAW whose size field matches the
    // struct, and the call copies it.
    let done = unsafe { Shell_NotifyIconW(action, data) };
    done.as_bool()
}

/// The tray window's procedure. It keeps no state: a right-click on the
/// icon, a restarted taskbar, and a request to close become messages posted
/// to the window, which the tray thread's loop handles. It never panics,
/// because it runs inside a Windows call.
extern "system" fn on_tray_message(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let next = match message {
        // The low word of `lparam` names the shell's notice, and `wparam`
        // holds the point to open the menu at.
        TRAY_CALLBACK if u32::try_from(lparam.0 & 0xFFFF) == Ok(WM_CONTEXTMENU) => {
            Some((OPEN_MENU, wparam))
        },
        // A close request must not destroy the window, or the icon would stay
        // with no menu; the owner decides what it means. A non-zero `wparam`
        // on WM_ENDSESSION means the session really ends.
        WM_CLOSE => Some((CLOSE_REQUESTED, WPARAM(0))),
        WM_ENDSESSION if wparam.0 != 0 => Some((CLOSE_REQUESTED, WPARAM(0))),
        STOP_TRAY => {
            end_loop(STOPPED);
            return LRESULT(0);
        },
        WM_DESTROY => {
            end_loop(0);
            return LRESULT(0);
        },
        // Registered messages start at 0xC000.
        _ if message >= 0xC000 && message == taskbar_created() => Some((ADD_ICON_AGAIN, WPARAM(0))),
        _ => None,
    };
    if let Some((next, argument)) = next {
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the call takes this window's handle and integers.
        let posted = unsafe { PostMessageW(Some(window), next, argument, LPARAM(0)) };
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a full queue drops one notice, and the user or Windows repeats it"
        )]
        let _ = posted;
        return LRESULT(0);
    }
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: the arguments are the ones Windows passed to this procedure.
    unsafe {
        DefWindowProcW(window, message, wparam, lparam)
    }
}

/// Closes an open menu, then ends the tray loop with `code`.
fn end_loop(code: i32) {
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: the call takes no arguments and closes this thread's open menu.
    let ended = unsafe { EndMenu() };
    #[expect(
        clippy::let_underscore_must_use,
        reason = "it fails when no menu is open, which is the usual case"
    )]
    let _ = ended;
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: the call takes an integer and ends this thread's loop.
    unsafe {
        PostQuitMessage(code);
    }
}

/// The message Explorer broadcasts when the taskbar starts.
fn taskbar_created() -> u32 {
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: the name is a static wide string; registering it again returns
    // the same number.
    unsafe {
        RegisterWindowMessageW(w!("TaskbarCreated"))
    }
}

/// Opens the menu at the shell's `anchor` point and returns the id of the
/// action the user picked, if any. The anchor is the pointer for a click and
/// the icon for the keyboard.
fn pick(window: HWND, items: &[MenuItem], anchor: WPARAM) -> io::Result<Option<NonZeroU16>> {
    let menu = PopupMenu::new(items)?;
    let x = signed_word(anchor.0);
    let y = signed_word(anchor.0 >> 16);
    // The menu closes when the user clicks elsewhere only while its window
    // is in the foreground.
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: the call takes this thread's window handle.
    let foreground = unsafe { SetForegroundWindow(window) };
    #[expect(
        clippy::let_underscore_must_use,
        reason = "a menu that cannot take the foreground still opens; it only closes less eagerly"
    )]
    let _ = foreground;
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: the menu and the window belong to this thread and outlive the
    // call, which returns once the menu closes.
    let chosen = unsafe {
        TrackPopupMenu(
            menu.0,
            TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
            x,
            y,
            None,
            window,
            None,
        )
    };
    // Microsoft documents this empty post after a tray menu, so the next
    // click is not swallowed.
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: the call takes this thread's window handle and integers.
    let posted = unsafe { PostMessageW(Some(window), WM_NULL, WPARAM(0), LPARAM(0)) };
    #[expect(
        clippy::let_underscore_must_use,
        reason = "the empty message only helps the next click; losing it changes nothing else"
    )]
    let _ = posted;
    Ok(u16::try_from(chosen.0).ok().and_then(NonZeroU16::new))
}

/// The low 16 bits of `value` as a signed coordinate, as a screen point to
/// the left of or above the main display is negative.
fn signed_word(value: usize) -> i32 {
    let word = u16::try_from(value & 0xFFFF).unwrap_or(0);
    i32::from(i16::from_ne_bytes(word.to_ne_bytes()))
}

/// A menu built from [`MenuItem`]s, destroyed on drop with its submenus.
struct PopupMenu(HMENU);

impl PopupMenu {
    fn new(items: &[MenuItem]) -> io::Result<Self> {
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the call takes no arguments.
        let menu = Self(unsafe { CreatePopupMenu() }?);
        fill(menu.0, items)?;
        Ok(menu)
    }
}

/// Appends `items` to `menu`, which a [`PopupMenu`] owns, directly or as an
/// attached submenu, so it is destroyed even when this fails halfway.
fn fill(menu: HMENU, items: &[MenuItem]) -> io::Result<()> {
    for item in items {
        match item {
            MenuItem::Action { id, label, checked } => {
                let check = if *checked { MF_CHECKED } else { MF_UNCHECKED };
                append(menu, MF_STRING | check, usize::from(id.get()), Some(label))?;
            },
            MenuItem::Note(label) => append(menu, MF_STRING, 0, Some(label))?,
            MenuItem::Inactive(label) => append(menu, MF_STRING | MF_GRAYED, 0, Some(label))?,
            MenuItem::Submenu { label, items } => {
                #[expect(unsafe_code, reason = "a Win32 call")]
                // SAFETY: the call takes no arguments.
                let submenu = unsafe { CreatePopupMenu() }?;
                // Once attached, the submenu belongs to `menu` and is destroyed
                // with it; until then it is destroyed here.
                if let Err(error) = append(menu, MF_POPUP, submenu.0.addr(), Some(label)) {
                    #[expect(unsafe_code, reason = "a Win32 call")]
                    // SAFETY: the submenu came from CreatePopupMenu, is attached to
                    // nothing, and is destroyed once.
                    let destroyed = unsafe { DestroyMenu(submenu) };
                    #[expect(
                        clippy::let_underscore_must_use,
                        reason = "the append error is the one to report"
                    )]
                    let _ = destroyed;
                    return Err(error);
                }
                fill(submenu, items)?;
            },
            MenuItem::Separator => append(menu, MF_SEPARATOR, 0, None)?,
        }
    }
    Ok(())
}

/// Appends one line to `menu`.
fn append(menu: HMENU, flags: MENU_ITEM_FLAGS, id: usize, label: Option<&str>) -> io::Result<()> {
    let text = label.map(|label| HSTRING::from(crate::menu_label(label)));
    let text = text
        .as_ref()
        .map_or_else(PCWSTR::null, |text| PCWSTR(text.as_ptr()));
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: `menu` is a live menu, and `text` is null or a null-terminated wide
    // string that outlives the call, which copies it.
    let appended = unsafe { AppendMenuW(menu, flags, id, text) };
    appended.map_err(io::Error::from)
}

impl Drop for PopupMenu {
    fn drop(&mut self) {
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the menu came from CreatePopupMenu, is not attached to another
        // menu, and is destroyed once.
        let destroyed = unsafe { DestroyMenu(self.0) };
        #[expect(
            clippy::let_underscore_must_use,
            reason = "destroying a menu this value owns cannot be retried, and Drop has no caller to report to"
        )]
        let _ = destroyed;
    }
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
    use windows::Win32::UI::WindowsAndMessaging::{GetMenuItemCount, GetSubMenu};

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

    fn sample_menu() -> TrayMenu {
        TrayMenu {
            tooltip: "AppyTappy: idle".to_owned(),
            items: vec![
                MenuItem::Action {
                    id: NonZeroU16::MIN,
                    label: "Enabled".to_owned(),
                    checked: true,
                },
                MenuItem::Submenu {
                    label: "Active automations".to_owned(),
                    items: vec![
                        MenuItem::Note("Pause music".to_owned()),
                        MenuItem::Inactive("None".to_owned()),
                    ],
                },
                MenuItem::Separator,
            ],
        }
    }

    #[test]
    fn the_menu_has_a_line_for_each_item() {
        let menu = PopupMenu::new(&sample_menu().items).expect("build the menu");
        #[expect(unsafe_code, reason = "Win32 calls")]
        // SAFETY: the menu belongs to `menu`, which outlives both calls.
        let (lines, nested) = unsafe {
            let submenu = GetSubMenu(menu.0, 1);
            (
                GetMenuItemCount(Some(menu.0)),
                GetMenuItemCount(Some(submenu)),
            )
        };
        assert_eq!((lines, nested), (3, 2));
    }

    #[test]
    fn the_tray_starts_takes_a_new_menu_and_stops() {
        let (events, _received) = mpsc::channel::<TrayEvent>();
        let tray = Tray::start(events, sample_menu()).expect("start");
        tray.show(sample_menu()).expect("show");
        tray.stop().expect("stop");
    }

    #[test]
    fn a_close_request_reaches_the_owner_and_keeps_the_tray() {
        let (events, received) = mpsc::channel::<TrayEvent>();
        let tray = Tray::start(events, sample_menu()).expect("start");
        let window = HWND(ptr::without_provenance_mut(tray.window));
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the call takes the tray's window handle and integers.
        let posted = unsafe { PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0)) };
        posted.expect("post");
        let event = received
            .recv_timeout(Duration::from_secs(5))
            .expect("an event");
        assert_eq!(event, TrayEvent::CloseRequested);
        tray.show(sample_menu()).expect("the tray still runs");
        tray.stop().expect("stop");
        assert!(received.try_recv().is_err(), "a stop reports no end");
    }

    #[test]
    fn anchor_words_are_signed() {
        assert_eq!(signed_word(0x0001_0010), 16);
        assert_eq!(signed_word(0xFFFF), -1);
        assert_eq!(signed_word(0x8000), -32768);
    }
}
