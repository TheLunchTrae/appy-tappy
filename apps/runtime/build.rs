//! Links the runtime's Windows resources, from `runtime.rc`, and the Visual
//! C++ runtime into the program. Builds for other systems skip both.

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo::rerun-if-changed=runtime.rc");
    println!("cargo::rerun-if-changed=../setup/src-tauri/icons/icon.ico");
    // A Windows build without a resource compiler fails, rather than ship a
    // runtime with no icon.
    embed_resource::compile("runtime.rc", embed_resource::NONE)
        .manifest_required()
        .map_err(|error| error.to_string())?;
    // The setup app's Tauri build does the same, so neither program needs
    // the Visual C++ Redistributable.
    static_vcruntime::metabuild();
    Ok(())
}
