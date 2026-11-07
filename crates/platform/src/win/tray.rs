//! The tray icon, its hidden window, and its right-click menu.
//!
//! The window procedure keeps no state: it turns the shell's notices into
//! messages posted to its own window, and the tray thread's message loop,
//! which owns the menu, handles them. While the menu is open, its own loop
//! dispatches those posts to the procedure, which drops them, so the tray loop
//! refreshes the icon each time the menu closes. Stopping the tray works even
//! then, because the procedure itself closes the menu and ends the loop.

use std::io;
use std::mem::size_of;
use std::num::NonZeroU16;
use std::ptr;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::thread::{self, JoinHandle};

use windows::Win32::Foundation::{ERROR_CLASS_ALREADY_EXISTS, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION,
    NOTIFY_ICON_MESSAGE, NOTIFYICON_VERSION_4, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, EndMenu, GetMessageW, HMENU, IDI_APPLICATION, LoadIconW, MENU_ITEM_FLAGS,
    MF_CHECKED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, MF_UNCHECKED, MSG, PostMessageW,
    PostQuitMessage, RegisterClassW, RegisterWindowMessageW, SetForegroundWindow, TPM_NONOTIFY,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, WINDOW_EX_STYLE, WM_APP, WM_CLOSE,
    WM_CONTEXTMENU, WM_DESTROY, WM_ENDSESSION, WM_NULL, WNDCLASSW, WS_OVERLAPPED,
};
use windows::core::{HSTRING, PCWSTR, w};

use crate::{MenuItem, TrayEvent, TrayMenu};

use super::{join, wait_for_start};

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

/// The tray window's class. The installer hooks in
/// `apps/setup/src-tauri/windows/hooks.nsh` find the window by this class and
/// its title to ask the runtime to close.
const TRAY_CLASS: PCWSTR = w!("AppyTappy.Tray");

/// The icon's number among the tray window's icons.
const ICON_ID: u32 = 1;

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
                // The installer hooks match this title too; see TRAY_CLASS.
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
        for (unit, slot) in crate::tray::tooltip_units(tooltip)
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
    let text = label.map(|label| HSTRING::from(crate::tray::menu_label(label)));
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use windows::Win32::UI::WindowsAndMessaging::{
        GetMenuItemCount, GetMenuItemID, GetMenuState, GetSubMenu, MF_BYPOSITION,
    };

    use super::*;

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
        // SAFETY: the menu belongs to `menu`, which outlives all the calls.
        let (lines, nested, action_id, action, note, inactive, separator) = unsafe {
            let submenu = GetSubMenu(menu.0, 1);
            (
                GetMenuItemCount(Some(menu.0)),
                GetMenuItemCount(Some(submenu)),
                GetMenuItemID(menu.0, 0),
                GetMenuState(menu.0, 0, MF_BYPOSITION),
                GetMenuState(submenu, 0, MF_BYPOSITION),
                GetMenuState(submenu, 1, MF_BYPOSITION),
                GetMenuState(menu.0, 2, MF_BYPOSITION),
            )
        };
        assert_eq!((lines, nested), (3, 2));
        assert_eq!(action_id, 1, "the action keeps its id");
        assert_ne!(action & MF_CHECKED.0, 0, "a checked action shows its check");
        assert_eq!(note & MF_GRAYED.0, 0, "a note is not grayed");
        assert_ne!(inactive & MF_GRAYED.0, 0, "an inactive line is grayed");
        assert_ne!(
            separator & MF_SEPARATOR.0,
            0,
            "a separator line is a separator"
        );
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
