//! Runs the built server and talks to it over stdin and stdout.

use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

#[test]
fn the_server_answers_over_stdio_until_input_ends() {
    let mut server = Command::new(env!("CARGO_BIN_EXE_appytappy-mcp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("the server starts");
    let mut input = server.stdin.take().expect("stdin is piped");
    input
        .write_all(
            concat!(
                r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
                "\n",
                r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
                "\n",
                r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#,
                "\n",
            )
            .as_bytes(),
        )
        .expect("the request is written");
    drop(input);
    let output = server.wait_with_output().expect("the server exits");
    assert!(output.status.success(), "{:?}", output.status);
    let responses: Vec<Value> = String::from_utf8(output.stdout)
        .expect("the output is UTF-8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("each line is JSON"))
        .collect();
    assert_eq!(responses.len(), 2);
    assert_eq!(responses[0]["result"]["serverInfo"]["name"], "appytappy");
    assert_eq!(
        responses[1],
        json!({ "jsonrpc": "2.0", "id": 2, "result": {} })
    );
}
