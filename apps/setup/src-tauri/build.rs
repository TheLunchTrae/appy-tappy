//! Embeds the setup app's configuration, icon, and Windows manifest.
//!
//! The program embeds the web UI from `apps/setup/dist`, which only
//! `mise run ui:build` makes. On a fresh checkout that folder is missing and
//! Tauri refuses to compile, so this writes a stand-in page that says how to
//! build the UI. That keeps `cargo clippy`, `cargo test`, and the rest of the
//! Rust tasks working before the UI has been built; the UI build replaces the
//! stand-in.

use std::fs;
use std::io;
use std::path::Path;

/// The page shown when the UI has not been built.
const STAND_IN: &str = "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\
<title>AppyTappy setup</title></head><body><p>The setup UI is not built. Run \
<code>mise run ui:build</code>, then build this program again.</p></body></html>\n";

fn main() -> io::Result<()> {
    // A deleted UI build brings the stand-in back.
    println!("cargo::rerun-if-changed=../dist/index.html");
    let dist = Path::new("../dist");
    if !dist.join("index.html").exists() {
        fs::create_dir_all(dist)?;
        fs::write(dist.join("index.html"), STAND_IN)?;
    }
    tauri_build::build();
    Ok(())
}
