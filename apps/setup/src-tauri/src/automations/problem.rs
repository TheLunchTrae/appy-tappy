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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::fs;

    use appytappy_config::automation_file::{ActionDef, ClickAtDef};
    use appytappy_config::click::InvalidClick;

    use super::*;
    use crate::automations::test_support::{clicking, draft, png, stores};
    use crate::automations::{list, save, set_enabled};

    #[test]
    fn listing_explains_automations_whose_value_went_away() {
        let (stores, folder) = stores("problems");
        save(&stores, None, draft("Pause music")).expect("saves");
        let mut app = stores.apps.load().expect("loads")[0]
            .file
            .clone()
            .expect("good");
        // A hand edit that leaves the file readable but the value not, as
        // the runtime reads it, shows on the automation that watches it.
        let mut broken = app.clone();
        for value in &mut broken.values {
            value.screen.region.width = 0;
        }
        stores.apps.save("example", &broken, &[]).expect("saves");
        assert!(
            list(&stores).expect("lists")[0]
                .problem
                .as_deref()
                .is_some_and(|problem| problem.starts_with("Icon visible cannot be read: ")),
            "{:?}",
            list(&stores)
        );
        app.values.retain(|value| value.id != "icon_visible");
        stores.apps.save("example", &app, &[]).expect("saves");
        assert_eq!(
            list(&stores).expect("lists")[0].problem.as_deref(),
            Some("Example App no longer has the value it watches.")
        );
        set_enabled(&stores, "pause_music", false).expect("turns off");
        assert_eq!(
            set_enabled(&stores, "pause_music", true),
            Err(
                "Pause music cannot run until it is fixed. Example App no longer has the value \
                 it watches."
                    .to_owned()
            )
        );
        assert!(!list(&stores).expect("lists")[0].automation.enabled);
        fs::write(stores.apps.folder("example").join("app.json"), b"{").expect("breaks");
        let error = stores.apps.load().expect("loads")[0]
            .file
            .clone()
            .expect_err("broken");
        assert_eq!(
            list(&stores).expect("lists")[0].problem,
            Some(format!(
                "Its application's app.json, in the example folder, cannot be read: {error}."
            ))
        );
        fs::remove_dir_all(folder).expect("clean up");
    }

    #[test]
    fn only_automations_that_can_run_watch_their_values() {
        let (stores, folder) = stores("watched");
        let none = BTreeSet::new();
        assert_eq!(watched_values(&stores, "example"), Ok(none.clone()));
        save(&stores, None, draft("Pause music")).expect("saves");
        let watched = watched_values(&stores, "example").expect("reads");
        assert_eq!(watched, BTreeSet::from(["icon_visible".to_owned()]));
        assert_eq!(watched_values(&stores, "other"), Ok(none.clone()));
        // A value the runtime cannot build stops the automation, so the
        // runtime reads nothing for it.
        let app = stores.apps.load().expect("loads")[0]
            .file
            .clone()
            .expect("good");
        let mut broken = app.clone();
        for value in &mut broken.values {
            value.screen.region.width = 0;
        }
        stores.apps.save("example", &broken, &[]).expect("saves");
        assert_eq!(watched_values(&stores, "example"), Ok(none.clone()));
        stores.apps.save("example", &app, &[]).expect("saves");
        assert_eq!(watched_values(&stores, "example"), Ok(watched));
        set_enabled(&stores, "pause_music", false).expect("turns off");
        assert_eq!(watched_values(&stores, "example"), Ok(none));
        fs::remove_dir_all(folder).expect("clean up");
    }

    #[test]
    fn a_click_s_picture_fits_its_area_and_holds_its_press() {
        let (stores, folder) = stores("click-fit");
        let picture = stores.automations.add_picture(&png(5)).expect("adds");
        let unreadable = stores.automations.add_picture(b"not a png").expect("adds");
        assert_eq!(
            save(&stores, None, clicking(&stores, "Unreadable", &unreadable)),
            Err("The picture action 1 clicks on is gone. Set up its target again.".to_owned())
        );
        let mut small = clicking(&stores, "Small", &picture);
        let mut off = clicking(&stores, "Off", &picture);
        if let [
            ActionDef::Click {
                at: ClickAtDef::Picture { search, .. },
                ..
            },
        ] = small.actions.as_mut_slice()
        {
            search.width = 3;
        }
        if let [
            ActionDef::Click {
                at: ClickAtDef::Picture { press, .. },
                ..
            },
        ] = off.actions.as_mut_slice()
        {
            press.x = 4;
        }
        assert_eq!(
            save(&stores, None, small),
            Err(format!(
                "Action 1's target cannot be used: {}. Set up its target again.",
                InvalidClick::LargePicture
            ))
        );
        assert_eq!(
            save(&stores, None, off),
            Err(format!(
                "Action 1's target cannot be used: {}. Set up its target again.",
                InvalidClick::PressOutside
            ))
        );
        // A picture removed outside the app shows on the list, and the
        // automation cannot be turned back on, as the runtime would refuse it.
        let mut kept = clicking(&stores, "Kept", &picture);
        kept.enabled = false;
        let id = save(&stores, None, kept).expect("saves");
        fs::remove_file(stores.automations.pictures().join(&picture)).expect("removes");
        let gone = "The picture action 1 clicks on is gone. Set up its target again.".to_owned();
        let listed = list(&stores).expect("lists");
        assert_eq!(
            listed.first().and_then(|summary| summary.problem.clone()),
            Some(gone.clone())
        );
        assert_eq!(
            set_enabled(&stores, &id, true),
            Err(format!("Kept cannot run until it is fixed. {gone}"))
        );
        fs::remove_dir_all(folder).expect("clean up");
    }

    #[test]
    fn an_application_clicked_in_whose_file_does_not_read_says_why() {
        let (stores, folder) = stores("clicked-in-broken");
        let picture = stores.automations.add_picture(&png(4)).expect("adds");
        let mut clicks = clicking(&stores, "Click", &picture);
        if let [ActionDef::Click { app, .. }] = clicks.actions.as_mut_slice() {
            "target".clone_into(app);
        }
        let target = stores.app("example").expect("saved").file;
        stores.apps.save("target", &target, &[]).expect("saves");
        let mut file = load(&stores).expect("loads");
        file.automations = vec![clicks];
        stores.automations.save(&file).expect("saves");
        assert_eq!(list(&stores).expect("lists")[0].problem, None);
        fs::write(stores.apps.folder("target").join("app.json"), b"{").expect("breaks");
        let error = stores
            .apps
            .load()
            .expect("loads")
            .into_iter()
            .find(|app| app.id == "target")
            .expect("listed")
            .file
            .expect_err("broken");
        assert_eq!(
            list(&stores).expect("lists")[0].problem,
            Some(format!(
                "The app.json of the application it clicks in, in the target folder, cannot be \
                 read: {error}."
            ))
        );
        fs::remove_dir_all(folder).expect("clean up");
    }
}
