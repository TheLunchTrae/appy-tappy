//! Updates from GitHub Releases. AppyTappy Setup asks whether a newer
//! AppyTappy is out when it opens, and installs it only when the user agrees.
//!
//! One installer holds both programs, so installing the setup app's update
//! updates the runtime too, and the two always come from the same release.
//! The installer stops the runtime first, which undoes what its automations
//! changed, and starts it again afterwards. The updates section of
//! `wiki/developer/Installer.md` describes the flow and the key that signs
//! each update.

use serde::Serialize;
use tauri::{App, AppHandle, Manager, Runtime};
use tauri_plugin_updater::UpdaterExt;

/// The plugin's name, which is also its section in the configuration.
const UPDATER: &str = "updater";

/// Whether this build checks for updates: only builds whose configuration
/// carries the updater's section, which the release workflow adds when the
/// repository has an update key.
#[derive(Debug, Clone, Copy)]
struct Updates(bool);

/// Registers the updater for a build configured for it. The plugin refuses a
/// configuration without its section, so other builds go without it.
// Needs a running Tauri app, which the unit tests cannot build.
#[mutants::skip]
pub(crate) fn set_up<R: Runtime>(app: &App<R>) -> tauri::Result<()> {
    let configured = app.config().plugins.0.contains_key(UPDATER);
    if configured {
        app.handle()
            .plugin(tauri_plugin_updater::Builder::new().build())?;
    }
    app.manage(Updates(configured));
    Ok(())
}

/// A newer release the user can install.
#[derive(Debug, Serialize)]
pub(crate) struct UpdateOffer {
    /// Its version, such as `0.2.0`.
    version: String,
}

/// Whether the release the update check found now is the one the user agreed
/// to install. A newer one out since then needs the user's word again.
fn agreed(found: &str, offered: &str) -> Result<(), String> {
    if found == offered {
        Ok(())
    } else {
        Err(format!(
            "AppyTappy {found} came out after you chose to update. Update again to install it."
        ))
    }
}

/// The message for a failed check or download.
// Only the commands below, which need a release on GitHub, call it.
#[mutants::skip]
fn failed(error: &tauri_plugin_updater::Error) -> String {
    format!("AppyTappy Setup could not update: {error}.")
}

/// The newer release, if any. A build without the updater never has one.
// Needs a release on GitHub to answer.
#[tauri::command]
#[mutants::skip]
pub(crate) async fn check_for_update(app: AppHandle) -> Result<Option<UpdateOffer>, String> {
    if !app.state::<Updates>().0 {
        return Ok(None);
    }
    let updater = app.updater().map_err(|error| failed(&error))?;
    let found = updater.check().await.map_err(|error| failed(&error))?;
    Ok(found.map(|update| UpdateOffer {
        version: update.version,
    }))
}

/// Downloads `version`, checks its signature, and runs its installer, which
/// closes AppyTappy Setup and opens it again once it finishes.
// Needs a signed release on GitHub to install.
#[tauri::command]
#[mutants::skip]
pub(crate) async fn install_update(app: AppHandle, version: String) -> Result<(), String> {
    if !app.state::<Updates>().0 {
        return Err("This build of AppyTappy Setup does not update itself.".to_owned());
    }
    let updater = app.updater().map_err(|error| failed(&error))?;
    let update = updater
        .check()
        .await
        .map_err(|error| failed(&error))?
        .ok_or_else(|| format!("AppyTappy {version} is no longer available."))?;
    agreed(&update.version, &version)?;
    // On Windows the installer starts and this process exits inside the call.
    update
        .download_and_install(|_chunk, _total| {}, || {})
        .await
        .map_err(|error| failed(&error))
}

#[cfg(test)]
mod tests {
    use super::agreed;

    #[test]
    fn installs_only_the_release_the_user_agreed_to() {
        assert!(agreed("0.2.0", "0.2.0").is_ok());
        let newer = agreed("0.3.0", "0.2.0").expect_err("a newer release needs the user again");
        assert!(newer.contains("0.3.0"), "{newer}");
    }
}
