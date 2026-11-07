//! What the tray icon shows and what its menu does.
//!
//! The menu has an Enabled check mark, an Active automations submenu that
//! lists the automations that run now, and Quit. The platform
//! crate draws it; this module decides what it says.

use std::num::NonZeroU16;

use appytappy_platform::{MenuItem, TrayMenu};

use crate::model::Model;

/// The Enabled item while automations are disabled: picking it enables them.
pub(crate) const ENABLE: NonZeroU16 = NonZeroU16::MIN;

/// The Enabled item while automations are enabled: picking it disables them.
/// The item names what it does rather than flipping whatever is current, so a
/// pick from a menu that was out of date cannot undo the change it showed.
pub(crate) const DISABLE: NonZeroU16 = NonZeroU16::MIN.saturating_add(1);

/// The Quit item.
pub(crate) const QUIT: NonZeroU16 = NonZeroU16::MIN.saturating_add(2);

/// What the user asked for from the tray menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
    /// Let rules fire, or stop them.
    SetEnabled(bool),
    /// Stop the runtime.
    Quit,
}

/// The tooltip and menu for `model`.
pub(crate) fn menu(model: &Model) -> TrayMenu {
    let tooltip = if model.enabled() {
        format!("AppyTappy: {}", model.state())
    } else {
        "AppyTappy: disabled".to_owned()
    };
    let active = model.active_automations();
    let automations = if active.is_empty() {
        vec![MenuItem::Inactive("None".to_owned())]
    } else {
        active
            .into_iter()
            .map(|name| MenuItem::Note(name.to_owned()))
            .collect()
    };
    TrayMenu {
        tooltip,
        items: vec![
            MenuItem::Action {
                id: if model.enabled() { DISABLE } else { ENABLE },
                label: "Enabled".to_owned(),
                checked: model.enabled(),
            },
            MenuItem::Submenu {
                label: "Active automations".to_owned(),
                items: automations,
            },
            MenuItem::Separator,
            MenuItem::Action {
                id: QUIT,
                label: "Quit".to_owned(),
                checked: false,
            },
        ],
    }
}

/// What the menu item `id` asks for.
pub(crate) fn command(id: NonZeroU16) -> Option<Command> {
    match id {
        ENABLE => Some(Command::SetEnabled(true)),
        DISABLE => Some(Command::SetEnabled(false)),
        QUIT => Some(Command::Quit),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use appytappy_config::anti_cheat::ProcessId;

    use super::*;
    use crate::lifecycle::Change;

    #[test]
    fn the_menu_has_the_toggle_the_automations_and_quit() {
        let model = Model::default();
        assert_eq!(
            menu(&model),
            TrayMenu {
                tooltip: "AppyTappy: idle".to_owned(),
                items: vec![
                    MenuItem::Action {
                        id: DISABLE,
                        label: "Enabled".to_owned(),
                        checked: true,
                    },
                    MenuItem::Submenu {
                        label: "Active automations".to_owned(),
                        items: vec![MenuItem::Inactive("None".to_owned())],
                    },
                    MenuItem::Separator,
                    MenuItem::Action {
                        id: QUIT,
                        label: "Quit".to_owned(),
                        checked: false,
                    },
                ],
            }
        );
    }

    #[test]
    fn the_tooltip_shows_the_state_unless_disabled() {
        let mut model = Model::default();
        model.apply(
            Change::Started {
                app: "example".to_owned(),
                process: ProcessId::new(1),
                executable: "Example.exe".to_owned(),
            },
            Instant::now(),
        );
        assert_eq!(menu(&model).tooltip, "AppyTappy: attached");
        model.set_enabled(false);
        let disabled = menu(&model);
        assert_eq!(disabled.tooltip, "AppyTappy: disabled");
        assert_eq!(
            disabled.items.first(),
            Some(&MenuItem::Action {
                id: ENABLE,
                label: "Enabled".to_owned(),
                checked: false,
            })
        );
    }

    #[test]
    fn picks_enable_disable_or_quit() {
        assert_eq!(command(ENABLE), Some(Command::SetEnabled(true)));
        assert_eq!(command(DISABLE), Some(Command::SetEnabled(false)));
        assert_eq!(command(QUIT), Some(Command::Quit));
        assert_eq!(command(NonZeroU16::MAX), None);
    }
}
