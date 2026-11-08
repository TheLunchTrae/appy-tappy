//! The Windows adapter: each function turns Windows API calls into a typed
//! value or an error and makes no decisions of its own.
//!
//! Media sessions come from the Windows Runtime media transport controls,
//! which music players and browsers publish without any app-specific API.
//! Volumes come from Core Audio on the default playback device. Naming an
//! audio session's app takes a process snapshot, which never opens a process,
//! so no action opens a watched app's process. Key presses go through
//! `SendInput`, which types into whichever window has the keyboard focus,
//! after asking which process that window belongs to. Opening a program,
//! file, or link goes through `ShellExecuteExW`, the call behind File
//! Explorer's open, on a short-lived thread of its own, so a slow program
//! cannot hold up the action worker for longer than [`OPEN_TIMEOUT`]. A sound
//! plays through `PlaySoundW`, which returns once the sound starts.
//!
//! The media session manager may not be ready yet, such as right after
//! sign-in, so the controls start without it and ask for it again when a
//! media action needs it, waiting longer after each failure. A volume
//! control that stops working, such as after its device was unplugged and
//! plugged back in, is found again by its ID; an audio session that is not
//! found again ended for good.

use std::collections::BTreeMap;
use std::io;
use std::marker::PhantomData;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use appytappy_config::action::{Key, MediaCommand, Modifier, OpenTarget, VolumeLevel};
use appytappy_config::anti_cheat::ProcessId;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession, GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus,
};
use windows::Win32::Foundation::{CloseHandle, ERROR_NO_MORE_FILES, ERROR_NOT_FOUND, HANDLE};
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
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC, MapVirtualKeyW, SendInput,
    VIRTUAL_KEY, VK_BACK, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_F1, VK_HOME,
    VK_INSERT, VK_LCONTROL, VK_LEFT, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_NEXT, VK_PRIOR,
    VK_RETURN, VK_RIGHT, VK_RWIN, VK_SHIFT, VK_SPACE, VK_TAB, VK_UP,
};
use windows::Win32::UI::Shell::{
    SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, SW_SHOWNOACTIVATE,
};
use windows::core::{GUID, IUnknown, Interface, PCWSTR, PWSTR, RuntimeType};
use windows_future::{AsyncStatus, IAsyncOperation};

use crate::{Backoff, Controls, MediaSession, Stroke, Volume};

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
    // Dropped last, after the COM objects above.
    _apartment: Apartment,
}

impl WindowsControls {
    /// Connects to the audio devices, and to the media session manager if
    /// it is ready.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    pub fn new() -> io::Result<Self> {
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
        let executables = executables()?;
        let mut volumes = Vec::new();
        for (id, details) in sessions(&device)? {
            #[expect(unsafe_code, reason = "a COM call")]
            // SAFETY: the call takes no arguments. For a session that spans several
            // processes it still succeeds and names the process that created it.
            let process = unsafe { details.GetProcessId() }?;
            let Some(executable) = executables.get(&process) else {
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

    fn hold(&mut self, time: Duration) {
        thread::sleep(time);
    }

    fn open(&mut self, target: &OpenTarget) -> io::Result<()> {
        let file = wide(target.as_str().encode_utf16());
        let (done, outcome) = mpsc::channel();
        // The thread is not joined: after a timeout it finishes on its own,
        // and its answer goes nowhere.
        thread::Builder::new()
            .name("open".to_owned())
            .spawn(move || {
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "after a timeout nobody waits for the answer"
                )]
                let _ = done.send(shell_open(&file));
            })?;
        match outcome.recv_timeout(OPEN_TIMEOUT) {
            Ok(opened) => opened,
            Err(RecvTimeoutError::Timeout) => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "opening it did not finish within 2 seconds, so it may still open",
            )),
            Err(RecvTimeoutError::Disconnected) => {
                Err(io::Error::other("the thread that opens it stopped"))
            },
        }
    }

    fn play_sound(&mut self, file: &Path) -> io::Result<bool> {
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
        Ok(started.as_bool())
    }
}

/// UTF-16 `units` with the nul that ends a Windows string.
fn wide(units: impl Iterator<Item = u16>) -> Vec<u16> {
    units.chain([0]).collect()
}

/// Opens `file`, a nul-terminated UTF-16 path or link, with its default
/// action and no arguments, showing no error dialog. It asks the opened
/// window not to take the keyboard focus, though a program may take it
/// anyway. It runs on a thread of its own in a single-threaded apartment,
/// as the shell asks of its callers.
fn shell_open(file: &[u16]) -> io::Result<()> {
    let _apartment = Apartment::join(COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE)?;
    let mut info = SHELLEXECUTEINFOW {
        cbSize: u32::try_from(size_of::<SHELLEXECUTEINFOW>()).map_err(io::Error::other)?,
        // The thread ends after the call, so the call must finish first.
        fMask: SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpFile: PCWSTR(file.as_ptr()),
        nShow: SW_SHOWNOACTIVATE.0,
        ..Default::default()
    };
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: `info` is a SHELLEXECUTEINFOW whose cbSize is set and whose
    // file points to a nul-terminated string that outlives the call; every
    // other pointer is null, which the call allows.
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

/// An owned kernel handle, closed on drop.
#[derive(Debug)]
struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "closing a handle this value owns cannot be retried, and Drop has no caller to report to"
        )]
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the handle came from a successful snapshot call and is closed once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Every running process's executable name, by process ID, from a snapshot
/// that opens no process.
fn executables() -> io::Result<BTreeMap<u32, String>> {
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: the call takes flags and a process ID and returns a new handle.
    let snapshot = OwnedHandle(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }?);
    let mut entry = PROCESSENTRY32W {
        dwSize: u32::try_from(size_of::<PROCESSENTRY32W>()).map_err(io::Error::other)?,
        ..Default::default()
    };
    let mut executables = BTreeMap::new();
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: `entry` is a PROCESSENTRY32W whose dwSize is set, as the call requires.
    let mut found = walk_end(unsafe { Process32FirstW(snapshot.0, &raw mut entry) })?;
    while found {
        let end = entry
            .szExeFile
            .iter()
            .position(|&unit| unit == 0)
            .unwrap_or(entry.szExeFile.len());
        let name = String::from_utf16_lossy(entry.szExeFile.get(..end).unwrap_or_default());
        executables.insert(entry.th32ProcessID, name);
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: as for Process32FirstW.
        let next = unsafe { Process32NextW(snapshot.0, &raw mut entry) };
        found = walk_end(next)?;
    }
    Ok(executables)
}

/// Ends a Toolhelp walk: running out of entries is the normal end, and any
/// other failure is an error.
fn walk_end(result: windows::core::Result<()>) -> io::Result<bool> {
    match result {
        Ok(()) => Ok(true),
        Err(error) if error.code() == ERROR_NO_MORE_FILES.to_hresult() => Ok(false),
        Err(error) => Err(error.into()),
    }
}
