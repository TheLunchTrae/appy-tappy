//! The Windows adapter: each function turns Windows API calls into a typed
//! value or an error and makes no decisions of its own.
//!
//! Media sessions come from the Windows Runtime media transport controls,
//! which music players and browsers publish without any app-specific API.
//! Volumes come from Core Audio on the default playback device. Naming an
//! audio session's app takes the process lister the caller hands the
//! controls, which must not open a process, so no action opens a watched
//! app's process. Key presses go through `SendInput`, which types into
//! whichever window has the keyboard focus, after asking which process that
//! window belongs to. A click with the cursor moves it with `SetCursorPos`
//! and presses through `SendInput` too, after `WindowFromPoint` says which
//! top-level window shows there; giving the focus back goes through
//! `SetForegroundWindow`. A click sent as messages goes through
//! `PostMessageW`, after the window's own answers say which window a real
//! click at its place would reach: its child windows from `GetWindow`,
//! their styles and bounds, and where the place is on one from
//! `WM_NCHITTEST`. Those answers come in the pixels the window's program
//! counts in, because the calls run in that window's DPI awareness
//! context. Opening a program,
//! file, or link goes through `ShellExecuteExW`, the call behind File
//! Explorer's open, on a short-lived thread of its own, so a slow program
//! cannot hold up the action worker for longer than 2 seconds; while an open
//! that timed out is still running, the next is skipped, so a program that
//! never answers leaves one waiting thread at most. A sound plays through
//! `PlaySoundW`, which returns once the sound is queued, after the adapter
//! checks that its file is there, because the call does not report a
//! missing file when it plays asynchronously.
//!
//! The media session manager may not be ready yet, such as right after
//! sign-in, so the controls start without it and ask for it again when a
//! media action needs it, waiting longer after each failure. A volume
//! control that stops working, such as after its device was unplugged and
//! plugged back in, is found again by its ID; an audio session that is not
//! found again ended for good.

use std::fmt;
use std::io;
use std::marker::PhantomData;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use appytappy_config::action::{Key, MediaCommand, Modifier, OpenTarget, VolumeLevel};
use appytappy_config::anti_cheat::ProcessId;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession, GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus,
};
use windows::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_NOT_FOUND, ERROR_SUCCESS, ERROR_TIMEOUT, HWND, LPARAM, POINT, RECT,
    SetLastError, WPARAM,
};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{
    IAudioSessionControl2, IAudioSessionManager2, IMMDevice, IMMDeviceEnumerator,
    ISimpleAudioVolume, MMDeviceEnumerator, PlaySoundW, SND_ASYNC, SND_FILENAME, SND_NODEFAULT,
    eConsole, eRender,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, COINIT_MULTITHREADED,
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT, GetWindowDpiAwarenessContext, PhysicalToLogicalPointForPerMonitorDPI,
    SetThreadDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBD_EVENT_FLAGS, KEYBDINPUT,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC, MOUSEEVENTF_LEFTDOWN,
    MOUSEEVENTF_LEFTUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEINPUT, MapVirtualKeyW,
    SendInput, VIRTUAL_KEY, VK_BACK, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_F1,
    VK_HOME, VK_INSERT, VK_LBUTTON, VK_LCONTROL, VK_LEFT, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MBUTTON,
    VK_MENU, VK_NEXT, VK_PRIOR, VK_RBUTTON, VK_RETURN, VK_RIGHT, VK_RWIN, VK_SHIFT, VK_SPACE,
    VK_TAB, VK_UP,
};
use windows::Win32::UI::Shell::{
    SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GA_ROOT, GCL_STYLE, GET_WINDOW_CMD, GW_CHILD, GW_HWNDNEXT, GWL_STYLE, GetAncestor,
    GetClassLongPtrW, GetClientRect, GetCursorPos, GetForegroundWindow, GetSystemMetrics,
    GetWindow, GetWindowLongW, GetWindowRect, GetWindowThreadProcessId, IsHungAppWindow,
    PostMessageW, SM_SWAPBUTTON, SMTO_ABORTIFHUNG, SMTO_BLOCK, SW_SHOWNOACTIVATE,
    SendMessageTimeoutW, SetCursorPos, SetForegroundWindow, WM_NCHITTEST, WindowFromPoint,
};
use windows::core::{GUID, IUnknown, Interface, PCWSTR, PWSTR, RuntimeType};
use windows_future::{AsyncStatus, IAsyncOperation};

use crate::{
    Backoff, Controls, Executables, Input, MediaSession, MouseButton, MouseMessage, Press, Stroke,
    Volume, pack,
};

/// How long a media request may take before it counts as failed, so an app
/// that hangs cannot stall the action worker.
const MEDIA_TIMEOUT: Duration = Duration::from_secs(2);

/// How often a pending media request is checked.
const MEDIA_POLL: Duration = Duration::from_millis(5);

/// How long opening a program, file, or link may take before it counts as
/// failed, so a program that is slow to start cannot stall the action
/// worker. What it opens may still open after that.
const OPEN_TIMEOUT: Duration = Duration::from_secs(2);

/// This thread's membership in a COM apartment: the multithreaded one,
/// which Core Audio needs, or a single-threaded one, which some of the
/// shell's handlers for opening files need. It ends on drop, on the thread
/// that joined, because the type cannot move between threads.
#[derive(Debug)]
struct Apartment(PhantomData<*const ()>);

impl Apartment {
    fn join(kind: COINIT) -> io::Result<Self> {
        #[expect(unsafe_code, reason = "a COM call")]
        // SAFETY: the call takes no pointers; a success, including S_FALSE for a
        // thread that already joined, is balanced by CoUninitialize on drop.
        unsafe { CoInitializeEx(None, kind) }.ok()?;
        Ok(Self(PhantomData))
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        #[expect(unsafe_code, reason = "a COM call")]
        // SAFETY: this value's successful CoInitializeEx ran on this thread, since
        // the type is not Send, and every COM object this adapter holds is
        // released before it, because those fields come first.
        unsafe {
            CoUninitialize();
        }
    }
}

/// A media session. Two keys are equal when they hold the same session
/// object, which COM identifies by its `IUnknown` pointer.
#[derive(Debug, Clone)]
pub struct MediaKey {
    session: GlobalSystemMediaTransportControlsSession,
    identity: IUnknown,
}

impl PartialEq for MediaKey {
    fn eq(&self, other: &Self) -> bool {
        ptr::eq(self.identity.as_raw(), other.identity.as_raw())
    }
}

/// A volume control. Two keys are equal when Windows gives them the same ID:
/// a device's endpoint ID, or an audio session's instance ID.
#[derive(Debug, Clone)]
pub struct VolumeKey {
    id: String,
    control: VolumeControl,
}

#[derive(Debug, Clone)]
enum VolumeControl {
    Device(IAudioEndpointVolume),
    /// An audio session, and the endpoint ID of the device it plays on.
    Session(ISimpleAudioVolume, String),
}

impl PartialEq for VolumeKey {
    fn eq(&self, other: &Self) -> bool {
        let same_kind = matches!(
            (&self.control, &other.control),
            (VolumeControl::Device(_), VolumeControl::Device(_))
                | (VolumeControl::Session(..), VolumeControl::Session(..))
        );
        same_kind && self.id == other.id
    }
}

/// The Windows implementation of [`Controls`].
///
/// It joins the calling thread to the COM multithreaded apartment, so it is
/// created and used on the action worker's thread and cannot move to
/// another.
#[derive(Debug)]
pub struct WindowsControls {
    /// The media session manager, or why Windows did not give it yet.
    media: Result<GlobalSystemMediaTransportControlsSessionManager, MediaMissing>,
    devices: IMMDeviceEnumerator,
    opener: Opener,
    input: WindowsInput,
    /// Names each audio session's app.
    executables: Lister,
    // Dropped last, after the COM objects above.
    _apartment: Apartment,
}

impl WindowsControls {
    /// Connects to the audio devices, and to the media session manager if
    /// it is ready. `executables` names each audio session's app.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    pub fn new(executables: Executables) -> io::Result<Self> {
        let apartment = Apartment::join(COINIT_MULTITHREADED)?;
        #[expect(unsafe_code, reason = "a COM call")]
        // SAFETY: the class ID is a static GUID, and the result is a new owned interface.
        let devices = unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }?;
        // A manager that is not ready yet is asked for again by a later
        // media action, so volume and key actions run meanwhile.
        let media = media_manager().map_err(|error| MediaMissing {
            error: error.to_string(),
            backoff: Backoff::new(Instant::now()),
        });
        Ok(Self {
            media,
            devices,
            opener: Opener::default(),
            input: WindowsInput,
            executables: Lister(executables),
            _apartment: apartment,
        })
    }

    /// The media session manager, asked for again when Windows did not give
    /// it before and the wait since the last try is over. Each request can
    /// take up to [`MEDIA_TIMEOUT`], so it is not made on every action.
    fn media(&mut self) -> io::Result<&GlobalSystemMediaTransportControlsSessionManager> {
        if let Err(missing) = &mut self.media {
            let now = Instant::now();
            if missing.backoff.is_over(now) {
                match media_manager() {
                    Ok(manager) => self.media = Ok(manager),
                    Err(error) => {
                        missing.error = error.to_string();
                        missing.backoff.failed_again(now);
                    },
                }
            }
        }
        self.media.as_ref().map_err(|missing| {
            io::Error::other(format!(
                "the media session manager is not ready: {}",
                missing.error
            ))
        })
    }

    /// The device whose endpoint ID is `id`.
    fn device(&self, id: &str) -> io::Result<IMMDevice> {
        let wide: Vec<u16> = id.encode_utf16().chain([0]).collect();
        #[expect(unsafe_code, reason = "a COM call")]
        // SAFETY: `wide` is a nul-terminated UTF-16 string that outlives the
        // call, and the result is a new owned interface.
        Ok(unsafe { self.devices.GetDevice(PCWSTR(wide.as_ptr())) }?)
    }

    /// Finds the control `key` names again, by its ID, for when the
    /// interface it holds stopped working. Answers `None` for an audio
    /// session that is no longer on its device.
    fn find_again(&self, key: &VolumeKey) -> io::Result<Option<VolumeControl>> {
        match &key.control {
            VolumeControl::Device(_) => {
                let device = self.device(&key.id)?;
                #[expect(unsafe_code, reason = "a COM call")]
                // SAFETY: the call activates an interface the type parameter
                // names, with no activation parameters.
                let control: IAudioEndpointVolume = unsafe { device.Activate(CLSCTX_ALL, None) }?;
                Ok(Some(VolumeControl::Device(control)))
            },
            VolumeControl::Session(_, device_id) => {
                let device = self.device(device_id)?;
                for (id, session) in sessions(&device)? {
                    if id == key.id {
                        return Ok(Some(VolumeControl::Session(
                            session.cast()?,
                            device_id.clone(),
                        )));
                    }
                }
                Ok(None)
            },
        }
    }

    /// The default playback device, or `None` when there is none.
    fn default_device(&self) -> io::Result<Option<IMMDevice>> {
        #[expect(unsafe_code, reason = "a COM call")]
        // SAFETY: the call takes two enum values and returns a new owned interface.
        match unsafe { self.devices.GetDefaultAudioEndpoint(eRender, eConsole) } {
            Ok(device) => Ok(Some(device)),
            // With no playback device, the call fails with "not found".
            Err(error) if error.code() == ERROR_NOT_FOUND.to_hresult() => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
}

fn media_session(
    session: GlobalSystemMediaTransportControlsSession,
) -> io::Result<MediaSession<MediaKey>> {
    let status = session.GetPlaybackInfo()?.PlaybackStatus()?;
    Ok(MediaSession {
        app: session.SourceAppUserModelId()?.to_string_lossy(),
        playing: status == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing,
        key: MediaKey {
            identity: session.cast()?,
            session,
        },
    })
}

impl Controls for WindowsControls {
    type Media = MediaKey;
    type Volume = VolumeKey;
    type Input = WindowsInput;

    fn input(&mut self) -> &mut WindowsInput {
        &mut self.input
    }

    fn current_media(&mut self) -> io::Result<Option<MediaSession<MediaKey>>> {
        // With no current session, the call returns null, which arrives as an
        // error carrying a success code.
        match self.media()?.GetCurrentSession() {
            Ok(session) => media_session(session).map(Some),
            Err(error) if error.code().is_ok() => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn media_sessions(&mut self) -> io::Result<Vec<MediaSession<MediaKey>>> {
        self.media()?
            .GetSessions()?
            .into_iter()
            .map(media_session)
            .collect()
    }

    fn media_command(&mut self, session: &MediaKey, command: MediaCommand) -> io::Result<bool> {
        let session = &session.session;
        let request = match command {
            MediaCommand::Play => session.TryPlayAsync()?,
            MediaCommand::Pause => session.TryPauseAsync()?,
            MediaCommand::Next => session.TrySkipNextAsync()?,
            MediaCommand::Previous => session.TrySkipPreviousAsync()?,
        };
        finish(&request)
    }

    fn system_volume(&mut self) -> io::Result<Option<Volume<VolumeKey>>> {
        let Some(device) = self.default_device()? else {
            return Ok(None);
        };
        #[expect(unsafe_code, reason = "a COM call")]
        // SAFETY: the call takes no arguments and returns a string the caller owns.
        let id = take_string(unsafe { device.GetId() }?)?;
        #[expect(unsafe_code, reason = "a COM call")]
        // SAFETY: the call activates an interface the type parameter names, with no
        // activation parameters.
        let control: IAudioEndpointVolume = unsafe { device.Activate(CLSCTX_ALL, None) }?;
        #[expect(unsafe_code, reason = "a COM call")]
        // SAFETY: the call takes no arguments.
        let level = level(unsafe { control.GetMasterVolumeLevelScalar() }?)?;
        Ok(Some(Volume {
            key: VolumeKey {
                id,
                control: VolumeControl::Device(control),
            },
            level,
        }))
    }

    fn app_volumes(&mut self) -> io::Result<Vec<(String, Volume<VolumeKey>)>> {
        let Some(device) = self.default_device()? else {
            return Ok(Vec::new());
        };
        #[expect(unsafe_code, reason = "a COM call")]
        // SAFETY: the call takes no arguments and returns a string the caller owns.
        let device_id = take_string(unsafe { device.GetId() }?)?;
        let executables = (self.executables.0)()?;
        let mut volumes = Vec::new();
        for (id, details) in sessions(&device)? {
            #[expect(unsafe_code, reason = "a COM call")]
            // SAFETY: the call takes no arguments. For a session that spans several
            // processes it still succeeds and names the process that created it.
            let process = unsafe { details.GetProcessId() }?;
            let process = ProcessId::new(process);
            let Some((_, executable)) = executables.iter().find(|(listed, _)| *listed == process)
            else {
                continue;
            };
            let control: ISimpleAudioVolume = details.cast()?;
            #[expect(unsafe_code, reason = "a COM call")]
            // SAFETY: the call takes no arguments.
            let level = level(unsafe { control.GetMasterVolume() }?)?;
            let key = VolumeKey {
                id,
                control: VolumeControl::Session(control, device_id.clone()),
            };
            volumes.push((executable.clone(), Volume { key, level }));
        }
        Ok(volumes)
    }

    fn set_volume(&mut self, control: &VolumeKey, level: VolumeLevel) -> io::Result<bool> {
        // An interface whose device went away keeps failing even after the
        // device returns, so a failure finds the control again by its ID.
        let Err(error) = set_level(&control.control, level) else {
            return Ok(true);
        };
        match self.find_again(control) {
            Ok(Some(found)) => set_level(&found, level).map(|()| true),
            Ok(None) => Ok(false),
            // The device is still missing, so the first error says why.
            Err(_missing) => Err(error),
        }
    }

    fn open(&mut self, target: &OpenTarget) -> io::Result<()> {
        self.opener.open(target)
    }

    fn play_sound(&mut self, file: &Path) -> io::Result<bool> {
        Ok(play_file(file))
    }
}

/// The Windows implementation of [`Input`]: the keyboard, the mouse, and
/// the windows, through calls that need no COM, so it works on any thread.
#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsInput;

impl Input for WindowsInput {
    fn focused_process(&mut self) -> io::Result<Option<ProcessId>> {
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the call takes no arguments.
        let window = unsafe { GetForegroundWindow() };
        if window.is_invalid() {
            return Ok(None);
        }
        let mut process = 0_u32;
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `window` came from Windows, and `process` outlives the call.
        unsafe {
            GetWindowThreadProcessId(window, Some(&raw mut process))
        };
        Ok((process != 0).then(|| ProcessId::new(process)))
    }

    fn held_modifiers(&mut self) -> io::Result<Vec<Modifier>> {
        Ok(Modifier::ALL
            .into_iter()
            .filter(|&modifier| {
                held_keys(modifier).iter().any(|key| {
                    #[expect(unsafe_code, reason = "a Win32 call")]
                    // SAFETY: the call takes one integer and reads no memory.
                    let state = unsafe { GetAsyncKeyState(i32::from(key.0)) };
                    // The high bit is set while the key is down.
                    state < 0
                })
            })
            .collect())
    }

    fn send_strokes(&mut self, strokes: &[Stroke]) -> io::Result<usize> {
        if strokes.is_empty() {
            return Ok(0);
        }
        let inputs: Vec<INPUT> = strokes.iter().copied().map(input).collect();
        let size = i32::try_from(size_of::<INPUT>()).map_err(io::Error::other)?;
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the slice holds initialized keyboard inputs, and `size` is
        // the size of one, as the call requires.
        let sent = unsafe { SendInput(&inputs, size) };
        Ok(usize::try_from(sent).unwrap_or(0))
    }

    fn front_window(&mut self) -> io::Result<Option<u64>> {
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the call takes no arguments.
        let window = unsafe { GetForegroundWindow() };
        Ok(number(window))
    }

    fn window_at(&mut self, x: i32, y: i32) -> io::Result<Option<u64>> {
        #[expect(unsafe_code, reason = "Win32 calls")]
        // SAFETY: the calls take a point and a window handle Windows gave,
        // and read no memory of ours.
        let root = unsafe {
            let window = WindowFromPoint(POINT { x, y });
            if window.is_invalid() {
                return Ok(None);
            }
            GetAncestor(window, GA_ROOT)
        };
        Ok(number(root))
    }

    fn bring_to_front(&mut self, window: u64) -> io::Result<bool> {
        let handle = handle(window)?;
        #[expect(unsafe_code, reason = "Win32 calls")]
        // SAFETY: the calls take a window handle and fail without harm when
        // no such window exists.
        let front = unsafe {
            // Windows lets only the program in front, or one sharing its
            // input, move the foreground; a click runs while another
            // program is in front, so this thread shares that program's
            // input for the moment it asks, then stops sharing it.
            // A hung window is left alone, since sharing its input could
            // wait on it.
            let ours = GetCurrentThreadId();
            let foreground = GetForegroundWindow();
            let theirs = GetWindowThreadProcessId(foreground, None);
            let attached = theirs != 0
                && theirs != ours
                && !IsHungAppWindow(foreground).as_bool()
                && AttachThreadInput(ours, theirs, true).as_bool();
            // It may still be refused, so the answer is whether the window
            // is in front after.
            let _asked = SetForegroundWindow(handle);
            if attached {
                let _detached = AttachThreadInput(ours, theirs, false);
            }
            GetForegroundWindow()
        };
        Ok(front == handle)
    }

    fn cursor(&mut self) -> io::Result<(i32, i32)> {
        let mut point = POINT::default();
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `point` is a live local the call writes.
        unsafe { GetCursorPos(&raw mut point) }?;
        Ok((point.x, point.y))
    }

    fn move_cursor(&mut self, x: i32, y: i32) -> io::Result<()> {
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the call takes two integers and reads no memory.
        unsafe { SetCursorPos(x, y) }?;
        Ok(())
    }

    fn buttons_held(&mut self) -> io::Result<bool> {
        Ok([VK_LBUTTON, VK_RBUTTON, VK_MBUTTON]
            .into_iter()
            .any(|button| {
                #[expect(unsafe_code, reason = "a Win32 call")]
                // SAFETY: the call takes one integer and reads no memory.
                let state = unsafe { GetAsyncKeyState(i32::from(button.0)) };
                // The high bit is set while the button is down.
                state < 0
            }))
    }

    fn send_presses(&mut self, presses: &[Press]) -> io::Result<usize> {
        if presses.is_empty() {
            return Ok(0);
        }
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the call takes one integer and reads no memory.
        let swapped = unsafe { GetSystemMetrics(SM_SWAPBUTTON) } != 0;
        let inputs: Vec<INPUT> = presses
            .iter()
            .map(|&press| mouse_input(press, swapped))
            .collect();
        let size = i32::try_from(size_of::<INPUT>()).map_err(io::Error::other)?;
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the slice holds initialized mouse inputs, and `size` is the
        // size of one, as the call requires.
        let sent = unsafe { SendInput(&inputs, size) };
        Ok(usize::try_from(sent).unwrap_or(0))
    }

    fn hold(&mut self, time: Duration) {
        thread::sleep(time);
    }

    fn now(&mut self) -> Instant {
        Instant::now()
    }

    fn own_place(&mut self, window: u64, x: i32, y: i32) -> io::Result<(i32, i32)> {
        let handle = handle(window)?;
        let mut point = POINT { x, y };
        forget_last_error();
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `point` is a live local the call reads and writes, and the
        // call fails without harm when no such window exists.
        let converted =
            unsafe { PhysicalToLogicalPointForPerMonitorDPI(Some(handle), &raw mut point) };
        if !converted.as_bool() {
            return Err(failed(
                "the place could not be converted to the window's own pixels",
            ));
        }
        Ok((point.x, point.y))
    }

    fn client_point(&mut self, window: u64, x: i32, y: i32) -> io::Result<(i32, i32)> {
        let handle = handle(window)?;
        let _counting = CountsAs::window(handle)?;
        let mut point = POINT { x, y };
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `point` is a live local the call reads and writes, and the
        // call fails without harm when no such window exists.
        let converted = unsafe { ScreenToClient(handle, &raw mut point) };
        if !converted.as_bool() {
            return Err(gone());
        }
        Ok((point.x, point.y))
    }

    fn client_size(&mut self, window: u64) -> io::Result<(i32, i32)> {
        let handle = handle(window)?;
        let _counting = CountsAs::window(handle)?;
        let mut area = RECT::default();
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `area` is a live local the call writes, and the call fails
        // without harm when no such window exists.
        unsafe { GetClientRect(handle, &raw mut area) }?;
        // A client area starts at (0, 0), so its far corner is its size.
        Ok((area.right, area.bottom))
    }

    fn top_child(&mut self, window: u64) -> io::Result<Option<u64>> {
        Ok(related(handle(window)?, GW_CHILD))
    }

    fn below(&mut self, window: u64) -> io::Result<Option<u64>> {
        Ok(related(handle(window)?, GW_HWNDNEXT))
    }

    fn style(&mut self, window: u64) -> io::Result<u32> {
        let handle = handle(window)?;
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the call takes a window handle and an index, reads no
        // memory of ours, and answers 0 when no such window exists.
        let style = unsafe { GetWindowLongW(handle, GWL_STYLE) };
        // The style is 32 bits, which the call hands over as a signed number.
        Ok(u32::from_ne_bytes(style.to_ne_bytes()))
    }

    fn bounds(&mut self, window: u64) -> io::Result<(i32, i32, i32, i32)> {
        let handle = handle(window)?;
        let _counting = CountsAs::window(handle)?;
        let mut edges = RECT::default();
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `edges` is a live local the call writes, and the call
        // fails without harm when no such window exists.
        unsafe { GetWindowRect(handle, &raw mut edges) }?;
        Ok((edges.left, edges.top, edges.right, edges.bottom))
    }

    fn hit_test(&mut self, window: u64, at: (i16, i16), wait: Duration) -> io::Result<usize> {
        let handle = handle(window)?;
        let place = isize::try_from(pack(at)).map_err(io::Error::other)?;
        let wait = u32::try_from(wait.as_millis()).unwrap_or(u32::MAX);
        let mut answer = 0_usize;
        // The call does not always say why it failed, so an error that an
        // earlier call left is cleared first.
        forget_last_error();
        // While it waits, this thread handles no message that another thread
        // sends it (`SMTO_BLOCK`). Without the flag, Windows lets such a
        // message run during the wait, and so in the middle of a click.
        // Nothing sends the clicking threads a message today: neither the
        // action thread nor the setup app's owner thread has a window, and
        // the action thread is in COM's multithreaded apartment, whose calls
        // arrive on other threads and not as window messages, as a
        // single-threaded apartment's do. The flag keeps a click whole on
        // any thread that makes this call. Its cost is that a window which
        // sends this thread a message while it answers waits for nothing,
        // so the wait runs out and the click is not sent.
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `answer` is a live local the call writes before it
        // returns. The call hands the window two integers, waits no longer
        // than `wait` for the window's answer, and fails without harm when
        // no such window exists.
        let asked = unsafe {
            SendMessageTimeoutW(
                handle,
                WM_NCHITTEST,
                WPARAM(0),
                LPARAM(place),
                SMTO_ABORTIFHUNG | SMTO_BLOCK,
                wait,
                Some(&raw mut answer),
            )
        };
        if asked.0 != 0 {
            return Ok(answer);
        }
        // The system's error says why: the wait ran out, or Windows refuses
        // a message to a window of a program that runs with more rights
        // than this one, which reads as a denied permission.
        Err(failed("the window did not answer where the place is on it"))
    }

    fn class_style(&mut self, window: u64) -> io::Result<u32> {
        let handle = handle(window)?;
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the call takes a window handle and an index, reads no
        // memory of ours, and answers 0 when no such window exists.
        let style = unsafe { GetClassLongPtrW(handle, GCL_STYLE) };
        // A class's style is 32 bits in a wider number.
        u32::try_from(style).map_err(io::Error::other)
    }

    fn post(&mut self, window: u64, message: MouseMessage) -> io::Result<()> {
        let handle = handle(window)?;
        // A message for no window would go to this thread's own queue.
        if handle.is_invalid() {
            return Err(gone());
        }
        let place = isize::try_from(message.place()).map_err(io::Error::other)?;
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the call copies a number and two integers into the
        // window's message queue, hands over no memory of ours, and fails
        // without harm when no such window exists.
        let posted = unsafe {
            PostMessageW(
                Some(handle),
                message.number(),
                WPARAM(message.held()),
                LPARAM(place),
            )
        };
        match posted {
            Ok(()) => Ok(()),
            // Windows refuses a message to a window of a program that runs
            // with more rights than this one, and says so with this error.
            Err(error) if error.code() == ERROR_ACCESS_DENIED.to_hresult() => {
                Err(io::ErrorKind::PermissionDenied.into())
            },
            Err(error) => Err(error.into()),
        }
    }
}

/// Opens programs, files, and links, one at a time.
#[derive(Debug, Default)]
struct Opener {
    /// The thread of an open that timed out, which may still be waiting on
    /// the shell. An open that answered in time leaves none, so the next open
    /// right after it runs.
    waiting: Option<JoinHandle<()>>,
}

impl Opener {
    /// Opens `target` on a short-lived thread, waiting for it at most
    /// [`OPEN_TIMEOUT`]. It skips `target` while an open that timed out is
    /// still running, so a target that never answers leaves one waiting
    /// thread.
    fn open(&mut self, target: &OpenTarget) -> io::Result<()> {
        if let Some(last) = self.waiting.take() {
            if !last.is_finished() {
                self.waiting = Some(last);
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "the last program, file, or link is still opening, so this one was skipped",
                ));
            }
            wait_out(last);
        }
        let file = wide(target.as_str().encode_utf16());
        let folder = target.folder().map(|folder| wide(folder.encode_utf16()));
        let (done, outcome) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("open".to_owned())
            .spawn(move || {
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "after a timeout nobody waits for the answer"
                )]
                let _ = done.send(shell_open(&file, folder.as_deref()));
            })?;
        match outcome.recv_timeout(OPEN_TIMEOUT) {
            Ok(opened) => {
                wait_out(thread);
                opened
            },
            Err(RecvTimeoutError::Timeout) => {
                // Not joined: it finishes on its own, and its answer goes
                // nowhere.
                self.waiting = Some(thread);
                Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!(
                        "opening it did not finish within {} seconds, so it may still open",
                        OPEN_TIMEOUT.as_secs()
                    ),
                ))
            },
            Err(RecvTimeoutError::Disconnected) => {
                wait_out(thread);
                Err(io::Error::other("the thread that opens it stopped"))
            },
        }
    }
}

/// Waits for an open's `thread`, which has answered or stopped, to end.
fn wait_out(thread: JoinHandle<()>) {
    #[expect(
        clippy::let_underscore_must_use,
        reason = "the answer, or its absence, was already read from the channel"
    )]
    let _ = thread.join();
}

/// Starts playing the WAV file at `file` without waiting for it to end,
/// stopping any sound this process is still playing. Answers whether it
/// started: `false` for a file that is not there, which `PlaySoundW` does not
/// report when it plays asynchronously, or one the call refused.
fn play_file(file: &Path) -> bool {
    if !file.is_file() {
        return false;
    }
    let name = wide(file.as_os_str().encode_wide());
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: `name` is a nul-terminated UTF-16 path that lives through
    // the call. For a file name, PlaySoundW copies the name before it
    // returns; only a sound in memory must outlive an asynchronous call.
    let started = unsafe {
        PlaySoundW(
            PCWSTR(name.as_ptr()),
            None,
            SND_FILENAME | SND_ASYNC | SND_NODEFAULT,
        )
    };
    started.as_bool()
}

/// UTF-16 `units` with the nul that ends a Windows string.
fn wide(units: impl Iterator<Item = u16>) -> Vec<u16> {
    units.chain([0]).collect()
}

/// Opens `file`, a nul-terminated UTF-16 path or link, with its default
/// action and no arguments, showing no error dialog. A program starts in
/// `folder`, a nul-terminated UTF-16 path, when one is given. It asks the
/// opened window not to take the keyboard focus, though a program may take
/// it anyway. It runs on a thread of its own in a single-threaded apartment,
/// as the shell asks of its callers.
fn shell_open(file: &[u16], folder: Option<&[u16]>) -> io::Result<()> {
    let _apartment = Apartment::join(COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE)?;
    let mut info = SHELLEXECUTEINFOW {
        cbSize: u32::try_from(size_of::<SHELLEXECUTEINFOW>()).map_err(io::Error::other)?,
        // The thread ends after the call, so the call must finish first.
        fMask: SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpFile: PCWSTR(file.as_ptr()),
        lpDirectory: folder.map_or(PCWSTR::null(), |folder| PCWSTR(folder.as_ptr())),
        nShow: SW_SHOWNOACTIVATE.0,
        ..Default::default()
    };
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: `info` is a SHELLEXECUTEINFOW whose cbSize is set and whose
    // file and folder, when given, point to nul-terminated strings that
    // outlive the call; every other pointer is null, which the call allows.
    unsafe { ShellExecuteExW(&raw mut info) }?;
    Ok(())
}

/// The keys that count as `modifier` held down: either Ctrl, Alt, or
/// Shift, and either Windows key.
const fn held_keys(modifier: Modifier) -> &'static [VIRTUAL_KEY] {
    match modifier {
        Modifier::Ctrl => &[VK_CONTROL],
        Modifier::Alt => &[VK_MENU],
        Modifier::Shift => &[VK_SHIFT],
        Modifier::Win => &[VK_LWIN, VK_RWIN],
    }
}

/// The keyboard input for one stroke. It names the key both by its virtual
/// key and by its scan code, because some applications read only one.
fn input(stroke: Stroke) -> INPUT {
    let ((key, extended), down) = match stroke {
        Stroke::Modifier(modifier, down) => (modifier_key(modifier), down),
        Stroke::Key(key, down) => (virtual_key(key), down),
    };
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: the call takes two integers and reads no memory.
    let scan = unsafe { MapVirtualKeyW(u32::from(key.0), MAPVK_VK_TO_VSC) };
    let mut flags = if down {
        KEYBD_EVENT_FLAGS(0)
    } else {
        KEYEVENTF_KEYUP
    };
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: u16::try_from(scan).unwrap_or(0),
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// The mouse input for one press, at wherever the cursor is.
fn mouse_input(press: Press, swapped: bool) -> INPUT {
    // Injected events name the physical buttons, so with the buttons
    // swapped in Windows' settings the left click is the right button.
    let button = match (press.button, swapped) {
        (MouseButton::Left, false) | (MouseButton::Right, true) => MouseButton::Left,
        (MouseButton::Right, false) | (MouseButton::Left, true) => MouseButton::Right,
    };
    let flags = match (button, press.down) {
        (MouseButton::Left, true) => MOUSEEVENTF_LEFTDOWN,
        (MouseButton::Left, false) => MOUSEEVENTF_LEFTUP,
        (MouseButton::Right, true) => MOUSEEVENTF_RIGHTDOWN,
        (MouseButton::Right, false) => MOUSEEVENTF_RIGHTUP,
    };
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// This thread counting pixels the way a window's program does, until the
/// value is dropped.
///
/// A window that Windows stretches, because its program does not follow its
/// monitor's scale, counts in its own pixels, and the mouse messages it
/// gets name places in them. Microsoft documents that Windows runs a
/// window's procedure in the DPI awareness context the window was made in,
/// and that a thread's API calls run in the context the thread has taken
/// ("Mixed-Mode DPI Scaling and DPI-aware APIs",
/// learn.microsoft.com/windows/win32/hidpi). So while this thread has the
/// window's context, each call answers what the window's own code would
/// get, with no scaling of ours. `PhysicalToLogicalPointForPerMonitorDPI`
/// is the documented way to put a screen place in the window's pixels
/// first. None of this is checked yet on a window that is stretched.
#[derive(Debug)]
struct CountsAs(DPI_AWARENESS_CONTEXT);

impl CountsAs {
    /// Takes `window`'s DPI awareness context for this thread.
    fn window(window: HWND) -> io::Result<Self> {
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the call takes a window handle, reads no memory of ours,
        // and answers null when no such window exists.
        let theirs = unsafe { GetWindowDpiAwarenessContext(window) };
        if theirs.0.is_null() {
            return Err(gone());
        }
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the call takes a context Windows gave, changes only this
        // thread's own setting, and answers null when it refuses the context.
        let ours = unsafe { SetThreadDpiAwarenessContext(theirs) };
        if ours.0.is_null() {
            return Err(io::Error::other(
                "Windows did not let AppyTappy count pixels the way the window's program does",
            ));
        }
        Ok(Self(ours))
    }
}

impl Drop for CountsAs {
    fn drop(&mut self) {
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the context is the one this thread had, which Windows gave
        // when this value was made on this thread, since the type is not
        // Send. The call changes only this thread's own setting.
        let _replaced = unsafe { SetThreadDpiAwarenessContext(self.0) };
    }
}

/// The window that stands in `relation` to `window`, such as its topmost
/// child, or `None` when it has none. The call answers nothing as well for
/// a window that closed, which the calls around it report.
fn related(window: HWND, relation: GET_WINDOW_CMD) -> Option<u64> {
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: the call takes a window handle and a relation, reads no
    // memory of ours, and fails without harm when no such window exists.
    let found = unsafe { GetWindow(window, relation) };
    found.ok().and_then(number)
}

/// A Windows call that failed: what it was for, and the system's error.
#[derive(Debug)]
struct Failed {
    /// What the call was for, as a clause.
    doing: &'static str,
    /// The system's last error after the call.
    cause: io::Error,
}

impl fmt::Display for Failed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // A call that leaves no error code behind did not say why.
        if self.cause.raw_os_error() == Some(0) {
            write!(formatter, "{}, and Windows gave no reason", self.doing)
        } else {
            write!(formatter, "{}: {}", self.doing, self.cause)
        }
    }
}

impl std::error::Error for Failed {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}

/// The error for a call that says only that it failed: `doing`, what the
/// call was for, with the system's last error as its kind and its cause.
/// [`forget_last_error`] runs before such a call, so the error is its own.
fn failed(doing: &'static str) -> io::Error {
    let cause = io::Error::last_os_error();
    io::Error::new(kind(&cause), Failed { doing, cause })
}

/// The kind of the system error `cause`. The two kinds that a click tells
/// apart, a wait that ran out and a refused message, are read from the
/// system's own codes, so they do not depend on what the standard library
/// makes of those codes.
fn kind(cause: &io::Error) -> io::ErrorKind {
    let code = cause
        .raw_os_error()
        .and_then(|code| u32::try_from(code).ok());
    if code == Some(ERROR_TIMEOUT.0) {
        io::ErrorKind::TimedOut
    } else if code == Some(ERROR_ACCESS_DENIED.0) {
        io::ErrorKind::PermissionDenied
    } else {
        cause.kind()
    }
}

/// Clears this thread's last error, before a call that does not always set
/// one when it fails.
fn forget_last_error() {
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: the call takes one integer and changes only this thread's own
    // last error.
    unsafe {
        SetLastError(ERROR_SUCCESS);
    }
}

/// The error for a window that Windows no longer knows, where the call says
/// only that it failed.
fn gone() -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        "Windows does not know the window, which may have closed",
    )
}

/// The window handle a number from the runtime names.
fn handle(window: u64) -> io::Result<HWND> {
    Ok(HWND(ptr::without_provenance_mut(
        usize::try_from(window).map_err(io::Error::other)?,
    )))
}

/// A window handle as the number the runtime names it by, or `None` for no
/// window.
fn number(window: HWND) -> Option<u64> {
    (!window.is_invalid())
        .then(|| u64::try_from(window.0.addr()).ok())
        .flatten()
}

/// A modifier's left-hand key, and whether it is an extended key.
const fn modifier_key(modifier: Modifier) -> (VIRTUAL_KEY, bool) {
    match modifier {
        Modifier::Ctrl => (VK_LCONTROL, false),
        Modifier::Alt => (VK_LMENU, false),
        Modifier::Shift => (VK_LSHIFT, false),
        Modifier::Win => (VK_LWIN, true),
    }
}

/// A key's virtual key, and whether it is an extended key: the keys of the
/// navigation block and the arrows are, so Windows does not read them as the
/// number pad's.
fn virtual_key(key: Key) -> (VIRTUAL_KEY, bool) {
    match key {
        // Letters and digits are their own virtual keys, `A` to `Z` and `0`
        // to `9`, and a function key counts up from F1.
        Key::Letter(key) | Key::Digit(key) => (
            VIRTUAL_KEY(u16::try_from(u32::from(key)).unwrap_or(0)),
            false,
        ),
        Key::Function(number) => (
            VIRTUAL_KEY(VK_F1.0.saturating_add(u16::from(number.saturating_sub(1)))),
            false,
        ),
        Key::Space => (VK_SPACE, false),
        Key::Enter => (VK_RETURN, false),
        Key::Tab => (VK_TAB, false),
        Key::Escape => (VK_ESCAPE, false),
        Key::Backspace => (VK_BACK, false),
        Key::Insert => (VK_INSERT, true),
        Key::Delete => (VK_DELETE, true),
        Key::Home => (VK_HOME, true),
        Key::End => (VK_END, true),
        Key::PageUp => (VK_PRIOR, true),
        Key::PageDown => (VK_NEXT, true),
        Key::Up => (VK_UP, true),
        Key::Down => (VK_DOWN, true),
        Key::Left => (VK_LEFT, true),
        Key::Right => (VK_RIGHT, true),
    }
}

/// Why the media session manager is missing, and when to ask for it again.
#[derive(Debug)]
struct MediaMissing {
    /// The last error asking for it.
    error: String,
    /// When to ask again.
    backoff: Backoff,
}

/// Asks Windows for the media session manager.
fn media_manager() -> io::Result<GlobalSystemMediaTransportControlsSessionManager> {
    finish(&GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?)
}

/// Sets one control's level.
fn set_level(control: &VolumeControl, level: VolumeLevel) -> io::Result<()> {
    match control {
        VolumeControl::Device(device) => {
            #[expect(unsafe_code, reason = "a COM call")]
            // SAFETY: the level is in 0 to 1, and a null event context is allowed.
            unsafe { device.SetMasterVolumeLevelScalar(level.scalar(), ptr::null::<GUID>()) }?;
        },
        VolumeControl::Session(session, _) => {
            #[expect(unsafe_code, reason = "a COM call")]
            // SAFETY: the level is in 0 to 1, and a null event context is allowed.
            unsafe { session.SetMasterVolume(level.scalar(), ptr::null::<GUID>()) }?;
        },
    }
    Ok(())
}

/// Every audio session on `device`, each with its instance ID.
fn sessions(device: &IMMDevice) -> io::Result<Vec<(String, IAudioSessionControl2)>> {
    #[expect(unsafe_code, reason = "a COM call")]
    // SAFETY: the call activates an interface the type parameter names, with no
    // activation parameters.
    let manager: IAudioSessionManager2 = unsafe { device.Activate(CLSCTX_ALL, None) }?;
    #[expect(unsafe_code, reason = "a COM call")]
    // SAFETY: the call takes no arguments and returns a new owned interface.
    let enumerator = unsafe { manager.GetSessionEnumerator() }?;
    #[expect(unsafe_code, reason = "a COM call")]
    // SAFETY: the call takes no arguments.
    let count = unsafe { enumerator.GetCount() }?;
    let mut sessions = Vec::new();
    for index in 0..count {
        #[expect(unsafe_code, reason = "a COM call")]
        // SAFETY: `index` is below the count the enumerator reported.
        let session = unsafe { enumerator.GetSession(index) }?;
        let details: IAudioSessionControl2 = session.cast()?;
        #[expect(unsafe_code, reason = "a COM call")]
        // SAFETY: the call takes no arguments and returns a string the caller owns.
        let id = take_string(unsafe { details.GetSessionInstanceIdentifier() }?)?;
        sessions.push((id, details));
    }
    Ok(sessions)
}

/// Waits for `request`, for at most [`MEDIA_TIMEOUT`], and cancels it if it
/// takes longer.
fn finish<T: RuntimeType + 'static>(request: &IAsyncOperation<T>) -> io::Result<T> {
    let deadline = Instant::now()
        .checked_add(MEDIA_TIMEOUT)
        .ok_or_else(|| io::Error::other("the clock cannot represent the deadline"))?;
    while request.Status()? == AsyncStatus::Started {
        if Instant::now() >= deadline {
            request.Cancel()?;
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "the media app did not answer",
            ));
        }
        thread::sleep(MEDIA_POLL);
    }
    Ok(request.GetResults()?)
}

/// A volume level Windows reported, which it keeps from 0 to 1.
fn level(scalar: f32) -> io::Result<VolumeLevel> {
    VolumeLevel::from_scalar(scalar)
        .ok_or_else(|| io::Error::other(format!("Windows reported volume {scalar}")))
}

/// Copies a string that COM allocated for the caller, then frees it.
fn take_string(text: PWSTR) -> io::Result<String> {
    #[expect(unsafe_code, reason = "reading a COM string")]
    // SAFETY: the COM call that returned `text` succeeded, so it points to a
    // nul-terminated UTF-16 string that stays valid until it is freed below.
    let copied = unsafe { text.to_string() };
    #[expect(unsafe_code, reason = "a COM call")]
    // SAFETY: the string came from CoTaskMemAlloc, as the COM call documents, and
    // is freed once, after the copy.
    unsafe {
        CoTaskMemFree(Some(text.0.cast_const().cast()));
    }
    copied.map_err(io::Error::other)
}

/// The caller's process lister, which has no `Debug` of its own.
struct Lister(Executables);

impl fmt::Debug for Lister {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Lister")
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant};

    use appytappy_config::action::OpenTarget;
    use appytappy_config::anti_cheat::ProcessId;
    use appytappy_config::click::{ClickButton, ClickDelivery};
    use windows::Win32::Foundation::{
        ERROR_ACCESS_DENIED, ERROR_INVALID_WINDOW_HANDLE, ERROR_TIMEOUT, HINSTANCE, HWND, LPARAM,
        LRESULT, POINT, WIN32_ERROR, WPARAM,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::SystemServices::{MK_LBUTTON, MK_RBUTTON};
    use windows::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, DPI_AWARENESS_CONTEXT_UNAWARE,
        LogicalToPhysicalPointForPerMonitorDPI, SetThreadDpiAwarenessContext,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        BS_GROUPBOX, CS_DBLCLKS, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
        GetMessageW, HTCLIENT, HTTRANSPARENT, HWND_TOP, MSG, PM_REMOVE, PeekMessageW, PostMessageW,
        PostQuitMessage, RegisterClassW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
        WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_DESTROY, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN,
        WM_LBUTTONUP, WM_MOUSEFIRST, WM_MOUSELAST, WM_MOUSEMOVE, WM_NCHITTEST, WM_RBUTTONDOWN,
        WM_RBUTTONUP, WNDCLASS_STYLES, WNDCLASSW, WS_CHILD, WS_DISABLED, WS_POPUP, WS_VISIBLE,
    };
    use windows::core::{PCWSTR, w};

    use super::{Opener, WindowsInput, handle, kind, number, play_file, shell_open, wide};
    use crate::{ActionError, Input, MouseEvent, MouseMessage, PlacedClick, Watched, click};

    #[test]
    fn a_missing_sound_file_does_not_play() {
        let missing = env::temp_dir().join("appytappy-test-missing-sound.wav");
        assert!(!missing.exists(), "{}", missing.display());
        assert!(!play_file(&missing));
    }

    #[test]
    fn opening_a_missing_program_fails_without_a_dialog() {
        let folder = env::temp_dir();
        let missing = folder.join("appytappy-test-missing-program.exe");
        assert!(!missing.exists(), "{}", missing.display());
        let file = wide(missing.as_os_str().encode_wide());
        let folder = wide(folder.as_os_str().encode_wide());
        assert!(shell_open(&file, Some(&folder)).is_err());
    }

    #[test]
    fn an_open_that_answered_does_not_hold_up_the_next() {
        let missing = env::temp_dir()
            .join("appytappy-test-missing-folder")
            .join("program.exe");
        assert!(!missing.exists(), "{}", missing.display());
        let target =
            OpenTarget::parse(missing.to_str().expect("a UTF-8 path")).expect("a full path");
        let mut opener = Opener::default();
        for time in ["first", "second", "third"] {
            let error = opener.open(&target).expect_err("the program is missing");
            assert_ne!(
                error.kind(),
                std::io::ErrorKind::WouldBlock,
                "{time}: {error}"
            );
        }
    }

    /// Where the test window's client area starts on the screen.
    const ORIGIN: (i32, i32) = (300, 200);

    /// A window class of this program whose windows take every click in
    /// their client area and are not told about double clicks.
    const PLAIN: PCWSTR = w!("AppyTappy.Test.Plain");

    /// A window class like [`PLAIN`] whose windows are told about double
    /// clicks.
    const DOUBLE_CLICKS: PCWSTR = w!("AppyTappy.Test.DoubleClicks");

    /// The procedure of the test classes, which leaves every message to
    /// Windows.
    extern "system" fn leave_to_windows(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the arguments are the ones Windows called this procedure
        // with.
        unsafe {
            DefWindowProcW(window, message, wparam, lparam)
        }
    }

    /// This program's module, which owns the test classes.
    fn module() -> HINSTANCE {
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: no module name asks for the running program's own module.
        let module = unsafe { GetModuleHandleW(None) }.expect("this program's module");
        module.into()
    }

    /// Makes this thread see the screen's own pixels, as the runtime and the
    /// setup app do, and registers the test classes. A class that another
    /// test of this process registered already stays as it is.
    fn set_up() {
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the call takes a predefined context and changes only this
        // thread's own setting.
        let before =
            unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        assert!(!before.0.is_null(), "Windows knows the context");
        for (name, style) in [(PLAIN, WNDCLASS_STYLES(0)), (DOUBLE_CLICKS, CS_DBLCLKS)] {
            let class = WNDCLASSW {
                style,
                lpfnWndProc: Some(leave_to_windows),
                hInstance: module(),
                lpszClassName: name,
                ..WNDCLASSW::default()
            };
            #[expect(unsafe_code, reason = "a Win32 call")]
            // SAFETY: `class` is a filled WNDCLASSW whose class name is a
            // static wide string and whose procedure lives for the whole
            // program.
            let _atom = unsafe { RegisterClassW(&raw const class) };
        }
    }

    /// A window of this thread that is never shown, 400 by 300 at
    /// [`ORIGIN`] with no border, so its client area is all of it. Its
    /// child windows go when it does.
    struct TestWindow(HWND);

    impl TestWindow {
        /// A window of `class`, which [`set_up`] registered.
        fn new(class: PCWSTR) -> Self {
            #[expect(unsafe_code, reason = "a Win32 call")]
            // SAFETY: the class is registered, the title is a static wide
            // string, and the window has no parent, menu, or creation data.
            let window = unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    class,
                    w!("AppyTappy click test"),
                    WS_POPUP,
                    ORIGIN.0,
                    ORIGIN.1,
                    400,
                    300,
                    None,
                    None,
                    Some(module()),
                    None,
                )
            }
            .expect("create a window");
            Self(window)
        }

        /// A child window of `class` with `style`, 100 by 80 at (`x`, `y`)
        /// of the client area, on top of the children made before it.
        fn child(&self, class: PCWSTR, style: WINDOW_STYLE, x: i32, y: i32) -> u64 {
            #[expect(unsafe_code, reason = "a Win32 call")]
            // SAFETY: the class is a system class or registered, the title
            // is a static wide string, the parent is this thread's live
            // window, and the child has no menu or creation data.
            let child = unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    class,
                    w!("AppyTappy click test child"),
                    WS_CHILD | style,
                    x,
                    y,
                    100,
                    80,
                    Some(self.0),
                    None,
                    Some(module()),
                    None,
                )
            }
            .expect("create a child window");
            #[expect(unsafe_code, reason = "a Win32 call")]
            // SAFETY: the child is this thread's live window, and the call
            // moves it only among its siblings.
            unsafe {
                SetWindowPos(
                    child,
                    Some(HWND_TOP),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                )
            }
            .expect("put the child on top");
            number(child).expect("a window")
        }

        fn number(&self) -> u64 {
            number(self.0).expect("a window")
        }
    }

    impl Drop for TestWindow {
        fn drop(&mut self) {
            #[expect(unsafe_code, reason = "a Win32 call")]
            // SAFETY: this thread created the window and destroys it once.
            let destroyed = unsafe { DestroyWindow(self.0) };
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a test window that cannot be destroyed goes with the test's thread"
            )]
            let _ = destroyed;
        }
    }

    /// The screen place of (`x`, `y`) in the test window's client area.
    fn on_screen(x: i32, y: i32) -> (i32, i32) {
        (
            ORIGIN.0.checked_add(x).expect("on the screen"),
            ORIGIN.1.checked_add(y).expect("on the screen"),
        )
    }

    /// Takes every mouse message waiting in this thread's queue, for any of
    /// its windows: the window each is for, its number, its `wParam`, and
    /// its `lParam`.
    fn thread_mouse_messages() -> Vec<(Option<u64>, u32, usize, isize)> {
        let mut taken = Vec::new();
        loop {
            let mut message = MSG::default();
            #[expect(unsafe_code, reason = "a Win32 call")]
            // SAFETY: `message` is a live local the call writes, and no
            // window asks for this thread's messages of every window.
            let found = unsafe {
                PeekMessageW(
                    &raw mut message,
                    None,
                    WM_MOUSEFIRST,
                    WM_MOUSELAST,
                    PM_REMOVE,
                )
            };
            if !found.as_bool() {
                return taken;
            }
            taken.push((
                number(message.hwnd),
                message.message,
                message.wParam.0,
                message.lParam.0,
            ));
        }
    }

    /// The low 32 bits of what a window answered to where a place is on it.
    fn low(answer: usize) -> u32 {
        let [first, second, third, fourth, ..] = answer.to_le_bytes();
        u32::from_le_bytes([first, second, third, fourth])
    }

    fn watched() -> Watched {
        Watched {
            process: ProcessId::new(std::process::id()),
            executable: "test.exe".to_owned(),
        }
    }

    /// A click sent as messages to (`x`, `y`) of `window`'s client area.
    fn click_at(window: &TestWindow, button: ClickButton, x: i32, y: i32) -> PlacedClick {
        PlacedClick {
            window: window.number(),
            process: ProcessId::new(std::process::id()),
            app: "Test".to_owned(),
            at: on_screen(x, y),
            button,
            delivery: ClickDelivery::Message,
        }
    }

    /// The window that a left click sent as messages to (`x`, `y`) of
    /// `window`'s client area reaches, and the place its messages name.
    fn reached(window: &TestWindow, x: i32, y: i32) -> (Option<u64>, isize) {
        let placed = click_at(window, ClickButton::Left, x, y);
        click(&mut WindowsInput, &placed, &watched()).expect("sent");
        let messages = thread_mouse_messages();
        let &(first, _, _, place) = messages.first().expect("a message");
        let left = usize::try_from(MK_LBUTTON.0).expect("a small flag");
        assert_eq!(
            messages,
            [
                (first, WM_MOUSEMOVE, 0, place),
                (first, WM_LBUTTONDOWN, left, place),
                (first, WM_LBUTTONUP, 0, place)
            ]
        );
        (first, place)
    }

    #[test]
    fn a_window_answers_about_itself_and_its_children() {
        set_up();
        let window = TestWindow::new(PLAIN);
        let top = window.number();
        let lower = window.child(PLAIN, WS_VISIBLE, 50, 40);
        let upper = window.child(PLAIN, WS_DISABLED, 200, 40);
        let mut input = WindowsInput;
        let (x, y) = on_screen(60, 50);
        assert_eq!(
            input.own_place(top, x, y).expect("its place"),
            (x, y),
            "a window that follows its monitor's scale counts in the screen's pixels"
        );
        assert_eq!(input.client_size(top).expect("its size"), (400, 300));
        assert_eq!(input.client_point(top, x, y).expect("its place"), (60, 50));
        assert_eq!(input.client_size(lower).expect("its size"), (100, 80));
        assert_eq!(
            input.client_point(lower, x, y).expect("its place"),
            (10, 10)
        );
        assert_eq!(input.top_child(top).expect("looked"), Some(upper));
        assert_eq!(input.below(upper).expect("looked"), Some(lower));
        assert_eq!(input.below(lower).expect("looked"), None);
        assert_eq!(input.top_child(lower).expect("looked"), None);
        let (left, top_edge) = on_screen(50, 40);
        let (right, bottom) = on_screen(150, 120);
        assert_eq!(
            input.bounds(lower).expect("its bounds"),
            (left, top_edge, right, bottom)
        );
        let bits = |style: u32| (style & WS_VISIBLE.0 != 0, style & WS_DISABLED.0 != 0);
        assert_eq!(bits(input.style(lower).expect("its style")), (true, false));
        assert_eq!(bits(input.style(upper).expect("its style")), (false, true));
        let at = (
            i16::try_from(x).expect("near"),
            i16::try_from(y).expect("near"),
        );
        let wait = Duration::from_millis(200);
        let asked = |input: &mut WindowsInput, window| {
            low(input.hit_test(window, at, wait).expect("answered"))
        };
        assert_eq!(asked(&mut input, lower), HTCLIENT);
        assert_eq!(asked(&mut input, top), HTCLIENT);
        let label = window.child(w!("STATIC"), WS_VISIBLE, 50, 150);
        assert_eq!(
            asked(&mut input, label),
            u32::from_ne_bytes(HTTRANSPARENT.to_ne_bytes()),
            "the mouse goes through a label"
        );
        assert_eq!(input.class_style(top).expect("its class") & CS_DBLCLKS.0, 0);
        let other = TestWindow::new(DOUBLE_CLICKS);
        assert_eq!(
            input.class_style(other.number()).expect("its class") & CS_DBLCLKS.0,
            CS_DBLCLKS.0
        );
    }

    #[test]
    fn a_click_reaches_the_window_a_real_click_would() {
        set_up();
        let window = TestWindow::new(PLAIN);
        let top = Some(window.number());
        // An ordinary child gets its own click, in its own client area.
        let plain = window.child(PLAIN, WS_VISIBLE, 50, 40);
        assert_eq!(reached(&window, 60, 50), (Some(plain), 0x000A_000A));
        // The window itself gets a click beside its children.
        assert_eq!(reached(&window, 3, 2), (top, 0x0002_0003));
        // The mouse goes through a label to the control behind it.
        let behind_label = window.child(PLAIN, WS_VISIBLE, 200, 40);
        let _label = window.child(w!("STATIC"), WS_VISIBLE, 200, 40);
        assert_eq!(reached(&window, 210, 50), (Some(behind_label), 0x000A_000A));
        // With nothing behind a label, the window around it gets the click.
        let _alone = window.child(w!("STATIC"), WS_VISIBLE, 50, 150);
        assert_eq!(reached(&window, 60, 160), (top, 0x00A0_003C));
        // The mouse goes through a group box to the control inside it.
        let in_group = window.child(PLAIN, WS_VISIBLE, 200, 150);
        let group_box = WINDOW_STYLE(u32::try_from(BS_GROUPBOX).expect("a small style"));
        let _group = window.child(w!("BUTTON"), WS_VISIBLE | group_box, 200, 150);
        assert_eq!(reached(&window, 210, 160), (Some(in_group), 0x000A_000A));
        // A hidden child and a disabled one are not there for the mouse.
        let _hidden = window.child(PLAIN, WINDOW_STYLE(0), 300, 40);
        assert_eq!(reached(&window, 310, 50), (top, 0x0032_0136));
        let _disabled = window.child(PLAIN, WS_VISIBLE | WS_DISABLED, 300, 150);
        assert_eq!(reached(&window, 310, 160), (top, 0x00A0_0136));
    }

    #[test]
    fn each_click_posts_its_messages_as_the_window_s_class_is_told() {
        set_up();
        let left = usize::try_from(MK_LBUTTON.0).expect("a small flag");
        let right = usize::try_from(MK_RBUTTON.0).expect("a small flag");
        let place = 0x0032_003C_isize;
        for (class, button, messages) in [
            (
                PLAIN,
                ClickButton::Right,
                vec![
                    (WM_MOUSEMOVE, 0),
                    (WM_RBUTTONDOWN, right),
                    (WM_RBUTTONUP, 0),
                ],
            ),
            (
                PLAIN,
                ClickButton::Double,
                vec![
                    (WM_MOUSEMOVE, 0),
                    (WM_LBUTTONDOWN, left),
                    (WM_LBUTTONUP, 0),
                    (WM_LBUTTONDOWN, left),
                    (WM_LBUTTONUP, 0),
                ],
            ),
            (
                DOUBLE_CLICKS,
                ClickButton::Double,
                vec![
                    (WM_MOUSEMOVE, 0),
                    (WM_LBUTTONDOWN, left),
                    (WM_LBUTTONUP, 0),
                    (WM_LBUTTONDBLCLK, left),
                    (WM_LBUTTONUP, 0),
                ],
            ),
        ] {
            let window = TestWindow::new(class);
            let placed = click_at(&window, button, 60, 50);
            click(&mut WindowsInput, &placed, &watched()).expect("sent");
            let expected: Vec<_> = messages
                .into_iter()
                .map(|(number, held)| (Some(window.number()), number, held, place))
                .collect();
            assert_eq!(thread_mouse_messages(), expected, "{button:?}");
        }
    }

    #[test]
    fn a_window_that_does_not_follow_the_scale_gets_places_in_its_own_pixels() {
        set_up();
        // A window made by a thread that does not follow the monitor's
        // scale does not follow it either, so Windows stretches it on a
        // monitor that is scaled. On one that is not, its pixels are the
        // screen's, and this only shows that the calls work across the two
        // ways of counting.
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the call takes a predefined context and changes only this
        // thread's own setting.
        let ours = unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_UNAWARE) };
        assert!(!ours.0.is_null(), "Windows knows the context");
        let window = TestWindow::new(PLAIN);
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the context is the one this thread had.
        let theirs = unsafe { SetThreadDpiAwarenessContext(ours) };
        assert!(!theirs.0.is_null(), "Windows knows the context");
        // Where the window's own place (360, 248) is in the screen's pixels.
        let mut point = POINT { x: 360, y: 248 };
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `point` is a live local the call reads and writes, and the
        // window is this thread's live window.
        let converted =
            unsafe { LogicalToPhysicalPointForPerMonitorDPI(Some(window.0), &raw mut point) };
        assert!(converted.as_bool(), "converted");
        let mut input = WindowsInput;
        let top = window.number();
        assert_eq!(
            input.own_place(top, point.x, point.y).expect("its place"),
            (360, 248)
        );
        assert_eq!(input.client_size(top).expect("its size"), (400, 300));
        assert_eq!(
            input.client_point(top, 360, 248).expect("its place"),
            (60, 48)
        );
        assert_eq!(input.bounds(top).expect("its bounds"), (300, 200, 700, 500));
        let placed = PlacedClick {
            at: (point.x, point.y),
            ..click_at(&window, ClickButton::Left, 0, 0)
        };
        click(&mut input, &placed, &watched()).expect("sent");
        let left = usize::try_from(MK_LBUTTON.0).expect("a small flag");
        assert_eq!(
            thread_mouse_messages(),
            [
                (Some(top), WM_MOUSEMOVE, 0, 0x0030_003C),
                (Some(top), WM_LBUTTONDOWN, left, 0x0030_003C),
                (Some(top), WM_LBUTTONUP, 0, 0x0030_003C)
            ]
        );
    }

    #[test]
    fn a_message_arrives_as_it_was_posted() {
        set_up();
        let window = TestWindow::new(PLAIN);
        let top = window.number();
        let message = MouseMessage {
            event: MouseEvent::Move,
            at: (7, 9),
        };
        WindowsInput.post(top, message).expect("posted");
        assert_eq!(
            thread_mouse_messages(),
            [(Some(top), WM_MOUSEMOVE, 0, 0x0009_0007)]
        );
    }

    #[test]
    fn a_window_that_is_gone_gets_nothing() {
        set_up();
        let window = TestWindow::new(PLAIN);
        let gone = window.number();
        drop(window);
        let mut input = WindowsInput;
        let message = MouseMessage {
            event: MouseEvent::Move,
            at: (0, 0),
        };
        assert!(input.client_point(gone, 0, 0).is_err());
        assert!(input.client_size(gone).is_err());
        assert!(input.bounds(gone).is_err());
        assert_eq!(input.top_child(gone).expect("looked"), None);
        assert_eq!(input.below(gone).expect("looked"), None);
        assert_eq!(input.style(gone).expect("looked"), 0);
        assert_eq!(input.class_style(gone).expect("looked"), 0);
        let unanswered = input
            .hit_test(gone, (0, 0), Duration::from_millis(200))
            .expect_err("no such window");
        assert_ne!(unanswered.kind(), std::io::ErrorKind::TimedOut);
        assert!(
            unanswered
                .to_string()
                .starts_with("the window did not answer where the place is on it"),
            "{unanswered}"
        );
        let refused = input.post(gone, message).expect_err("no such window");
        assert_ne!(refused.kind(), std::io::ErrorKind::PermissionDenied);
        // No window at all would mean this thread's own queue.
        assert!(input.post(0, message).is_err());
        assert_eq!(thread_mouse_messages(), []);
    }

    #[test]
    fn a_wait_that_ran_out_and_a_refused_message_keep_their_kinds() {
        let of = |code: WIN32_ERROR| {
            let code = i32::try_from(code.0).expect("a small code");
            kind(&io::Error::from_raw_os_error(code))
        };
        assert_eq!(of(ERROR_TIMEOUT), io::ErrorKind::TimedOut);
        assert_eq!(of(ERROR_ACCESS_DENIED), io::ErrorKind::PermissionDenied);
        let other = of(ERROR_INVALID_WINDOW_HANDLE);
        assert_ne!(other, io::ErrorKind::TimedOut);
        assert_ne!(other, io::ErrorKind::PermissionDenied);
    }

    /// A window class whose windows take [`SLOW_ANSWER`] to say where a
    /// place is on them.
    const SLOW: PCWSTR = w!("AppyTappy.Test.Slow");

    /// How long a [`SLOW`] window takes to answer: longer than a click
    /// waits for all its windows.
    const SLOW_ANSWER: Duration = Duration::from_millis(900);

    /// The procedure of the [`SLOW`] class. It ends its thread's message
    /// loop when its window goes.
    extern "system" fn answer_slowly(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if message == WM_NCHITTEST {
            thread::sleep(SLOW_ANSWER);
        }
        if message == WM_DESTROY {
            #[expect(unsafe_code, reason = "a Win32 call")]
            // SAFETY: the call takes one integer and posts to this thread.
            unsafe {
                PostQuitMessage(0);
            }
        }
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the arguments are the ones Windows called this procedure
        // with.
        unsafe {
            DefWindowProcW(window, message, wparam, lparam)
        }
    }

    /// Makes a [`SLOW`] window on this thread, hands its number to `made`,
    /// and handles its messages until it is closed, which destroys it.
    fn keep_a_slow_window(made: &mpsc::Sender<u64>) {
        set_up();
        let class = WNDCLASSW {
            lpfnWndProc: Some(answer_slowly),
            hInstance: module(),
            lpszClassName: SLOW,
            ..WNDCLASSW::default()
        };
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `class` is a filled WNDCLASSW whose class name is a static
        // wide string and whose procedure lives for the whole program.
        let _atom = unsafe { RegisterClassW(&raw const class) };
        let window = TestWindow::new(SLOW);
        made.send(window.number()).expect("the test listens");
        loop {
            let mut message = MSG::default();
            #[expect(unsafe_code, reason = "a Win32 call")]
            // SAFETY: `message` is a live local the call writes.
            let got = unsafe { GetMessageW(&raw mut message, None, 0, 0) };
            // Zero ends the loop, and so does the call failing.
            if got.0 <= 0 {
                break;
            }
            #[expect(unsafe_code, reason = "a Win32 call")]
            // SAFETY: `message` is the one the call above filled.
            unsafe {
                DispatchMessageW(&raw const message);
            }
        }
    }

    #[test]
    fn a_window_that_does_not_answer_in_time_gets_no_click() {
        set_up();
        let (made, windows) = mpsc::channel();
        let owner = thread::spawn(move || keep_a_slow_window(&made));
        let slow = windows.recv().expect("a window");
        let placed = PlacedClick {
            window: slow,
            process: ProcessId::new(std::process::id()),
            app: "Test".to_owned(),
            at: on_screen(60, 50),
            button: ClickButton::Left,
            delivery: ClickDelivery::Message,
        };
        let started = Instant::now();
        let sent = click(&mut WindowsInput, &placed, &watched());
        let took = started.elapsed();
        // Closing the window ends its thread once it has answered.
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the call copies a number and two integers into the
        // window's message queue.
        unsafe {
            PostMessageW(
                Some(handle(slow).expect("a handle")),
                WM_CLOSE,
                WPARAM(0),
                LPARAM(0),
            )
        }
        .expect("asked the window to close");
        owner.join().expect("the window's thread ended");
        assert!(
            matches!(&sent, Err(ActionError::TooSlow(app)) if app == "Test"),
            "{sent:?}"
        );
        assert!(took < SLOW_ANSWER, "the click waited {took:?}");
        assert_eq!(thread_mouse_messages(), []);
    }
}
