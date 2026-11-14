//! Saved automations: listing them, and saving, turning on or off, and
//! deleting one, each of which the runtime reads again once it worked;
//! whether the runtime that runs them runs; and asking it to run one's
//! actions once.

use appytappy_config::automation_file::AutomationDef;
use appytappy_config::store::RunOnce;

use crate::automations::{self, AutomationSummary};

use super::Owner;
use super::host::Host;

impl<H: Host> Owner<H> {
    /// Whether the runtime runs, which the Automations page and the
    /// automation editor warn about when it does not, or `None` when the
    /// check fails.
    pub(crate) fn runtime_runs(&self) -> Option<bool> {
        self.host.runtime_runs().ok()
    }

    /// The saved automations, as the Automations page lists them.
    pub(crate) fn list_automations(&self) -> Result<Vec<AutomationSummary>, String> {
        self.stores().and_then(automations::list)
    }

    /// Saves `automation` as a new automation, or over `existing`, and
    /// answers its ID.
    pub(crate) fn save_automation(
        &self,
        existing: Option<&str>,
        automation: AutomationDef,
    ) -> Result<String, String> {
        self.update_runtime(|stores| automations::save(stores, existing, automation))
    }

    /// Turns the automation `id` on or off.
    pub(crate) fn set_automation_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
        self.update_runtime(|stores| automations::set_enabled(stores, id, enabled))
    }

    /// Deletes the automation `id`.
    pub(crate) fn delete_automation(&self, id: &str) -> Result<(), String> {
        self.update_runtime(|stores| automations::delete(stores, id))
    }

    /// Asks the runtime to run the actions of the saved automation `id`
    /// once, as if its condition had just become true. The setup app runs
    /// none of them itself: it leaves the request in its file and tells
    /// the runtime, which takes it. A request that no runtime heard is
    /// taken away again, so none waits for a runtime that starts later.
    ///
    /// The answer says only that the runtime heard the request. Whether
    /// the actions ran is in the runtime's log, because the request's path
    /// brings nothing back.
    ///
    /// # Errors
    ///
    /// Returns why nothing was asked, for the user: the automation is not
    /// saved, is turned off or cannot run as saved, the request cannot be
    /// left, or no runtime heard it.
    pub(crate) fn run_automation_once(&self, id: &str) -> Result<(), String> {
        let stores = self.stores()?;
        automations::runs(stores, id)?;
        stores
            .run_once
            .save(&RunOnce::now(id.to_owned()))
            .map_err(|error| {
                format!(
                    "AppyTappy cannot ask its background part to run the actions, so nothing \
                     ran: {error}."
                )
            })?;
        let unheard = match self.host.run_once() {
            Ok(true) => return Ok(()),
            Ok(false) => "AppyTappy's background part isn't running, so nothing ran.".to_owned(),
            Err(error) => {
                format!("AppyTappy cannot reach its background part, so nothing ran: {error}.")
            },
        };
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the answer already says nothing ran, and a runtime drops a request that waited too long"
        )]
        let _ = stores.run_once.take();
        Err(unheard)
    }

    /// Removes the click pictures no saved automation names, as once the
    /// automation's flow has ended: those of a target set up for an
    /// automation that was then not saved. The runtime reads none of them, so it is
    /// not told.
    pub(crate) fn forget_unsaved_click_pictures(&self) -> Result<(), String> {
        self.stores().and_then(automations::forget_unsaved_pictures)
    }
}

#[cfg(test)]
mod tests {
    use appytappy_config::anti_cheat::AntiCheatClass;
    use appytappy_config::automation_file::AutomationDef;

    use crate::owner::test_support::{Harness, harness, with_saved_bar};
    use appytappy_config::app_file::RegionDef;

    #[test]
    fn the_runtime_runs_or_not_or_cannot_be_told() {
        let mut harness = harness(false);
        harness.owner.host.runtime = Ok(false);
        assert_eq!(harness.ask(|owner| owner.runtime_runs()), Some(false));
        harness.owner.host.runtime = Ok(true);
        assert_eq!(harness.ask(|owner| owner.runtime_runs()), Some(true));
        harness.owner.host.runtime = Err("no mutexes here".to_owned());
        assert_eq!(
            harness.ask(|owner| owner.runtime_runs()),
            None,
            "a check that fails warns of nothing"
        );
    }

    #[test]
    fn a_click_picture_no_automation_was_saved_with_is_forgotten() {
        let (mut harness, root) = with_saved_bar("owner-click-pictures");
        let region = RegionDef {
            x: 0,
            y: 0,
            width: 2,
            height: 2,
        };
        let taken = harness
            .ask(|owner| owner.add_click_picture(0, region, None))
            .expect("keeps the picture");
        let shown = |image: String| {
            move |owner: &mut crate::owner::Owner<_>| owner.click_picture_pixels(&image, 4).is_ok()
        };
        assert!(harness.ask(shown(taken.image.clone())));
        harness.forget_reloads();
        assert_eq!(
            harness.ask(|owner| owner.forget_unsaved_click_pictures()),
            Ok(())
        );
        assert!(!harness.ask(shown(taken.image)), "its file is gone");
        assert_eq!(harness.reloads(), 0, "the runtime reads none of them");
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn only_an_automation_the_runtime_runs_is_asked_for_and_a_request_unheard_is_taken_away() {
        let (mut harness, root) = with_saved_bar("owner-run-once");
        let automation = |name: &str, keys: bool| -> AutomationDef {
            let action = if keys {
                serde_json::json!({"kind": "keys", "keys": "Ctrl+M"})
            } else {
                serde_json::json!({"kind": "media", "command": "pause"})
            };
            serde_json::from_value(serde_json::json!({
                "id": "", "name": name, "enabled": true,
                "when": {"app": "example", "value": "progress_bar", "is": "below", "target": 50},
                "actions": [action], "undo": false
            }))
            .expect("the UI's spelling")
        };
        let id = harness
            .ask(move |owner| owner.save_automation(None, automation("Pause", false)))
            .expect("saves");
        let waiting = |harness: &mut Harness| {
            harness.ask(|owner| {
                owner
                    .stores()
                    .expect("stores")
                    .run_once
                    .take()
                    .expect("reads")
            })
        };
        let ask = |harness: &mut Harness, id: &str| {
            let id = id.to_owned();
            harness.ask(move |owner| owner.run_automation_once(&id))
        };
        harness.forget_reloads();

        assert_eq!(ask(&mut harness, &id), Ok(()));
        let left = waiting(&mut harness).expect("the request waits for the runtime");
        assert_eq!(left.automation, id);
        assert!(left.asked_ms > 0, "it says when it was asked");
        assert_eq!(
            harness.reloads(),
            0,
            "nothing was saved, so nothing is read again"
        );

        assert_eq!(
            ask(&mut harness, "missing"),
            Err("That automation is no longer saved.".to_owned())
        );
        harness.owner.host.hears = Ok(false);
        assert_eq!(
            ask(&mut harness, &id),
            Err("AppyTappy's background part isn't running, so nothing ran.".to_owned())
        );
        assert_eq!(
            waiting(&mut harness),
            None,
            "no request waits for a later runtime"
        );
        harness.owner.host.hears = Err("no events here".to_owned());
        assert_eq!(
            ask(&mut harness, &id),
            Err(
                "AppyTappy cannot reach its background part, so nothing ran: no events here."
                    .to_owned()
            )
        );
        assert_eq!(waiting(&mut harness), None);
        harness.owner.host.hears = Ok(true);

        let off = id.clone();
        harness
            .ask(move |owner| owner.set_automation_enabled(&off, false))
            .expect("turns off");
        assert_eq!(
            ask(&mut harness, &id),
            Err(
                "Pause is turned off, so nothing ran. Turn it on in the list of automations first."
                    .to_owned()
            )
        );
        assert_eq!(waiting(&mut harness), None, "nothing was asked");

        // Saved while the application had no anti-cheat, which then changed.
        let classed = |harness: &mut Harness, class| {
            harness
                .ask(move |owner| owner.set_anti_cheat("example", class))
                .expect("sets the class");
        };
        classed(&mut harness, AntiCheatClass::None);
        let keys = harness
            .ask(move |owner| owner.save_automation(None, automation("Mute", true)))
            .expect("saves");
        assert_eq!(ask(&mut harness, &keys), Ok(()));
        assert!(waiting(&mut harness).is_some());
        classed(&mut harness, AntiCheatClass::KernelLevel);
        let refused = ask(&mut harness, &keys).expect_err("its keys may not be pressed");
        assert!(
            refused.starts_with("Mute cannot run until it is fixed. "),
            "{refused}"
        );
        assert_eq!(waiting(&mut harness), None, "nothing was asked");
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn an_automation_change_that_worked_tells_the_runtime() {
        let (mut harness, root) = with_saved_bar("owner-automations");
        harness.forget_reloads();
        let automation = |name: &str| -> AutomationDef {
            serde_json::from_value(serde_json::json!({
                "id": "", "name": name, "enabled": true,
                "when": {"app": "example", "value": "progress_bar", "is": "below", "target": 50},
                "actions": [{"kind": "media", "command": "pause"}], "undo": false
            }))
            .expect("the UI's spelling")
        };
        let id = harness
            .ask(move |owner| owner.save_automation(None, automation("Pause")))
            .expect("saves");
        assert_eq!(harness.reloads(), 1, "the runtime reads the automations");
        assert!(
            harness
                .ask(move |owner| owner.save_automation(None, automation(" ")))
                .is_err()
        );
        assert_eq!(harness.reloads(), 0, "a save that failed tells no one");
        let off = id.clone();
        assert_eq!(
            harness.ask(move |owner| owner.set_automation_enabled(&off, false)),
            Ok(())
        );
        assert_eq!(harness.reloads(), 1, "turned off");
        assert!(
            harness
                .ask(|owner| owner.set_automation_enabled("missing", true))
                .is_err()
        );
        assert_eq!(harness.reloads(), 0, "nothing to turn on");
        let listed = harness
            .ask(|owner| owner.list_automations())
            .expect("lists");
        assert_eq!(listed.len(), 1);
        assert!(!listed[0].automation.enabled);
        assert_eq!(
            harness.ask(move |owner| owner.delete_automation(&id)),
            Ok(())
        );
        assert_eq!(harness.reloads(), 1, "deleted");
        assert_eq!(
            harness.ask(|owner| owner.list_automations()),
            Ok(Vec::new())
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }
}
