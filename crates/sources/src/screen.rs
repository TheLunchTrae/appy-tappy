//! The screen region source: one window's captured frames, a region of them,
//! and the checks that read a value from that region.
//!
//! Capture is the only part that needs the operating system, so it sits
//! behind the [`Capture`] trait. Everything else here is portable Rust over a
//! cropped [`Image`] and gives the same result on every platform.
//! `docs/developer/screen.md` describes the source and its checks.

#![forbid(unsafe_code)]

pub mod check;
pub mod digits;
pub mod saved;

mod capture;
mod image;

pub use capture::{Capture, CaptureSession, Crop, Frame, FrameHandler, NoSession};
pub use image::{FrameBuffer, Image, ImageError, OutsideWindow, Region, Size};
