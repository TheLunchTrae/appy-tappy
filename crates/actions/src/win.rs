//! The Windows adapter: each function turns Windows API calls into a typed
//! value or an error and makes no decisions of its own.
//!
//! Media sessions come from the Windows Runtime media transport controls, which Spotify
//! and browsers publish without any app-specific API. Volumes come from Core
//! Audio on the default playback device. Naming an audio session's app takes
//! a process snapshot, which never opens a process, so no action opens a
//! game's process.

use std::collections::BTreeMap;
use std::io;
use std::marker::PhantomData;
use std::mem::size_of;
use std::ptr;

use appytappy_config::action::{AppName, MediaCommand, MediaTarget, VolumeLevel, VolumeTarget};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession, GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus,
};
use windows::Win32::Foundation::{CloseHandle, ERROR_NO_MORE_FILES, HANDLE};
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{
    IAudioSessionControl2, IAudioSessionManager2, IMMDevice, IMMDeviceEnumerator,
    ISimpleAudioVolume, MMDeviceEnumerator, eConsole, eRender,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::core::{GUID, Interface};

use crate::{Controls, MediaSession};

/// This thread's membership in the COM multithreaded apartment, which Core
/// Audio needs. It ends on drop, on the thread that joined, because the type
/// cannot move between threads.
#[derive(Debug)]
struct Apartment(PhantomData<*const ()>);

impl Apartment {
    fn join() -> io::Result<Self> {
        #[expect(unsafe_code, reason = "a COM call")]
        // SAFETY: the call takes no pointers; a success, including S_FALSE for a
        // thread that already joined, is balanced by CoUninitialize on drop.
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok()?;
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

/// The Windows implementation of [`Controls`].
///
/// It joins the calling thread to the COM multithreaded apartment, so it is
/// created and used on the action worker's thread and cannot move to
/// another.
#[derive(Debug)]
pub struct WindowsControls {
    media: GlobalSystemMediaTransportControlsSessionManager,
    devices: IMMDeviceEnumerator,
    // Dropped last, after the COM objects above.
    _apartment: Apartment,
}

impl WindowsControls {
    /// Connects to the media session manager and the audio devices.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    pub fn new() -> io::Result<Self> {
        let apartment = Apartment::join()?;
        let media = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?.join()?;
        #[expect(unsafe_code, reason = "a COM call")]
        // SAFETY: the class ID is a static GUID, and the result is a new owned interface.
        let devices = unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }?;
        Ok(Self {
            media,
            devices,
            _apartment: apartment,
        })
    }

    fn find_session(
        &self,
        target: &MediaTarget,
    ) -> io::Result<Option<GlobalSystemMediaTransportControlsSession>> {
        match target {
            // With no current session, the call returns null, which arrives as an
            // error carrying a success code.
            MediaTarget::Current => match self.media.GetCurrentSession() {
                Ok(session) => Ok(Some(session)),
                Err(error) if error.code().is_ok() => Ok(None),
                Err(error) => Err(error.into()),
            },
            MediaTarget::App(app) => self.app_session(app),
        }
    }

    fn app_session(
        &self,
        app: &AppName,
    ) -> io::Result<Option<GlobalSystemMediaTransportControlsSession>> {
        for session in &self.media.GetSessions()? {
            if app.matches(&session.SourceAppUserModelId()?.to_string_lossy()) {
                return Ok(Some(session));
            }
        }
        Ok(None)
    }

    fn default_device(&self) -> io::Result<IMMDevice> {
        #[expect(unsafe_code, reason = "a COM call")]
        // SAFETY: the call takes two enum values and returns a new owned interface.
        let device = unsafe { self.devices.GetDefaultAudioEndpoint(eRender, eConsole) }?;
        Ok(device)
    }

    /// The volume controls of every audio session of `app` on the default
    /// playback device.
    fn app_volumes(&self, app: &AppName) -> io::Result<Vec<ISimpleAudioVolume>> {
        let device = self.default_device()?;
        #[expect(unsafe_code, reason = "a COM call")]
        // SAFETY: the call activates an interface the type parameter names, with no
        // activation parameters.
        let manager: IAudioSessionManager2 = unsafe { device.Activate(CLSCTX_ALL, None) }?;
        #[expect(unsafe_code, reason = "a COM call")]
        // SAFETY: the call takes no arguments and returns a new owned interface.
        let sessions = unsafe { manager.GetSessionEnumerator() }?;
        #[expect(unsafe_code, reason = "a COM call")]
        // SAFETY: the call takes no arguments.
        let count = unsafe { sessions.GetCount() }?;
        let executables = executables()?;
        let mut volumes = Vec::new();
        for index in 0..count {
            #[expect(unsafe_code, reason = "a COM call")]
            // SAFETY: `index` is below the count the enumerator reported.
            let session = unsafe { sessions.GetSession(index) }?;
            let control: IAudioSessionControl2 = session.cast()?;
            #[expect(unsafe_code, reason = "a COM call")]
            // SAFETY: the call takes no arguments. A session that spans several
            // processes reports no single ID, which names no app.
            let process = unsafe { control.GetProcessId() }?;
            if executables
                .get(&process)
                .is_some_and(|executable| app.matches(executable))
            {
                volumes.push(session.cast()?);
            }
        }
        Ok(volumes)
    }

    fn system_volume(&self) -> io::Result<IAudioEndpointVolume> {
        let device = self.default_device()?;
        #[expect(unsafe_code, reason = "a COM call")]
        // SAFETY: the call activates an interface the type parameter names, with no
        // activation parameters.
        let volume = unsafe { device.Activate(CLSCTX_ALL, None) }?;
        Ok(volume)
    }
}

impl Controls for WindowsControls {
    fn media_session(&mut self, target: &MediaTarget) -> io::Result<Option<MediaSession>> {
        let Some(session) = self.find_session(target)? else {
            return Ok(None);
        };
        let status = session.GetPlaybackInfo()?.PlaybackStatus()?;
        Ok(Some(MediaSession {
            app: session.SourceAppUserModelId()?.to_string_lossy(),
            playing: status == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing,
        }))
    }

    fn media_command(&mut self, app: &str, command: MediaCommand) -> io::Result<bool> {
        let mut found = None;
        for session in &self.media.GetSessions()? {
            if session.SourceAppUserModelId()?.to_string_lossy() == app {
                found = Some(session);
                break;
            }
        }
        let Some(session) = found else {
            return Ok(false);
        };
        let request = match command {
            MediaCommand::Play => session.TryPlayAsync()?,
            MediaCommand::Pause => session.TryPauseAsync()?,
            MediaCommand::Next => session.TrySkipNextAsync()?,
            MediaCommand::Previous => session.TrySkipPreviousAsync()?,
        };
        Ok(request.join()?)
    }

    fn volume(&mut self, target: &VolumeTarget) -> io::Result<Option<VolumeLevel>> {
        let scalar = match target {
            VolumeTarget::System => {
                let volume = self.system_volume()?;
                #[expect(unsafe_code, reason = "a COM call")]
                // SAFETY: the call takes no arguments.
                let scalar = unsafe { volume.GetMasterVolumeLevelScalar() }?;
                scalar
            },
            VolumeTarget::App(app) => {
                let Some(volume) = self.app_volumes(app)?.into_iter().next() else {
                    return Ok(None);
                };
                #[expect(unsafe_code, reason = "a COM call")]
                // SAFETY: the call takes no arguments.
                let scalar = unsafe { volume.GetMasterVolume() }?;
                scalar
            },
        };
        VolumeLevel::from_scalar(scalar)
            .map(Some)
            .ok_or_else(|| io::Error::other(format!("Windows reported volume {scalar}")))
    }

    fn set_volume(&mut self, target: &VolumeTarget, level: VolumeLevel) -> io::Result<bool> {
        match target {
            VolumeTarget::System => {
                let volume = self.system_volume()?;
                #[expect(unsafe_code, reason = "a COM call")]
                // SAFETY: the level is in 0 to 1, and a null event context is allowed.
                unsafe { volume.SetMasterVolumeLevelScalar(level.scalar(), ptr::null::<GUID>()) }?;
                Ok(true)
            },
            VolumeTarget::App(app) => {
                let volumes = self.app_volumes(app)?;
                for volume in &volumes {
                    #[expect(unsafe_code, reason = "a COM call")]
                    // SAFETY: the level is in 0 to 1, and a null event context is allowed.
                    unsafe { volume.SetMasterVolume(level.scalar(), ptr::null::<GUID>()) }?;
                }
                Ok(!volumes.is_empty())
            },
        }
    }
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
