//! The tool list the agent sees: each tool's name, description, and
//! input schema.

use appytappy_sources::memory::ValueType;
use serde_json::{Value, json};

use super::{READ_BYTES_LIMIT, RESULTS_LIMIT, SCREEN_POINTS_LIMIT};

/// The tool definitions `tools/list` returns.
pub(crate) fn definitions() -> Value {
    let value_types: Vec<&str> = ValueType::ALL.iter().map(|kind| kind.name()).collect();
    let address = json!({
        "type": ["string", "integer"],
        "description": "An address as a hex string such as \"0x1a2b3c\", or a number.",
    });
    let number = json!({ "type": "number" });
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
            "process_attach",
            "Open a process to read its memory, replacing any attached process and its scan. The user must allow it in a dialog; if they refuse, do not retry. Known anti-cheat processes are refused. Returns the loaded modules.",
            json!({ "pid": { "type": "integer" } }),
            &["pid"],
        ),
        tool(
            "process_detach",
            "Close the attached process and drop its scan.",
            json!({}),
            &[],
        ),
        tool(
            "memory_scan_first",
            "Start a value scan of the attached process's writable memory. exact needs value, range needs min and max (inclusive), and unknown snapshots everything for a value you cannot see yet. Floats in exact mode must match the stored value; prefer range for floats.",
            json!({
                "value_type": { "type": "string", "enum": value_types },
                "mode": { "type": "string", "enum": ["exact", "range", "unknown"] },
                "value": number,
                "min": number,
                "max": number,
            }),
            &["value_type", "mode"],
        ),
        tool(
            "memory_scan_next",
            "Narrow the scan: keep addresses whose value now passes the test, comparing with the value at the previous scan for changed, unchanged, increased, and decreased. Have the user change the value in the app between scans.",
            json!({
                "mode": { "type": "string", "enum": ["exact", "range", "changed", "unchanged", "increased", "decreased"] },
                "value": number,
                "min": number,
                "max": number,
            }),
            &["mode"],
        ),
        tool(
            "memory_scan_results",
            "List the scan's addresses with their previous and current values. module_offset names the module an address lies in; such an address is static and can start a pointer chain.",
            json!({
                "offset": { "type": "integer", "minimum": 0 },
                "limit": { "type": "integer", "minimum": 1, "maximum": RESULTS_LIMIT },
            }),
            &[],
        ),
        tool(
            "memory_read",
            "Read one value from the attached process.",
            json!({ "address": address, "value_type": { "type": "string", "enum": value_types } }),
            &["address", "value_type"],
        ),
        tool(
            "memory_read_bytes",
            "Read raw bytes from the attached process as hex, to inspect a structure around an address.",
            json!({ "address": address, "length": { "type": "integer", "minimum": 1, "maximum": READ_BYTES_LIMIT } }),
            &["address", "length"],
        ),
        tool(
            "memory_follow_pointer",
            "Follow a pointer chain: start at the module's base plus base (or at base alone without a module); for each offset, read a pointer there and add the offset. Returns each step and the final address, and its value when value_type is given. A chain that still reaches the value after the app restarts is stable.",
            json!({
                "module": { "type": "string", "description": "A loaded module's name, such as \"Example.exe\"." },
                "base": address,
                "offsets": { "type": "array", "items": { "type": ["string", "integer"] }, "description": "Offsets as numbers or hex strings such as \"0x1c\" or \"-0x8\"." },
                "value_type": { "type": "string", "enum": value_types },
            }),
            &["base"],
        ),
        tool(
            "screen_sample",
            "Read the color of screen pixels, in the screen coordinates window_list reports. This never touches an app's process, so it works whatever its anti-cheat.",
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
