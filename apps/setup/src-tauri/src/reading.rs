//! A value's reading as the UI shows it, and the words for a window size
//! where a value is not read.

use appytappy_config::signal::Value;
use appytappy_sources::screen::Size;
use appytappy_sources::screen::saved::Unread;
use serde::Serialize;

/// A value the check read, as the UI shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub(crate) enum Reading {
    /// A number, such as a percentage.
    Number(f64),
    /// Yes or no, such as whether a color shows.
    Flag(bool),
    /// A state's name, such as `Paused`, or `none`.
    State(String),
}

impl From<Value> for Reading {
    fn from(value: Value) -> Self {
        match value {
            Value::Number(number) => Self::Number(number.raw()),
            Value::Flag(flag) => Self::Flag(flag),
            Value::State(state) => Self::State(state.as_str().to_owned()),
        }
    }
}

/// Where and why a value is not read on a window of `size`, as the end of
/// a sentence that says it is not read. The live check, the automation
/// editor's live reading, and the overlays all say it in these words.
pub(crate) fn at_size(size: Size, unread: Unread) -> String {
    format!(
        "at {} × {}, the window's size now: {unread}",
        size.width, size.height
    )
}
