//! The Windows adapter: each function turns one Windows API call into a typed
//! value or an error and makes no decisions of its own.
//!
//! Listing processes and windows, and reading a process's executable
//! build, never open a process.

use std::io;
use std::mem::size_of;
use std::ptr;

use appytappy_config::app_file::BuildDef;
use appytappy_config::process::ProcessId;
use windows::Win32::Foundation::{CloseHandle, ERROR_NO_MORE_FILES, HANDLE, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::Graphics::Gdi::{CLR_INVALID, GetDC, GetPixel, ReleaseDC};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowInfo, GetWindowRect, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, WINDOWINFO,
};
use windows::core::BOOL;

use crate::screen::{Size, content_on_screen};
use crate::system::{Bounds, ProcessEntry, Rgb, System, WindowEntry, WindowPlace};

mod build;
mod capture;

pub use capture::WindowsCaptureSession;

/// The Windows implementation of [`System`].
#[derive(Debug)]
pub struct WindowsSystem(());

impl WindowsSystem {
    /// Prepares the adapter. It makes the process aware of per-monitor DPI,
    /// so window bounds and screen pixels share physical pixel coordinates.
    #[must_use]
    pub fn new() -> Self {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a failure means a manifest already set the awareness, which stays in effect"
        )]
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the call takes a predefined constant and no pointers.
        let _ =
            unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        Self(())
    }
}

impl Default for WindowsSystem {
    fn default() -> Self {
        Self::new()
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

fn utf16_to_string(buffer: &[u16]) -> String {
    let end = buffer
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(buffer.get(..end).unwrap_or_default())
}

fn struct_size<T>() -> io::Result<u32> {
    u32::try_from(size_of::<T>()).map_err(io::Error::other)
}

/// The window handle that `window`, a handle as a number, stands for.
fn window_handle(window: u64) -> io::Result<HWND> {
    Ok(HWND(ptr::without_provenance_mut(
        usize::try_from(window).map_err(io::Error::other)?,
    )))
}

/// The bounds of `window`'s frame as the screen shows it, without the
/// invisible borders around it: what a capture of it takes in.
fn frame_on_screen(window: HWND) -> io::Result<Bounds> {
    let mut rect = RECT::default();
    let size = struct_size::<RECT>()?;
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: `rect` is a live local of the size passed, which the call
    // writes; it fails without harm when the window is gone.
    unsafe {
        DwmGetWindowAttribute(
            window,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&raw mut rect).cast(),
            size,
        )
    }?;
    Ok(bounds_of(rect))
}

/// Where `window`'s client area lies on the screen, the part the
/// application draws in. The system gives it in screen pixels whole, so a
/// window that the system stretches because it does not follow the
/// monitor's scale, or that lays itself out right to left, still gets its
/// whole client area.
fn client_on_screen(window: HWND) -> io::Result<Bounds> {
    let mut info = WINDOWINFO {
        cbSize: struct_size::<WINDOWINFO>()?,
        ..WINDOWINFO::default()
    };
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: `info` is a live local whose `cbSize` gives its size, which
    // the call writes; it fails without harm when the window is gone.
    unsafe { GetWindowInfo(window, &raw mut info) }?;
    Ok(bounds_of(info.rcClient))
}

/// `rect` as bounds.
const fn bounds_of(rect: RECT) -> Bounds {
    Bounds {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}

/// Takes a Toolhelp snapshot of every running process.
fn process_snapshot() -> io::Result<OwnedHandle> {
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: the call takes flags and a process ID and returns a new handle.
    let handle = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }?;
    Ok(OwnedHandle(handle))
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

/// The collector [`EnumWindows`] calls for each top-level window.
extern "system" fn collect_window(window: HWND, collected: LPARAM) -> BOOL {
    let handles = ptr::with_exposed_provenance_mut::<Vec<HWND>>(collected.0.cast_unsigned());
    #[expect(
        unsafe_code,
        reason = "dereferencing the pointer EnumWindows passes back"
    )]
    // SAFETY: `collected` is the `&mut Vec<HWND>` that `windows` passed to EnumWindows,
    // which calls back only while that borrow is alive and on the same thread.
    let handles = unsafe { &mut *handles };
    handles.push(window);
    BOOL::from(true)
}

fn window_entry(window: HWND) -> WindowEntry {
    let mut process = 0;
    let mut title = [0_u16; 512];
    let mut class = [0_u16; 256];
    let mut rect = RECT::default();
    #[expect(unsafe_code, reason = "Win32 calls")]
    // SAFETY: `window` came from EnumWindows; every out-parameter is a live local
    // buffer, and the slice calls pass their own lengths.
    let (visible, rect_read) = unsafe {
        GetWindowThreadProcessId(window, Some(&raw mut process));
        GetWindowTextW(window, &mut title);
        GetClassNameW(window, &mut class);
        (
            IsWindowVisible(window).as_bool(),
            GetWindowRect(window, &raw mut rect).is_ok(),
        )
    };
    let bounds = if rect_read {
        Bounds {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        }
    } else {
        Bounds {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        }
    };
    WindowEntry {
        handle: window.0.addr() as u64,
        process: ProcessId::new(process),
        title: utf16_to_string(&title),
        class: utf16_to_string(&class),
        visible,
        bounds,
    }
}

impl System for WindowsSystem {
    fn processes(&self) -> io::Result<Vec<ProcessEntry>> {
        let snapshot = process_snapshot()?;
        let mut entry = PROCESSENTRY32W {
            dwSize: struct_size::<PROCESSENTRY32W>()?,
            ..Default::default()
        };
        let mut processes = Vec::new();
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `entry` is a PROCESSENTRY32W whose dwSize is set, as the call requires.
        let mut found = walk_end(unsafe { Process32FirstW(snapshot.0, &raw mut entry) })?;
        while found {
            processes.push(ProcessEntry {
                id: ProcessId::new(entry.th32ProcessID),
                parent: ProcessId::new(entry.th32ParentProcessID),
                executable: utf16_to_string(&entry.szExeFile),
                threads: entry.cntThreads,
            });
            #[expect(unsafe_code, reason = "a Win32 call")]
            // SAFETY: as for Process32FirstW.
            let next = unsafe { Process32NextW(snapshot.0, &raw mut entry) };
            found = walk_end(next)?;
        }
        Ok(processes)
    }

    fn windows(&self) -> io::Result<Vec<WindowEntry>> {
        let mut handles: Vec<HWND> = Vec::new();
        let collected = LPARAM(
            ptr::from_mut(&mut handles)
                .expose_provenance()
                .cast_signed(),
        );
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `collect_window` reads `collected` only as the `handles` vector,
        // which outlives this call.
        unsafe { EnumWindows(Some(collect_window), collected) }?;
        Ok(handles.into_iter().map(window_entry).collect())
    }

    fn window_place(&self, window: u64) -> io::Result<Option<WindowPlace>> {
        let handle = window_handle(window)?;
        let mut rect = RECT::default();
        #[expect(unsafe_code, reason = "Win32 calls")]
        // SAFETY: each call only reads the state of the window `handle` names,
        // and fails without harm when no such window exists; `rect` is a live
        // local the call writes.
        let (exists, visible, minimized, rect_read, front) = unsafe {
            (
                IsWindow(Some(handle)).as_bool(),
                IsWindowVisible(handle).as_bool(),
                IsIconic(handle).as_bool(),
                GetWindowRect(handle, &raw mut rect).is_ok(),
                GetForegroundWindow() == handle,
            )
        };
        if !exists || !rect_read {
            return Ok(None);
        }
        Ok(Some(WindowPlace {
            bounds: bounds_of(rect),
            shown: visible && !minimized,
            front,
        }))
    }

    fn content_size(&self, window: u64) -> io::Result<Option<Size>> {
        let handle = window_handle(window)?;
        #[expect(unsafe_code, reason = "Win32 calls")]
        // SAFETY: both calls only read the state of the window `handle`
        // names and answer false when no such window exists.
        let (exists, minimized) =
            unsafe { (IsWindow(Some(handle)).as_bool(), IsIconic(handle).as_bool()) };
        // A minimized window's bounds are its small placeholder, not the size
        // it will draw at once restored.
        if !exists || minimized {
            return Ok(None);
        }
        let content = content_on_screen(frame_on_screen(handle)?, client_on_screen(handle).ok());
        let side = |from: i32, to: i32| u32::try_from(to.saturating_sub(from)).unwrap_or_default();
        Ok(Some(Size {
            width: side(content.left, content.right),
            height: side(content.top, content.bottom),
        }))
    }

    fn content_bounds(&self, window: u64) -> io::Result<Option<Bounds>> {
        let Some(place) = self.window_place(window)? else {
            return Ok(None);
        };
        let handle = window_handle(window)?;
        // Without the desktop window manager's bounds, the window's own are
        // the best there is.
        let frame = frame_on_screen(handle).unwrap_or(place.bounds);
        Ok(Some(content_on_screen(
            frame,
            client_on_screen(handle).ok(),
        )))
    }

    fn executable_build(&self, process: ProcessId) -> io::Result<BuildDef> {
        build::executable_build(process)
    }

    fn screen_pixel(&self, x: i32, y: i32) -> io::Result<Rgb> {
        #[expect(unsafe_code, reason = "Win32 calls")]
        // SAFETY: the screen device context is released before the block ends and
        // used for nothing else.
        let color = unsafe {
            let screen = GetDC(None);
            if screen.is_invalid() {
                return Err(io::Error::other("cannot get the screen's device context"));
            }
            let color = GetPixel(screen, x, y);
            ReleaseDC(None, screen);
            color
        };
        if color.0 == CLR_INVALID {
            return Err(io::Error::other(format!(
                "pixel ({x}, {y}) is outside the screen"
            )));
        }
        let [red, green, blue, _] = color.0.to_le_bytes();
        Ok(Rgb { red, green, blue })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_this_process_without_opening_it() {
        let system = WindowsSystem::new();
        let own = ProcessId::new(std::process::id());
        let entry = system
            .processes()
            .unwrap()
            .into_iter()
            .find(|entry| entry.id == own)
            .expect("the snapshot lists this process");
        let exe = std::env::current_exe().unwrap();
        let name = exe.file_name().unwrap().to_string_lossy();
        assert!(
            entry.executable.eq_ignore_ascii_case(&name),
            "{} is not {name}",
            entry.executable
        );
        assert_ne!(entry.parent, own, "a process is not its own parent");
        assert!(entry.threads > 0, "a running process has a thread");
    }

    #[test]
    fn enumerates_windows_and_reads_screen_pixels() {
        let system = WindowsSystem::new();
        system.windows().unwrap();
        system.screen_pixel(0, 0).unwrap();
        assert!(system.screen_pixel(-1_000_000, -1_000_000).is_err());
    }
}
