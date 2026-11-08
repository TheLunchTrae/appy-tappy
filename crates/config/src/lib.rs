//! The saved files' formats, rules, their validation, and where the files live and how they are
//! written.
//!
//! See `wiki/Architecture.md` for this crate's boundaries.

#![forbid(unsafe_code)]

pub mod action;
pub mod anti_cheat;
pub mod app_file;
pub mod automation_file;
pub mod click;
pub mod paths;
pub mod rule;
pub mod settings_file;
pub mod signal;
pub mod store;
pub mod window_match;
