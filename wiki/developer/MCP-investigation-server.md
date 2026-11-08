# MCP investigation server

`appytappy-mcp` is a local [MCP](https://modelcontextprotocol.io/) server. An
AI agent such as Claude Code starts it and uses its tools to work out how an
application shows a value, such as a number in one spot of its window.
AppyTappy then reads that value without hardcoding anything per application.

The server talks MCP over standard input and output, so it has no network
port. Every tool only reads: none sends input.

## Tools

| Tool            | What it does                                                       |
| --------------- | ------------------------------------------------------------------ |
| `process_list`  | Lists running processes and names known anti-cheat processes.      |
| `window_list`   | Lists windows with their process, title, class, and screen bounds. |
| `screen_sample` | Reads the color of screen pixels.                                  |

No tool opens an application's process, so they are safe for protected
applications too, such as ones with anti-cheat. Tool names are `area_verb`,
so the tools that come next slot in beside these: `definition_*` and `rule_*`
for app definitions and rules, and `source_test` to try a source.

## Build

On Windows, install Rust with [rustup](https://rustup.rs/) and the Visual
Studio C++ build tools. From the repository root:

```sh
cargo build --release -p appytappy-mcp
```

The server is `target\release\appytappy-mcp.exe`.

## Register it in Claude Code

Add the server once for your user, with the full path to the program:

```sh
claude mcp add --scope user appytappy -- C:/path/to/appy-tappy/target/release/appytappy-mcp.exe
```

Check it with `claude mcp list`, or run `/mcp` inside Claude Code. Claude
Code starts the server when a session starts and stops it when the session
ends. Rebuild with the same command after pulling changes, then start a new
Claude Code session.
