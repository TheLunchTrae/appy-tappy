//! Windows actions the runtime performs when a rule fires.
//!
//! [`Actions`] runs each action through [`Controls`] and keeps a journal of
//! what the restoring rules changed, so releasing a rule puts back the right
//! state even when several rules changed the same thing. [`Controls`] only
//! lists things and acts on one of them; every choice is made here, where it
//! is tested with fake controls on every platform. The Windows adapter in
//! `win` implements [`Controls`]. `wiki/developer/Actions.md` explains the v1 set.
//!
//! See `wiki/developer/Architecture.md` for this crate's boundaries.

mod backoff;
mod controls;
mod error;
mod journal;
mod keys;
#[cfg(windows)]
pub mod win;

pub use backoff::Backoff;
pub use controls::{Controls, MediaSession, Volume};
pub use error::ActionError;
pub use journal::{Actions, MEDIA_RETRY, Watched};
pub use keys::{HOLD, Stroke, strokes};
