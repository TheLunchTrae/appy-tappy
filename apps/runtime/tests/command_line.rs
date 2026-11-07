//! The runtime's exit code for a command line it does not understand.

use std::process::Command;

#[test]
fn an_unknown_option_exits_with_the_usage_code() {
    let status = Command::new(env!("CARGO_BIN_EXE_appytappy-runtime"))
        .args(["--verbose", "yes"])
        .status()
        .expect("run the runtime");
    assert_eq!(status.code(), Some(2));
}
