//! The runtime's exit code for a command line it does not understand.

use std::env;
use std::fs;
use std::process::{self, Command};

#[test]
fn an_unknown_option_exits_with_the_usage_code() {
    // The runtime logs the refusal, so it gets a scratch folder instead of
    // the real local app data.
    let scratch = env::temp_dir().join(format!("appytappy-command-line-{}", process::id()));
    let status = Command::new(env!("CARGO_BIN_EXE_appytappy-runtime"))
        .args(["--verbose", "yes"])
        .env("LOCALAPPDATA", &scratch)
        .status()
        .expect("run the runtime");
    #[expect(
        clippy::let_underscore_must_use,
        reason = "the folder exists only where the runtime could write its log"
    )]
    let _ = fs::remove_dir_all(&scratch);
    assert_eq!(status.code(), Some(2));
}
