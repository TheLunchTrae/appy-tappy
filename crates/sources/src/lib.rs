//! Source primitives that read game state for the runtime and the setup app,
//! and the guided memory scan that finds what to read.
//!
//! The scan and the pointer-chain walker are portable and take a
//! [`memory::ProcessMemory`]. The screen region checks are portable and take
//! a cropped [`screen::Image`]. The Windows adapter in `win` implements
//! [`memory::ProcessMemory`], [`system::System`], and [`screen::Capture`];
//! other platforms get [`system::Unsupported`].
//!
//! See `docs/architecture.md` for this crate's boundaries.

pub mod memory;
pub mod schedule;
pub mod screen;
pub mod system;
#[cfg(windows)]
pub mod win;

/// The [`system::System`] of the platform this build targets.
#[cfg(windows)]
pub type HostSystem = win::WindowsSystem;

/// The [`system::System`] of the platform this build targets.
#[cfg(not(windows))]
pub type HostSystem = system::Unsupported;
