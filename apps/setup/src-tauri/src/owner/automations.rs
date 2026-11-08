//! Saved automations: listing them, and saving, turning on or off, and
//! deleting one, each of which the runtime reads again once it worked.

use appytappy_config::automation_file::AutomationDef;

use crate::automations::{self, AutomationSummary};

use super::host::Host;
use super::{Changed, Owner};

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
        let saved = self
            .stores()
            .and_then(|stores| automations::save(stores, existing, automation));
        self.after(Changed::Runtime, saved)
    }

    /// Turns the automation `id` on or off.
    pub(crate) fn set_automation_enabled(&mut self, id: &str, enabled: bool) -> Result<(), String> {
        let saved = self
            .stores()
            .and_then(|stores| automations::set_enabled(stores, id, enabled));
        self.after(Changed::Runtime, saved)
    }

    /// Deletes the automation `id`.
    pub(crate) fn delete_automation(&mut self, id: &str) -> Result<(), String> {
        let deleted = self
            .stores()
            .and_then(|stores| automations::delete(stores, id));
        self.after(Changed::Runtime, deleted)
    }
}
