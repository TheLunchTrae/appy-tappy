//! The screen region source: one window's captured frames, a region of them,
//! and the checks that read a value from that region.
//!
//! Capture is the only part that needs the operating system, so it sits
//! behind the [`Capture`] trait. Everything else here is portable Rust over a
//! cropped [`Image`] and gives the same result on every platform.
//! `wiki/Screen-regions.md` describes the source and its checks.

#![forbid(unsafe_code)]

pub mod check;
pub mod click;
pub mod digits;
pub mod find;
pub mod mask;
pub mod place;
pub mod saved;
pub mod scale;
pub mod select;
pub mod settle;

mod capture;
mod image;
mod layout;
mod resample;

pub use capture::{Capture, CaptureSession, Crop, Frame, FrameHandler, NoSession, grab};
pub use image::{FrameBuffer, Image, ImageError, OutsideWindow, Region, Size};
pub use layout::{Layout, Placement};
