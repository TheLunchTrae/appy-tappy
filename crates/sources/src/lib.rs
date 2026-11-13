//! Source primitives that read app state for the runtime and the setup app.
//!
//! The screen region checks are portable and take a cropped
//! [`screen::Image`]. The Windows adapter in `win` implements
//! [`system::System`] and [`screen::Capture`]; other platforms get
//! [`system::Unsupported`].
//!
//! Both programs build this crate, so it holds what both read with. A few
//! items serve only the setup app and stay here because they are the other
//! half of something the runtime uses: [`screen::Image::encode_png`] and
//! [`screen::Image::encode_png_masked`] write the files that
//! [`screen::Image::decode_png_masked`] reads, and
//! [`screen::digits::DigitReader::shapes`] and
//! [`screen::digits::DigitReader::is_ink`] are the shapes and the ink a
//! read itself decides with, so what the setup app shows of a reading
//! cannot differ from the reading. [`system::ScreenPixels`] serves only the
//! MCP server. Code that only the setup app needs and that stands by
//! itself, such as selecting the thing a state check looks for, lives in
//! the setup app.
//!
//! See `wiki/Architecture.md` for this crate's boundaries.

pub mod automation;
#[cfg(any(windows, test))]
mod build;
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
