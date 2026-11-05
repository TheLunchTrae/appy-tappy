//! `appytappy-mcp`: a local MCP server that gives an AI agent read-only tools
//! to investigate how a game stores its state.
//!
//! The agent launches it and talks MCP over stdin and stdout. See
//! `docs/mcp.md` for setup and the tool workflow.

#![forbid(unsafe_code)]

mod protocol;
mod tools;

use std::io;

fn main() -> io::Result<()> {
    let mut session = tools::Session::new(appytappy_sources::HostSystem::default());
    protocol::serve(io::stdin().lock(), io::stdout().lock(), &mut session)
}
