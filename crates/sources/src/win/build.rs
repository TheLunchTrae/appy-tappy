//! Reading which build of an executable a process runs, without opening the
//! process.
//!
//! The kernel names each process's executable in its process list, which
//! `NtQuerySystemInformation` reads by process ID with no handle to the
//! process: the same list a process snapshot reads. The path it gives is a
//! device path, which the drives' device names turn into one that file
//! calls open.

use std::ffi::c_void;
use std::io;
use std::mem::size_of;
use std::path::{Path, PathBuf};
use std::ptr;

use appytappy_config::anti_cheat::ProcessId;
use appytappy_config::app_file::BuildDef;
use windows::Wdk::System::SystemInformation::{NtQuerySystemInformation, SYSTEM_INFORMATION_CLASS};
use windows::Win32::Foundation::{HANDLE, STATUS_INFO_LENGTH_MISMATCH, UNICODE_STRING};
use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, QueryDosDeviceW, VS_FIXEDFILEINFO, VerQueryValueW,
};
use windows::core::{HSTRING, PWSTR, w};

use crate::build;

/// `SystemProcessIdInformation`, which the Windows headers leave out: one
/// process's executable path, by its ID.
const PROCESS_ID_INFORMATION: SYSTEM_INFORMATION_CLASS = SYSTEM_INFORMATION_CLASS(88);

/// `SYSTEM_PROCESS_ID_INFORMATION`, which the Windows headers leave out.
#[repr(C)]
struct ProcessIdInformation {
    process: HANDLE,
    image: UNICODE_STRING,
}

/// The longest device path, in UTF-16 units: a `UNICODE_STRING` counts its
/// length in bytes in a `u16`.
const MAX_PATH_UNITS: usize = 32_767;

/// The build of `process`'s executable.
pub(super) fn executable_build(process: ProcessId) -> io::Result<BuildDef> {
    let path = executable_path(process)?;
    build::read(&path, file_version(&path))
}

/// The path of `process`'s executable.
fn executable_path(process: ProcessId) -> io::Result<PathBuf> {
    let mut units = vec![0_u16; 260];
    loop {
        let mut information = ProcessIdInformation {
            process: HANDLE(ptr::without_provenance_mut(
                usize::try_from(process.raw()).map_err(io::Error::other)?,
            )),
            image: UNICODE_STRING {
                Length: 0,
                MaximumLength: u16::try_from(units.len().saturating_mul(2))
                    .map_err(io::Error::other)?,
                Buffer: PWSTR(units.as_mut_ptr()),
            },
        };
        #[expect(unsafe_code, reason = "a system call")]
        // SAFETY: `information` is a live SYSTEM_PROCESS_ID_INFORMATION whose
        // buffer is `units`, as long as MaximumLength says; the call writes
        // only within them.
        let status = unsafe {
            NtQuerySystemInformation(
                PROCESS_ID_INFORMATION,
                (&raw mut information).cast::<c_void>(),
                u32::try_from(size_of::<ProcessIdInformation>()).map_err(io::Error::other)?,
                ptr::null_mut(),
            )
        };
        // A longer path asks for a buffer as long as MaximumLength.
        let wanted = usize::from(information.image.MaximumLength) / 2;
        if status == STATUS_INFO_LENGTH_MISMATCH && wanted > units.len() && wanted <= MAX_PATH_UNITS
        {
            units.resize(wanted, 0);
            continue;
        }
        status.ok()?;
        let length = usize::from(information.image.Length) / 2;
        let device_path = String::from_utf16_lossy(units.get(..length).unwrap_or_default());
        return build::drive_path(&device_path, &drives()).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("no drive holds {device_path}"),
            )
        });
    }
}

/// Each drive that names a device, such as `C:` with
/// `\Device\HarddiskVolume3`.
fn drives() -> Vec<(String, String)> {
    ('A'..='Z')
        .filter_map(|letter| {
            let drive = format!("{letter}:");
            let mut target = [0_u16; 512];
            #[expect(unsafe_code, reason = "a Win32 call")]
            // SAFETY: the call writes at most `target`'s length into it.
            let written =
                unsafe { QueryDosDeviceW(&HSTRING::from(drive.as_str()), Some(&mut target)) };
            // The target is a list of names; the first is the device.
            (written > 0).then(|| {
                let end = target
                    .iter()
                    .position(|&unit| unit == 0)
                    .unwrap_or(target.len());
                (
                    drive,
                    String::from_utf16_lossy(target.get(..end).unwrap_or_default()),
                )
            })
        })
        .collect()
}

/// The file version of the executable at `path`, or `None` when it has
/// none.
fn file_version(path: &Path) -> Option<String> {
    let name = HSTRING::from(path);
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: the call reads the live string `name`.
    let size = unsafe { GetFileVersionInfoSizeW(&name, None) };
    let mut data = vec![0_u8; usize::try_from(size).ok().filter(|&size| size > 0)?];
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: `data` is `size` bytes long, as the call is told.
    unsafe { GetFileVersionInfoW(&name, None, size, data.as_mut_ptr().cast::<c_void>()) }.ok()?;
    let mut fixed: *mut c_void = ptr::null_mut();
    let mut length = 0_u32;
    #[expect(unsafe_code, reason = "a Win32 call")]
    // SAFETY: `data` holds the version information the call above wrote, and
    // `fixed` and `length` are live locals the call writes.
    let found = unsafe {
        VerQueryValueW(
            data.as_ptr().cast::<c_void>(),
            w!("\\"),
            &raw mut fixed,
            &raw mut length,
        )
    };
    if !found.as_bool()
        || fixed.is_null()
        || usize::try_from(length).ok()? < size_of::<VS_FIXEDFILEINFO>()
    {
        return None;
    }
    #[expect(unsafe_code, reason = "reading the structure the call pointed to")]
    // SAFETY: `fixed` points into `data`, which is still alive, at a
    // VS_FIXEDFILEINFO at least as long as the structure, as checked above.
    let info = unsafe { fixed.cast::<VS_FIXEDFILEINFO>().read_unaligned() };
    build::version_text(info.dwFileVersionMS, info.dwFileVersionLS)
}
