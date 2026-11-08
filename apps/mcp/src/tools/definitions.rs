//! The tool list the agent sees: each tool's name, description, and
//! input schema.

use serde_json::{Value, json};

use super::SCREEN_POINTS_LIMIT;

/// The tool definitions `tools/list` returns.
pub(crate) fn definitions() -> Value {
    let tools = [
        tool(
            "process_list",
            "List running processes without opening any of them. Each entry names a known anti-cheat product when the process is one.",
            json!({ "name_contains": { "type": "string", "description": "Keep processes whose executable name contains this, ignoring case." } }),
            &[],
        ),
        tool(
            "window_list",
            "List top-level windows with their process, title, class, and bounds in screen pixels, without opening any process.",
            json!({
                "pid": { "type": "integer", "description": "Keep this process's windows only." },
                "include_hidden": { "type": "boolean", "description": "Include hidden windows. Defaults to false." },
            }),
            &[],
        ),
        tool(
            "screen_sample",
            "Read the color of screen pixels, in the screen coordinates window_list reports. This never opens an app's process, so it works whatever its anti-cheat.",
            json!({
                "points": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": SCREEN_POINTS_LIMIT,
                    "items": {
                        "type": "object",
                        "properties": { "x": { "type": "integer" }, "y": { "type": "integer" } },
                        "required": ["x", "y"],
                        "additionalProperties": false,
                    },
                },
            }),
            &["points"],
        ),
    ];
    Value::Array(tools.into())
}

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false,
        },
        "annotations": { "readOnlyHint": true, "openWorldHint": false },
    })
}
