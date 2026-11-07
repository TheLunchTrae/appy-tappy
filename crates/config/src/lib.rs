//! App definition, rule, and compiled config formats, their validation, and the compile step.
//!
//! See `docs/architecture.md` for this crate's boundaries.

#![forbid(unsafe_code)]

pub mod action;
pub mod anti_cheat;
pub mod app_file;
pub mod automation_file;
pub mod paths;
pub mod rule;
pub mod signal;
pub mod store;
pub mod window_match;
