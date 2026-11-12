//! Windows actions the runtime performs when a rule fires.
//!
//! [`Actions`] runs each action through [`Controls`] and keeps a journal of
//! what the restoring rules changed, so releasing a rule puts back the right
//! state even when several rules changed the same thing. [`click`] clicks
//! where its caller placed a click, through [`Input`] alone, so the setup
//! app can send a click without the media and audio controls. [`Controls`]
//! and [`Input`] only list things and act on one of them; every choice is
//! made here, where it is tested with fakes on every platform. The Windows
//! adapter in `win` implements both; other platforms get [`Unsupported`]
//! input. `wiki/Actions.md` explains the v1 set.
//!
//! See `wiki/Architecture.md` for this crate's boundaries.

mod backoff;
mod click;
mod controls;
mod error;
mod input;
mod journal;
mod keys;
#[cfg(windows)]
pub mod win;

pub use backoff::Backoff;
pub use click::{MouseButton, MouseEvent, MouseMessage, PlacedClick, Press, click, pack};
pub use controls::{Controls, MediaSession, Volume};
pub use error::ActionError;
pub use input::{Input, Unsupported};
pub use journal::{Actions, MEDIA_RETRY, Watched};
pub use keys::{HOLD, Stroke, strokes};

/// The [`Input`] of the platform this build targets.
#[cfg(windows)]
pub type HostInput = win::WindowsInput;

/// The [`Input`] of the platform this build targets.
#[cfg(not(windows))]
pub type HostInput = Unsupported;
