//! Which build of an application runs: its executable's file version, its
//! size, and when it was written.
//!
//! The Windows adapter finds a process's executable from the system's list
//! of processes, without opening the process, so reading the build is safe
//! for every anti-cheat class. It reads the file version, and this module
//! does the rest.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use appytappy_config::app_file::BuildDef;

/// The build of the executable at `path`, whose file version is `version`.
///
/// # Errors
///
/// Returns the error from reading the file's size or the time it was
/// written.
pub fn read(path: &Path, version: Option<String>) -> io::Result<BuildDef> {
    let metadata = fs::metadata(path)?;
    // A file written before 1970 counts as written at its start.
    let modified = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    Ok(BuildDef {
        version,
        size: metadata.len(),
        modified,
    })
}

/// A fixed file version as text, such as `1.4.2.0` from its high and low
/// halves, or `None` for `0.0.0.0`, which a file without a real version
/// carries.
#[must_use]
pub fn version_text(high: u32, low: u32) -> Option<String> {
    ((high, low) != (0, 0)).then(|| {
        format!(
            "{}.{}.{}.{}",
            high >> 16,
            high & 0xFFFF,
            low >> 16,
            low & 0xFFFF
        )
    })
}

/// The path that programs open for `device_path`, a path as the kernel
/// names it, such as `\Device\HarddiskVolume3\Games\Example.exe`.
///
/// `drives` pairs each drive, such as `C:`, with the device it names. A path
/// on a network share, under `\Device\Mup`, becomes a `\\server\share` path.
/// `None` when no drive names the path's device.
#[must_use]
pub fn drive_path(device_path: &str, drives: &[(String, String)]) -> Option<PathBuf> {
    let under = |device: &str| {
        let rest = device_path.get(device.len()..)?;
        (device_path
            .get(..device.len())?
            .eq_ignore_ascii_case(device)
            && rest.starts_with('\\'))
        .then_some(rest)
    };
    if let Some(rest) = under(r"\Device\Mup") {
        return Some(PathBuf::from(format!(r"\{rest}")));
    }
    drives.iter().find_map(|(drive, device)| {
        under(device).map(|rest| PathBuf::from(format!("{drive}{rest}")))
    })
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::process;

    use super::*;

    #[test]
    fn a_build_is_the_files_version_size_and_write_time() {
        let path = env::temp_dir().join(format!("appytappy-build-{}.exe", process::id()));
        fs::write(&path, b"MZ1234").expect("write");
        let build = read(&path, Some("1.0.0.0".to_owned())).expect("reads");
        assert_eq!(build.version.as_deref(), Some("1.0.0.0"));
        assert_eq!(build.size, 6);
        let written = fs::metadata(&path)
            .and_then(|metadata| metadata.modified())
            .expect("modified")
            .duration_since(UNIX_EPOCH)
            .expect("after 1970")
            .as_secs();
        assert_eq!(build.modified, written);
        assert!(build.modified > 0);
        fs::remove_file(&path).expect("clean up");
        assert!(read(&path, None).is_err());
    }

    #[test]
    fn a_version_reads_as_four_parts() {
        assert_eq!(
            version_text(0x0001_0004, 0x0002_0000).as_deref(),
            Some("1.4.2.0")
        );
        assert_eq!(
            version_text(0xFFFF_0000, 0x0000_FFFF).as_deref(),
            Some("65535.0.0.65535")
        );
        assert_eq!(version_text(0, 0), None);
        assert_eq!(version_text(0, 1).as_deref(), Some("0.0.0.1"));
    }

    #[test]
    fn a_device_path_becomes_a_drive_or_share_path() {
        let drives = [
            ("C:".to_owned(), r"\Device\HarddiskVolume1".to_owned()),
            ("D:".to_owned(), r"\Device\HarddiskVolume10".to_owned()),
        ];
        assert_eq!(
            drive_path(r"\Device\HarddiskVolume10\Games\Example.exe", &drives),
            Some(PathBuf::from(r"D:\Games\Example.exe"))
        );
        assert_eq!(
            drive_path(r"\device\harddiskvolume1\Example.exe", &drives),
            Some(PathBuf::from(r"C:\Example.exe"))
        );
        assert_eq!(
            drive_path(r"\Device\Mup\server\share\Example.exe", &drives),
            Some(PathBuf::from(r"\\server\share\Example.exe"))
        );
        assert_eq!(
            drive_path(r"\Device\HarddiskVolume2\Example.exe", &drives),
            None
        );
        assert_eq!(drive_path(r"\Device\HarddiskVolume1", &drives), None);
        assert_eq!(drive_path("", &drives), None);
    }
}
