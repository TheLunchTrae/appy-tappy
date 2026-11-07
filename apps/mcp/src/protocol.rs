//! The MCP transport: newline-delimited JSON-RPC 2.0 over stdin and stdout.
//!
//! The server answers `initialize`, `ping`, `tools/list`, and `tools/call`,
//! ignores notifications and the client's responses, and handles one message
//! at a time, so the session has a single owner.

use std::io::{self, BufRead, Write};

use appytappy_sources::system::System;
use serde_json::{Map, Value, json};

use crate::tools::{self, Session};

/// The protocol revisions this server speaks, newest first. 2025-03-26 is
/// left out because it requires JSON-RPC batches, which this server does not
/// take.
const PROTOCOL_VERSIONS: [&str; 3] = ["2025-11-25", "2025-06-18", "2024-11-05"];

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// What the server tells the agent when it connects.
const INSTRUCTIONS: &str = "AppyTappy investigates how a Windows application exposes a value, \
such as a number shown in one spot of its window, so the user can automate on it. Every tool \
only reads. Prefer sources that do not touch the application: windows, screen pixels. To find a value in memory: process_list, \
then process_attach (the user must allow it in a dialog; if they refuse, do not retry and use \
another source), then memory_scan_first, then ask the user to make the value change and run \
memory_scan_next until few candidates remain, then memory_scan_results. Addresses change when \
the application restarts, so find a stable pointer chain before relying on one: inspect the \
structure around the value with memory_read_bytes, scan for pointer-sized values in a range just \
below the address to find what points at it, repeat until a result has a module_offset, then \
check the chain with memory_follow_pointer after the user restarts the application.";

/// Reads messages from `input` and writes each response to `output` until
/// the input ends.
///
/// # Errors
///
/// Returns the first read or write error.
pub(crate) fn serve<S: System>(
    mut input: impl BufRead,
    mut output: impl Write,
    session: &mut Session<S>,
) -> io::Result<()> {
    let mut line = Vec::new();
    loop {
        line.clear();
        if input.read_until(b'\n', &mut line)? == 0 {
            return Ok(());
        }
        let response = match std::str::from_utf8(&line) {
            Ok(text) if text.trim().is_empty() => None,
            Ok(text) => handle(session, text),
            Err(error) => Some(failure(&Value::Null, PARSE_ERROR, &error.to_string())),
        };
        if let Some(response) = response {
            serde_json::to_writer(&mut output, &response)?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }
}

/// Handles one message and returns the response, if it needs one.
fn handle<S: System>(session: &mut Session<S>, line: &str) -> Option<Value> {
    let message: Value = match serde_json::from_str(line) {
        Ok(message) => message,
        Err(error) => return Some(failure(&Value::Null, PARSE_ERROR, &error.to_string())),
    };
    let Some(message) = message.as_object() else {
        return Some(failure(
            &Value::Null,
            INVALID_REQUEST,
            "a message must be an object",
        ));
    };
    let id = message.get("id");
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        if message.contains_key("result") || message.contains_key("error") {
            return None;
        }
        return Some(failure(
            id.unwrap_or(&Value::Null),
            INVALID_REQUEST,
            "a request needs a method",
        ));
    };
    let id = id?;
    if id.is_null() {
        return Some(failure(
            id,
            INVALID_REQUEST,
            "a request id must not be null",
        ));
    }
    let params = message.get("params").unwrap_or(&Value::Null);
    Some(match dispatch(session, method, params) {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, text)) => failure(id, code, &text),
    })
}

fn dispatch<S: System>(
    session: &mut Session<S>,
    method: &str,
    params: &Value,
) -> Result<Value, (i64, String)> {
    match method {
        "initialize" => Ok(initialize(params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools::definitions() })),
        "tools/call" => {
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| (INVALID_PARAMS, "tools/call needs a tool name".to_owned()))?;
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| Value::Object(Map::new()));
            let outcome = session
                .call(name, arguments)
                .ok_or_else(|| (INVALID_PARAMS, format!("unknown tool: {name}")))?;
            Ok(tool_result(outcome))
        },
        _ => Err((METHOD_NOT_FOUND, format!("unknown method: {method}"))),
    }
}

fn initialize(params: &Value) -> Value {
    let requested = params.get("protocolVersion").and_then(Value::as_str);
    let version = PROTOCOL_VERSIONS
        .into_iter()
        .find(|version| Some(*version) == requested)
        .unwrap_or(PROTOCOL_VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "appytappy", "version": env!("CARGO_PKG_VERSION") },
        "instructions": INSTRUCTIONS,
    })
}

/// A tool's outcome as MCP content: the JSON result as structured content
/// and as text, or the error as text with `isError`.
fn tool_result(outcome: Result<Value, String>) -> Value {
    match outcome {
        Ok(value) => json!({
            "content": [{ "type": "text", "text": value.to_string() }],
            "structuredContent": value,
        }),
        Err(text) => json!({
            "content": [{ "type": "text", "text": text }],
            "isError": true,
        }),
    }
}

fn failure(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

#[cfg(test)]
mod tests {
    use appytappy_sources::system::Unsupported;

    use super::*;

    fn exchange(input: &str) -> Vec<Value> {
        let mut session = Session::new(Unsupported);
        let mut output = Vec::new();
        serve(input.as_bytes(), &mut output, &mut session).unwrap();
        String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    #[test]
    fn negotiates_the_requested_protocol_version_or_offers_the_newest() {
        let responses = exchange(concat!(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"1999-01-01"}}"#,
            "\n",
        ));
        assert_eq!(responses[0]["id"], 1);
        assert_eq!(responses[0]["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(
            responses[0]["result"]["capabilities"],
            json!({ "tools": {} })
        );
        assert_eq!(responses[1]["result"]["protocolVersion"], "2025-11-25");
    }

    #[test]
    fn reports_a_line_that_is_not_utf8_and_keeps_serving() {
        let mut session = Session::new(Unsupported);
        let mut output = Vec::new();
        let input: &[u8] = b"\xff\xfe\n{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"ping\"}";
        serve(input, &mut output, &mut session).unwrap();
        let responses: Vec<Value> = String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(responses[0]["error"]["code"], -32700);
        assert_eq!(
            responses[1],
            json!({ "jsonrpc": "2.0", "id": 7, "result": {} })
        );
    }

    #[test]
    fn answers_requests_and_stays_silent_for_notifications_and_responses() {
        let responses = exchange(concat!(
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            "\n\n",
            r#"{"jsonrpc":"2.0","id":"a","result":{}}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":"b","method":"ping"}"#,
            "\n",
        ));
        assert_eq!(
            responses,
            [json!({ "jsonrpc": "2.0", "id": "b", "result": {} })]
        );
    }

    #[test]
    fn reports_malformed_messages_and_unknown_methods() {
        let responses = exchange(concat!(
            "not json\n",
            "[1]\n",
            r#"{"jsonrpc":"2.0","id":3}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":4,"method":"resources/list"}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"nope"}}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{}}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":null,"method":"ping"}"#,
            "\n",
        ));
        let codes: Vec<(Value, Value)> = responses
            .iter()
            .map(|response| (response["id"].clone(), response["error"]["code"].clone()))
            .collect();
        assert_eq!(
            codes,
            [
                (Value::Null, json!(-32700)),
                (Value::Null, json!(-32600)),
                (json!(3), json!(-32600)),
                (json!(4), json!(-32601)),
                (json!(5), json!(-32602)),
                (json!(6), json!(-32602)),
                (Value::Null, json!(-32600)),
            ]
        );
    }

    #[test]
    fn lists_tools_and_returns_tool_failures_as_error_results() {
        let responses = exchange(concat!(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"process_list"}}"#,
            "\n",
        ));
        assert_eq!(responses[0]["result"]["tools"], tools::definitions());
        let failed = &responses[1]["result"];
        assert_eq!(failed["isError"], true);
        assert_eq!(
            failed["content"][0]["text"],
            "cannot list processes: AppyTappy reads application state on Windows only"
        );
    }

    #[test]
    fn returns_tool_results_as_structured_and_text_content() {
        let value = json!({ "count": 3 });
        assert_eq!(
            tool_result(Ok(value.clone())),
            json!({
                "content": [{ "type": "text", "text": r#"{"count":3}"# }],
                "structuredContent": value,
            })
        );
    }
}
