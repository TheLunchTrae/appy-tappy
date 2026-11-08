//! Source primitives that read app state for the runtime and the setup app.
//!
//! The screen region checks are portable and take a cropped
//! [`screen::Image`]. The Windows adapter in `win` implements
//! [`system::System`] and [`screen::Capture`]; other platforms get
//! [`system::Unsupported`].
//!
//! See `wiki/Architecture.md` for this crate's boundaries.

pub mod build;
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
