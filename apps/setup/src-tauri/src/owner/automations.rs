//! Saved automations: listing them, and saving, turning on or off, and
//! deleting one, each of which the runtime reads again once it worked.

use appytappy_config::automation_file::AutomationDef;

use crate::automations::{self, AutomationSummary};

use super::Owner;
use super::host::Host;

impl<H: Host> Owner<H> {
    /// The saved automations, as the Automations page lists them.
    pub(crate) fn list_automations(&self) -> Result<Vec<AutomationSummary>, String> {
        self.stores().and_then(automations::list)
    }

    /// Saves `automation` as a new automation, or over `existing`, and
    /// answers its ID.
    pub(crate) fn save_automation(
        &mut self,
        existing: Option<&str>,
        automation: AutomationDef,
    ) -> Result<String, String> {
        self.update_runtime(|stores| automations::save(stores, existing, automation))
    }

    /// Turns the automation `id` on or off.
    pub(crate) fn set_automation_enabled(&mut self, id: &str, enabled: bool) -> Result<(), String> {
        self.update_runtime(|stores| automations::set_enabled(stores, id, enabled))
    }

    /// Deletes the automation `id`.
    pub(crate) fn delete_automation(&mut self, id: &str) -> Result<(), String> {
        self.update_runtime(|stores| automations::delete(stores, id))
    }
}

#[cfg(test)]
mod tests {
    use appytappy_config::automation_file::AutomationDef;

    use crate::owner::test_support::with_saved_bar;

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
