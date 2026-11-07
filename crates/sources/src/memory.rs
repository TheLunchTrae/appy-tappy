//! Reading another process's memory: the guided value scan and the
//! pointer-chain walker.
//!
//! A scan works like Cheat Engine's: a first scan finds every address whose
//! value passes a test, or snapshots all readable memory when the value is
//! unknown, and each next scan keeps the addresses whose value passes a new
//! test against its current and previous value.

#![forbid(unsafe_code)]

use std::io;

#[cfg(test)]
mod fake;
mod pointer;
mod scan;
mod value;

pub use pointer::{PointerError, PointerStep, follow_pointer_chain, read_pointer};
pub use scan::{CANDIDATE_LIMIT, Candidate, FirstScan, NextScan, SNAPSHOT_LIMIT, Scan, ScanError};
pub use value::{Number, ValueTest, ValueType};

/// One committed, readable region of a process's address space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    /// The region's first address.
    pub base: u64,
    /// The region's length in bytes.
    pub size: u64,
}

/// A module, such as the app's executable or a DLL, loaded in a process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    /// The module's file name, such as `Example.exe`.
    pub name: String,
    /// The address the module is loaded at.
    pub base: u64,
    /// The module's length in bytes.
    pub size: u64,
}

/// The pointer size of a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerWidth {
    /// A 32-bit process.
    Bits32,
    /// A 64-bit process.
    Bits64,
}

impl PointerWidth {
    /// The pointer size in bits.
    #[must_use]
    pub const fn bits(self) -> u32 {
        match self {
            Self::Bits32 => 32,
            Self::Bits64 => 64,
        }
    }
}

/// Read access to one opened process.
pub trait ProcessMemory {
    /// The committed, writable regions a scan reads, in address order:
    /// the process's heap, stacks, and modules' data, where app state
    /// lives.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn regions(&self) -> io::Result<Vec<Region>>;

    /// Fills `buffer` with the memory at `address`.
    ///
    /// # Errors
    ///
    /// Fails when any byte of the range cannot be read.
    fn read(&self, address: u64, buffer: &mut [u8]) -> io::Result<()>;

    /// The loaded modules.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn modules(&self) -> io::Result<Vec<Module>>;

    /// The process's pointer size.
    fn pointer_width(&self) -> PointerWidth;

    /// The file name of the program the process runs, such as
    /// `Example.exe`, read from the opened process itself.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn image_name(&self) -> io::Result<String>;
}
