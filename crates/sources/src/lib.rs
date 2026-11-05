//! Source primitives that read game state for the runtime and the setup app,
//! and the guided memory scan that finds what to read.
//!
//! The scan and the pointer-chain walker are portable and take a
//! [`memory::ProcessMemory`]. The Windows adapter in `win` implements it and
//! [`system::System`]; other platforms get [`system::Unsupported`].
//!
//! See `docs/architecture.md` for this crate's boundaries.

pub mod memory;
pub mod system;
#[cfg(windows)]
pub mod win;

/// The [`system::System`] of the platform this build targets.
#[cfg(windows)]
pub type HostSystem = win::WindowsSystem;

/// The [`system::System`] of the platform this build targets.
#[cfg(not(windows))]
pub type HostSystem = system::Unsupported;
