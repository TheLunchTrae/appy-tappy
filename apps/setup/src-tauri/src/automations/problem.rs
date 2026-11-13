//! What stops an automation from running: the runtime's own answer, in the
//! page's words.

use std::collections::BTreeSet;
use std::fs;

use appytappy_config::app_file::AppFile;
use appytappy_config::automation_file::{ActionDef, AutomationDef};
use appytappy_config::store::StoredApp;
use appytappy_sources::automation::{AppReaders, Problem, Runnable};
use appytappy_sources::screen::click::ClickPictureError;

use crate::stores::Stores;
use crate::words::sentence;

use super::load;

/// Loads the applications, saying what went wrong for the user.
pub(super) fn apps(stores: &Stores) -> Result<Vec<StoredApp>, String> {
    stores.apps.load().map_err(|error| error.to_string())
}

/// What stops `automation` from running with the applications in `apps`,
/// if anything, as a sentence: the runtime's own answer, [`Runnable::new`],
/// which also builds the check of the value it watches and reads each
/// click's picture, so a value or a picture a hand edit broke shows here
/// too. An application it watches or clicks in whose file does not read is
/// named first, with why, since this app read the file and `Runnable::new`
/// takes only the applications whose files read.
pub(super) fn problem<'apps>(
    automation: &AutomationDef,
    apps: &'apps [StoredApp],
    stores: &Stores,
    readers: &mut AppReaders<'apps>,
) -> Option<String> {
    let unreadable = |id: &str| {
        apps.iter()
            .find(|app| app.id == id)
            .and_then(|app| app.file.as_ref().err())
    };
    let watched = &automation.when.app;
    if let Some(error) = unreadable(watched) {
        return Some(format!(
            "Its application's app.json, in the {watched} folder, cannot be read: {error}."
        ));
    }
    for action in &automation.actions {
        if let ActionDef::Click { app, .. } = action
            && let Some(error) = unreadable(app)
        {
            return Some(format!(
                "The app.json of the application it clicks in, in the {app} folder, cannot be \
                 read: {error}."
            ));
        }
    }
    let readable: Vec<(&str, &AppFile)> = apps
        .iter()
        .filter_map(|app| app.file.as_ref().ok().map(|file| (app.id.as_str(), file)))
        .collect();
    let problem = Runnable::new(
        automation,
        &readable,
        readers,
        |app, name| fs::read(stores.apps.folder(app).join(name)),
        |name| stores.automations.picture(name),
    )
    .err()?;
    let set_up_again = "Set up its target again.";
    Some(match problem {
        Problem::Rule(reason) => sentence(&reason),
        Problem::UnreadPicture { action, .. }
        | Problem::Picture {
            action,
            error: ClickPictureError::Read(_),
            ..
        } => format!("The picture action {action} clicks on is gone. {set_up_again}"),
        Problem::Picture {
            action,
            error: ClickPictureError::Misfit(misfit),
            ..
        } => format!("Action {action}'s target cannot be used: {misfit}. {set_up_again}"),
        Problem::Value { value, error, .. } => format!("{value} cannot be read: {error}."),
    })
}

/// The IDs of the values of the application `app` that the runtime reads:
/// those its enabled automations watch, of the automations that can run, as
/// [`problem`] decides with the runtime's own answer.
pub(crate) fn watched_values(stores: &Stores, app: &str) -> Result<BTreeSet<String>, String> {
    let file = load(stores)?;
    let apps = apps(stores)?;
    let mut readers = AppReaders::new();
    Ok(file
        .automations
        .iter()
        .filter(|automation| automation.enabled && automation.when.app == app)
        .filter(|automation| problem(automation, &apps, stores, &mut readers).is_none())
        .map(|automation| automation.when.value.clone())
        .collect())
}
