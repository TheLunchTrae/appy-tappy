//! How the tools word their results and errors for the agent.

use std::io;

pub(super) fn os_error(operation: &'static str) -> impl FnOnce(io::Error) -> String {
    move |error| format!("cannot {operation}: {error}")
}

pub(super) fn hex(value: u64) -> String {
    format!("{value:#x}")
}
