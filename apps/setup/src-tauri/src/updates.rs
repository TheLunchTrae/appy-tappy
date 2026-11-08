//! Updates from GitHub Releases. AppyTappy Setup asks whether a newer
//! AppyTappy is out when it opens. The user downloads it with one click, and
//! it installs only when they agree, with nothing left unsaved.
//!
//! One installer holds both programs, so installing the setup app's update
//! updates the runtime too, and the two always come from the same release.
//! The installer stops the runtime first, which undoes what its automations
//! changed, and starts it again afterwards. The updates section of
//! `wiki/developer/Installer.md` describes the flow and the key that signs
//! each update.

use std::fmt;
use std::time::Duration;

use serde::Serialize;
use tauri::{App, AppHandle, Manager, Runtime};
use tauri_plugin_updater::{Updater, UpdaterExt};
use tokio::sync::oneshot;

use crate::commands::{Bridge, ask};
use crate::owner::{Message, Request};

/// The plugin's name, which is also its section in the configuration.
const UPDATER: &str = "updater";

/// How long a check or download may wait for the connection or for its next
/// bytes before it fails, so a stalled download does not wait forever.
const STALL: Duration = Duration::from_secs(30);

/// Whether this build checks for updates: only builds whose configuration
/// carries the updater's section, which the release workflow adds when the
/// repository has an update key.
#[derive(Debug, Clone, Copy)]
struct Updates(bool);

/// A release whose installer is downloaded and its signature checked. The
/// owner keeps it until the user installs it.
pub(crate) struct Downloaded {
    /// Its version, such as `0.2.0`.
    pub(crate) version: String,
    /// The release's `latest.json`, which brings the release back without
    /// asking GitHub again.
    pub(crate) manifest: serde_json::Value,
    pub(crate) installer: Vec<u8>,
}

impl fmt::Debug for Downloaded {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Downloaded")
            .field("version", &self.version)
            .field("installer_bytes", &self.installer.len())
            .finish_non_exhaustive()
    }
}

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

/// The updater, with limits on a stalled connection. Before the installer
/// ends the process, it has the owner close what is open, as the app's own
/// end would, so the panels keep their places.
// Needs a running Tauri app with the updater registered.
#[mutants::skip]
fn updater(app: &AppHandle) -> Result<Updater, String> {
    if !app.state::<Updates>().0 {
        return Err("This build of AppyTappy Setup does not update itself.".to_owned());
    }
    let messages = app.state::<Bridge>().0.clone();
    let handle = app.clone();
    app.updater_builder()
        .configure_client(|client| client.connect_timeout(STALL).read_timeout(STALL))
        .on_before_exit(move || {
            let (reply, answer) = oneshot::channel();
            if messages
                .send(Message::Request(Request::CloseForUpdate(reply)))
                .is_ok()
            {
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "an owner that already ended has nothing left to close"
                )]
                let _ = answer.blocking_recv();
            }
            handle.cleanup_before_exit();
        })
        .build()
        .map_err(|error| failed(&error))
}

/// A newer release the user can install.
#[derive(Debug, Serialize)]
pub(crate) struct UpdateOffer {
    /// Its version, such as `0.2.0`.
    version: String,
}

/// Whether the release downloaded is the one the user agreed to install.
fn agreed(downloaded: &str, asked: &str) -> Result<(), String> {
    if downloaded == asked {
        Ok(())
    } else {
        Err(format!(
            "AppyTappy Setup downloaded AppyTappy {downloaded}, not {asked}. Update again."
        ))
    }
}

/// The message for a failed check, download or install.
fn failed(error: &tauri_plugin_updater::Error) -> String {
    let error = error.to_string();
    format!(
        "AppyTappy Setup could not update: {}.",
        error.trim_end_matches('.')
    )
}

/// The newer release, if any. A build without the updater never has one.
// Needs a release on GitHub to answer.
#[tauri::command]
#[mutants::skip]
pub(crate) async fn check_for_update(app: AppHandle) -> Result<Option<UpdateOffer>, String> {
    if !app.state::<Updates>().0 {
        return Ok(None);
    }
    let found = updater(&app)?
        .check()
        .await
        .map_err(|error| failed(&error))?;
    Ok(found.map(|update| UpdateOffer {
        version: update.version,
    }))
}

/// Downloads the newest release's installer, checks its signature, and has
/// the owner keep it until the user installs it. That release can be newer
/// than the one offered, if one came out since, so the answer names it.
// Needs a signed release on GitHub to download.
#[tauri::command]
#[mutants::skip]
pub(crate) async fn download_update(app: AppHandle) -> Result<UpdateOffer, String> {
    let update = updater(&app)?
        .check()
        .await
        .map_err(|error| failed(&error))?
        .ok_or_else(|| "AppyTappy Setup found no newer release this time.".to_owned())?;
    let installer = update
        .download(|_chunk, _total| {}, || {})
        .await
        .map_err(|error| failed(&error))?;
    let version = update.version;
    let downloaded = Downloaded {
        version: version.clone(),
        manifest: update.raw_json,
        installer,
    };
    ask(&app, |reply| Request::KeepUpdate {
        update: downloaded,
        reply,
    })
    .await?;
    Ok(UpdateOffer { version })
}

/// Runs the downloaded installer for `version`, which closes AppyTappy Setup
/// and opens it again once it finishes. The installer runs on a blocking
/// thread, since closing what is open waits for the owner's answer.
// Needs a signed release on GitHub to install.
#[tauri::command]
#[mutants::skip]
pub(crate) async fn install_update(app: AppHandle, version: String) -> Result<(), String> {
    let downloaded = ask(&app, Request::TakeUpdate)
        .await?
        .ok_or_else(|| "AppyTappy Setup has no update downloaded. Update again.".to_owned())?;
    agreed(&downloaded.version, &version)?;
    let update = updater(&app)?
        .restore_update(downloaded.manifest)
        .map_err(|error| failed(&error))?
        .ok_or_else(|| format!("AppyTappy {version} is no longer newer than this one."))?;
    let installer = downloaded.installer;
    // On Windows the installer starts and this process exits inside the call.
    tauri::async_runtime::spawn_blocking(move || update.install(installer))
        .await
        .map_err(|_stopped| "AppyTappy Setup stopped while it updated.".to_owned())?
        .map_err(|error| failed(&error))
}

#[cfg(test)]
mod tests {
    use tauri_plugin_updater::Error;

    use super::{Downloaded, agreed, failed};

    #[test]
    fn installs_only_the_release_the_user_agreed_to() {
        assert!(agreed("0.2.0", "0.2.0").is_ok());
        let other = agreed("0.3.0", "0.2.0").expect_err("another release needs the user again");
        assert!(other.contains("0.3.0"), "{other}");
    }

    #[test]
    fn debug_output_names_the_installer_size_not_its_bytes() {
        let downloaded = Downloaded {
            version: "0.2.0".to_owned(),
            manifest: serde_json::json!({ "version": "0.2.0" }),
            installer: vec![7; 3],
        };
        let shown = format!("{downloaded:?}");
        assert!(shown.contains("0.2.0"), "{shown}");
        assert!(shown.contains("installer_bytes: 3"), "{shown}");
        assert!(!shown.contains("[7, 7, 7]"), "{shown}");
    }

    #[test]
    fn ends_a_failure_with_one_full_stop() {
        assert_eq!(
            failed(&Error::EmptyEndpoints),
            "AppyTappy Setup could not update: Updater does not have any endpoints set."
        );
        assert_eq!(
            failed(&Error::ReleaseNotFound),
            "AppyTappy Setup could not update: Could not fetch a valid release JSON from the remote."
        );
    }
}
