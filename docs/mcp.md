# MCP investigation server

`appytappy-mcp` is a local [MCP](https://modelcontextprotocol.io/) server. An
AI agent such as Claude Code starts it and uses its tools to work out how an
application exposes a value, such as a number it shows in one spot of its
window or a counter it keeps in memory. AppyTappy then reads that value
without hardcoding anything per application.

The server talks MCP over standard input and output, so it has no network
port. Every tool only reads: none writes to an application's memory or sends
input.

## Tools

| Tool                    | What it does                                                          |
| ----------------------- | --------------------------------------------------------------------- |
| `process_list`          | Lists running processes and names known anti-cheat processes.         |
| `window_list`           | Lists windows with their process, title, class, and screen bounds.    |
| `screen_sample`         | Reads the color of screen pixels.                                     |
| `process_attach`        | Opens a process to read its memory, after the user allows it.         |
| `process_detach`        | Closes the attached process.                                          |
| `memory_scan_first`     | Starts a value scan: exact, range, or unknown.                        |
| `memory_scan_next`      | Narrows the scan by value or by how the value changed.                |
| `memory_scan_results`   | Lists the remaining addresses and their values.                       |
| `memory_read`           | Reads one value at an address.                                        |
| `memory_read_bytes`     | Reads raw bytes around an address.                                    |
| `memory_follow_pointer` | Follows a pointer chain from a module and reads the value it ends at. |

The first three never open an application's process, so they are safe for
protected applications too, such as ones with anti-cheat. Tool names are `area_verb`, so the definition and
config tools that come next slot in as `definition_*`, `rule_*`,
`source_test`, and `config_compile`.

## Anti-cheat safety

The server enforces the block itself, so no wording from the agent gets past
it.

- `process_attach` refuses the process of a known anti-cheat product.
- Every other process stays closed until the user allows it. Each attach
  shows a Windows dialog that AppyTappy's tools cannot answer, and it names
  any anti-cheat that is running. The default button is **No**.
- If the user chooses **No**, the server refuses that program for the rest of
  the session without asking again.
- After opening, the server checks that the process still runs the program
  the user allowed, in case it exited and its ID was reused.
- The server opens a process with read and query rights only.

Only allow an application that has no anti-cheat or other protection. An agent
that can run shell commands on the same desktop could click the dialog
itself, so don't auto-approve the agent's shell commands while it
investigates.

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

## First target test

This test answers
[#4](https://github.com/TheLunchTrae/appy-tappy/issues/4): can a value an
application shows on screen be read from memory through a pointer chain that
survives a restart?

1. Pick an application that has no anti-cheat and shows a number that changes
   when you do something, such as a counter, a score, or a progress
   percentage. Start it and get to where the number shows.
2. Start Claude Code on the same PC and ask it something like this:

   ```text
   Use the appytappy tools to find where Example.exe stores the number it
   shows, then find a pointer chain to it from Example.exe that still works
   after I restart the application. Tell me when to change the number.
   ```

   Use the application's own executable name in place of `Example.exe`.

3. Choose **Yes** in the AppyTappy dialog when Claude attaches to the
   application.
4. Change the number when Claude asks. Each change lets it narrow the scan.
   The number may be stored as a float or an integer, or worked out from
   other values, so Claude may need a few rounds.
5. Once Claude has a chain, quit and restart the application when it asks.
   The restarted application is a new process, so choose **Yes** in the
   dialog again, then let Claude check the chain.

The result to bring back to #4 is the application tested, the pointer chain
(module, base offset, and offsets), its value type, and whether it survived
the restart.
