# MCP investigation server

`appytappy-mcp` is a local [MCP](https://modelcontextprotocol.io/) server. An
AI agent such as Claude Code starts it and uses its tools to work out how a
game exposes a value, such as a Geometry Dash level's percent. AppyTappy then
reads that value without hardcoding anything per game.

The server talks MCP over standard input and output, so it has no network
port. Every tool only reads: none writes to a game's memory or sends input.

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

The first three never open a game's process, so they are safe whatever
anti-cheat a game uses. Tool names are `area_verb`, so the definition and
config tools that come next slot in as `definition_*`, `rule_*`,
`source_test`, and `config_compile`.

## Anti-cheat safety

The server enforces the block itself, so an agent cannot talk its way past it.

- `process_attach` refuses the process of a known anti-cheat product.
- Every other process is unknown until the user allows it. The server shows a
  Windows dialog that the agent cannot answer, and it names any anti-cheat
  that is running. The default button is **No**.
- If the user chooses **No**, the agent is told not to ask again and to use
  windows and screen pixels instead.
- The server opens a process with read and query rights only.

Only allow a game that has no anti-cheat. Geometry Dash has none.

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
claude mcp add --scope user appytappy -- C:\path\to\appy-tappy\target\release\appytappy-mcp.exe
```

Check it with `claude mcp list`, or run `/mcp` inside Claude Code. Claude
Code starts the server when a session starts and stops it when the session
ends. Rebuild with the same command after pulling changes, then start a new
Claude Code session.

## Geometry Dash test

This test answers
[#4](https://github.com/TheLunchTrae/appy-tappy/issues/4): can the level
percent be read from memory through a pointer chain that survives a restart?

1. Start Geometry Dash from Steam and open any level in normal mode.
2. Start Claude Code on the same PC and ask it something like this:

   ```text
   Use the appytappy tools to find where Geometry Dash stores the current
   level percent, then find a pointer chain to it from GeometryDash.exe that
   still works after I restart the game. Tell me when to play, pause, or die.
   ```

3. Choose **Yes** in the AppyTappy dialog when Claude attaches to
   `GeometryDash.exe`.
4. Play, pause, and die when Claude asks. Each change lets it narrow the scan.
   The percent may be stored as a float, an integer, or worked out from the
   player's position, so Claude may need a few rounds.
5. Once Claude has a chain, quit and restart the game when it asks, then let it
   check the chain again.

The result to bring back to #4 is the pointer chain (module, base offset, and
offsets), its value type, and whether it survived the restart.
