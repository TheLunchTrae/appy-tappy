//! What the tray icon shows and what its menu does.
//!
//! The menu has an Enabled check mark, an Active automations submenu that
//! lists the automations of the games attached now, and Quit. The platform
//! crate draws it; this module decides what it says.

use std::num::NonZeroU16;

use appytappy_platform::{MenuItem, Picked, TrayMenu};

use crate::model::Model;

/// The Enabled item.
const ENABLED: NonZeroU16 = NonZeroU16::MIN;

/// The Quit item.
const QUIT: NonZeroU16 = NonZeroU16::MIN.saturating_add(1);

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
    let automations = if model.automations.is_empty() {
        vec![MenuItem::Inactive("None".to_owned())]
    } else {
        model
            .automations
            .iter()
            .map(|name| MenuItem::Note(name.clone()))
            .collect()
    };
    TrayMenu {
        tooltip,
        items: vec![
            MenuItem::Action {
                id: ENABLED,
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

/// What a pick from the menu asks for, given whether automations are
/// enabled now. Enabled flips them.
pub(crate) fn command(picked: Picked, enabled: bool) -> Option<Command> {
    match picked.0 {
        id if id == ENABLED => Some(Command::SetEnabled(!enabled)),
        id if id == QUIT => Some(Command::Quit),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use appytappy_config::anti_cheat::ProcessId;

    use super::*;
    use crate::lifecycle::{Change, GameIndex};

    #[test]
    fn the_menu_has_the_toggle_the_automations_and_quit() {
        let model = Model::default();
        assert_eq!(
            menu(&model),
            TrayMenu {
                tooltip: "AppyTappy: idle".to_owned(),
                items: vec![
                    MenuItem::Action {
                        id: ENABLED,
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
        model.apply(Change::Started {
            game: GameIndex(0),
            process: ProcessId::new(1),
        });
        assert_eq!(menu(&model).tooltip, "AppyTappy: attached");
        model.set_enabled(false);
        let disabled = menu(&model);
        assert_eq!(disabled.tooltip, "AppyTappy: disabled");
        assert_eq!(
            disabled.items.first(),
            Some(&MenuItem::Action {
                id: ENABLED,
                label: "Enabled".to_owned(),
                checked: false,
            })
        );
    }

    #[test]
    fn active_automations_are_listed_by_name() {
        let mut model = Model::default();
        model.automations = vec!["Pause music on death".to_owned()];
        assert_eq!(
            menu(&model).items.get(1),
            Some(&MenuItem::Submenu {
                label: "Active automations".to_owned(),
                items: vec![MenuItem::Note("Pause music on death".to_owned())],
            })
        );
    }

    #[test]
    fn picks_toggle_enabled_or_quit() {
        assert_eq!(
            command(Picked(ENABLED), true),
            Some(Command::SetEnabled(false))
        );
        assert_eq!(
            command(Picked(ENABLED), false),
            Some(Command::SetEnabled(true))
        );
        assert_eq!(command(Picked(QUIT), true), Some(Command::Quit));
        let unknown = NonZeroU16::MAX;
        assert_eq!(command(Picked(unknown), true), None);
    }
}
