//! The arguments each tool takes, parsed from the agent's JSON.

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

pub(super) fn parse<T: DeserializeOwned>(arguments: Value) -> Result<T, String> {
    serde_json::from_value(arguments).map_err(|error| format!("invalid arguments: {error}"))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProcessListArgs {
    pub(super) name_contains: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WindowListArgs {
    pub(super) pid: Option<u32>,
    #[serde(default)]
    pub(super) include_hidden: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ScreenSampleArgs {
    pub(super) points: Vec<Point>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Point {
    pub(super) x: i32,
    pub(super) y: i32,
}
