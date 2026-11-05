//! The Windows adapter: each function turns one Windows API call into a typed
//! value or an error and makes no decisions of its own.
//!
//! Listing processes and windows never opens a process. Only
//! [`WindowsSystem::open`] does, it needs a permit, and it asks for read
//! access only: no rights to write memory, create threads, or inject input.

use std::ffi::c_void;
use std::io;
use std::mem::size_of;
use std::ptr;

use appytappy_config::anti_cheat::{ProcessAccessPermit, ProcessId};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_BAD_LENGTH, ERROR_INVALID_PARAMETER, ERROR_NO_MORE_FILES, HANDLE, HWND,
    LPARAM, RECT,
};
use windows::Win32::Graphics::Gdi::{CLR_INVALID, GetDC, GetPixel, ReleaseDC};
use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, MODULEENTRY32W, Module32FirstW, Module32NextW, PROCESSENTRY32W,
    Process32FirstW, Process32NextW, TH32CS_SNAPMODULE, TH32CS_SNAPMODULE32, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Memory::{
    MEM_COMMIT, MEM_MAPPED, MEMORY_BASIC_INFORMATION, PAGE_EXECUTE_READWRITE,
    PAGE_EXECUTE_WRITECOPY, PAGE_GUARD, PAGE_READWRITE, PAGE_WRITECOPY, VirtualQueryEx,
};
use windows::Win32::System::Threading::{
    IsWow64Process, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
    QueryFullProcessImageNameW,
};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, IDYES,
    IsWindowVisible, MB_DEFBUTTON2, MB_ICONWARNING, MB_SETFOREGROUND, MB_TOPMOST, MB_YESNO,
    MessageBoxW,
};
use windows::core::{BOOL, HSTRING, PWSTR};

use crate::memory::{Module, PointerWidth, ProcessMemory, Region};
use crate::system::{AccessAnswer, Bounds, ProcessEntry, Rgb, System, WindowEntry};

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
        // SAFETY: the handle came from a successful open or snapshot call and is closed once.
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

/// Takes a Toolhelp snapshot. A module snapshot fails with
/// `ERROR_BAD_LENGTH` while the process is loading a module, so that error is
/// retried a few times.
fn snapshot(
    flags: windows::Win32::System::Diagnostics::ToolHelp::CREATE_TOOLHELP_SNAPSHOT_FLAGS,
    process: u32,
) -> io::Result<OwnedHandle> {
    let mut attempts = 0_u8;
    loop {
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the call takes flags and a process ID and returns a new handle.
        let taken = unsafe { CreateToolhelp32Snapshot(flags, process) };
        match taken {
            Ok(handle) => return Ok(OwnedHandle(handle)),
            Err(error) if error.code() == ERROR_BAD_LENGTH.to_hresult() && attempts < 4 => {
                attempts = attempts.saturating_add(1);
            },
            Err(error) => return Err(error.into()),
        }
    }
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

/// An opened process, with read rights only.
#[derive(Debug)]
pub struct WindowsProcess {
    handle: OwnedHandle,
    id: ProcessId,
    width: PointerWidth,
}

impl ProcessMemory for WindowsProcess {
    fn regions(&self) -> io::Result<Vec<Region>> {
        let mut regions = Vec::new();
        let mut address: usize = 0;
        let length = size_of::<MEMORY_BASIC_INFORMATION>();
        loop {
            let mut info = MEMORY_BASIC_INFORMATION::default();
            #[expect(unsafe_code, reason = "a Win32 call")]
            // SAFETY: `info` is a writable MEMORY_BASIC_INFORMATION of `length` bytes,
            // and the address is only a query, never dereferenced here.
            let written = unsafe {
                VirtualQueryEx(
                    self.handle.0,
                    Some(ptr::without_provenance(address)),
                    &raw mut info,
                    length,
                )
            };
            if written == 0 {
                // Querying past the last region fails with ERROR_INVALID_PARAMETER.
                let error = windows::core::Error::from_thread();
                if error.code() == ERROR_INVALID_PARAMETER.to_hresult() {
                    break;
                }
                return Err(error.into());
            }
            let base = info.BaseAddress.addr();
            let protect = info.Protect.0;
            let writable = [
                PAGE_READWRITE,
                PAGE_WRITECOPY,
                PAGE_EXECUTE_READWRITE,
                PAGE_EXECUTE_WRITECOPY,
            ]
            .iter()
            .any(|flag| protect & 0xff == flag.0);
            if info.State == MEM_COMMIT
                && info.Type != MEM_MAPPED
                && writable
                && protect & PAGE_GUARD.0 == 0
            {
                regions.push(Region {
                    base: base as u64,
                    size: info.RegionSize as u64,
                });
            }
            match base.checked_add(info.RegionSize) {
                Some(next) if next > address => address = next,
                _ => break,
            }
        }
        Ok(regions)
    }

    fn read(&self, address: u64, buffer: &mut [u8]) -> io::Result<()> {
        let address = usize::try_from(address).map_err(io::Error::other)?;
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the destination is `buffer`, writable for its whole length; the
        // source address is in the other process and Windows checks it.
        unsafe {
            ReadProcessMemory(
                self.handle.0,
                ptr::without_provenance::<c_void>(address),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                None,
            )
        }?;
        Ok(())
    }

    fn modules(&self) -> io::Result<Vec<Module>> {
        let snapshot = snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, self.id.raw())?;
        let mut entry = MODULEENTRY32W {
            dwSize: struct_size::<MODULEENTRY32W>()?,
            ..Default::default()
        };
        let mut modules = Vec::new();
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `entry` is a MODULEENTRY32W whose dwSize is set, as the call requires.
        let mut found = walk_end(unsafe { Module32FirstW(snapshot.0, &raw mut entry) })?;
        while found {
            modules.push(Module {
                name: utf16_to_string(&entry.szModule),
                base: entry.modBaseAddr.addr() as u64,
                size: u64::from(entry.modBaseSize),
            });
            #[expect(unsafe_code, reason = "a Win32 call")]
            // SAFETY: as for Module32FirstW.
            let next = unsafe { Module32NextW(snapshot.0, &raw mut entry) };
            found = walk_end(next)?;
        }
        Ok(modules)
    }

    fn pointer_width(&self) -> PointerWidth {
        self.width
    }

    fn image_name(&self) -> io::Result<String> {
        let mut path = vec![0_u16; 32_768];
        let mut length = u32::try_from(path.len()).map_err(io::Error::other)?;
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `path` is writable for `length` UTF-16 units, and the call writes
        // at most that many and stores the count it wrote in `length`.
        unsafe {
            QueryFullProcessImageNameW(
                self.handle.0,
                PROCESS_NAME_WIN32,
                PWSTR(path.as_mut_ptr()),
                &raw mut length,
            )
        }?;
        let written = usize::try_from(length).map_err(io::Error::other)?;
        let path = String::from_utf16_lossy(path.get(..written).unwrap_or_default());
        Ok(path.rsplit('\\').next().unwrap_or_default().to_owned())
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
    type Process = WindowsProcess;

    fn processes(&self) -> io::Result<Vec<ProcessEntry>> {
        let snapshot = snapshot(TH32CS_SNAPPROCESS, 0)?;
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

    fn ask_memory_access(
        &self,
        process: &ProcessEntry,
        anti_cheat: &[&str],
    ) -> io::Result<AccessAnswer> {
        let warning = if anti_cheat.is_empty() {
            String::new()
        } else {
            format!(
                "Anti-cheat software is running right now: {}. Choose No unless you are sure \
                 it does not protect this game.\n\n",
                anti_cheat.join(", ")
            )
        };
        let text = format!(
            "An AI agent connected to AppyTappy wants to read the memory of {} (process {}).\n\n\
             Only allow this for a game that has no anti-cheat. Reading the memory of a game \
             that anti-cheat protects can get your account banned.\n\n\
             {warning}AppyTappy only reads memory. It never changes the game.\n\nAllow it?",
            process.executable, process.id
        );
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: both strings are live HSTRINGs for the duration of the call.
        let answer = unsafe {
            MessageBoxW(
                None,
                &HSTRING::from(text),
                &HSTRING::from("AppyTappy: allow memory access?"),
                MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2 | MB_TOPMOST | MB_SETFOREGROUND,
            )
        };
        match answer.0 {
            0 => Err(windows::core::Error::from_thread().into()),
            _ if answer == IDYES => Ok(AccessAnswer::Allowed),
            _ => Ok(AccessAnswer::Refused),
        }
    }

    fn open(&self, permit: &ProcessAccessPermit) -> io::Result<WindowsProcess> {
        let id = permit.process();
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: the call takes access flags and a process ID and returns a new handle.
        let handle =
            unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, id.raw()) }?;
        let handle = OwnedHandle(handle);
        let mut wow64 = BOOL::default();
        #[expect(unsafe_code, reason = "a Win32 call")]
        // SAFETY: `handle` is open with query rights, and `wow64` is a live BOOL.
        unsafe { IsWow64Process(handle.0, &raw mut wow64) }?;
        let width = if wow64.as_bool() || size_of::<usize>() == 4 {
            PointerWidth::Bits32
        } else {
            PointerWidth::Bits64
        };
        Ok(WindowsProcess { handle, id, width })
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
    use appytappy_config::anti_cheat::AntiCheatClass;

    use super::*;
    use crate::memory::{Number, ValueType, follow_pointer_chain};

    fn own_process(system: &WindowsSystem) -> WindowsProcess {
        let id = ProcessId::new(std::process::id());
        let permit = ProcessAccessPermit::issue(id, AntiCheatClass::None).unwrap();
        system.open(&permit).unwrap()
    }

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
        assert!(entry.executable.to_ascii_lowercase().ends_with(".exe"));
    }

    #[test]
    fn reads_this_process_s_memory_through_a_permit() {
        let system = WindowsSystem::new();
        let process = own_process(&system);
        let value = Box::new(1234.5_f64);
        let address = ptr::from_ref(value.as_ref()).addr() as u64;

        let mut bytes = [0; 8];
        process.read(address, &mut bytes).unwrap();
        assert_eq!(ValueType::F64.decode(&bytes), Some(Number::Float(1234.5)));
        assert!(
            process
                .regions()
                .unwrap()
                .iter()
                .any(|region| region.base <= address && address < region.base + region.size)
        );
        assert_eq!(process.pointer_width(), PointerWidth::Bits64);

        let pointer = Box::new(address);
        let pointer_address = ptr::from_ref(pointer.as_ref()).addr() as u64;
        let steps = follow_pointer_chain(&process, pointer_address, &[0]).unwrap();
        assert_eq!(steps[0].address, address);
    }

    #[test]
    fn names_the_opened_process_s_image() {
        let system = WindowsSystem::new();
        let own = std::env::current_exe().unwrap();
        let expected = own.file_name().unwrap().to_string_lossy();
        let named = own_process(&system).image_name().unwrap();
        assert!(
            named.eq_ignore_ascii_case(&expected),
            "{named} != {expected}"
        );
    }

    #[test]
    fn lists_this_process_s_modules() {
        let system = WindowsSystem::new();
        let modules = own_process(&system).modules().unwrap();
        assert!(
            modules
                .iter()
                .any(|module| module.name.eq_ignore_ascii_case("ntdll.dll"))
        );
    }

    #[test]
    fn enumerates_windows_and_reads_screen_pixels() {
        let system = WindowsSystem::new();
        system.windows().unwrap();
        system.screen_pixel(0, 0).unwrap();
        assert!(system.screen_pixel(-1_000_000, -1_000_000).is_err());
    }
}
