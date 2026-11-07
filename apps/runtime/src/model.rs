//! The root model on the owner thread: which games are attached, what
//! problem, if any, stops the runtime from watching, and whether
//! automations are enabled.

use std::collections::BTreeMap;
use std::fmt;

use appytappy_config::anti_cheat::ProcessId;

use crate::lifecycle::{Change, GameIndex};

/// The runtime's state, as the log and the tray report it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum State {
    /// No watched game runs.
    Idle,
    /// At least one watched game runs and is attached.
    Attached,
    /// A problem stops the runtime from watching games: process snapshots
    /// fail. It overrides the other two states until the problem clears.
    Degraded,
}

impl fmt::Display for State {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Idle => "idle",
            Self::Attached => "attached",
            Self::Degraded => "degraded",
        })
    }
}

/// The owner thread's state.
#[derive(Debug)]
pub(crate) struct Model {
    /// The process each attached game's session follows.
    attached: BTreeMap<GameIndex, ProcessId>,
    /// The problem that degrades the runtime.
    problem: Option<String>,
    /// Whether rules may fire. Turning it off from the tray keeps detection
    /// running, so turning it back on finds the games already attached.
    enabled: bool,
    /// The names of the automations of the attached games. The engine fills
    /// it once the runtime loads the compiled config
    /// ([#8](https://github.com/TheLunchTrae/appy-tappy/issues/8)); until
    /// then it stays empty.
    pub(crate) automations: Vec<String>,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            attached: BTreeMap::new(),
            problem: None,
            enabled: true,
            automations: Vec::new(),
        }
    }
}

impl Model {
    /// Whether rules may fire.
    pub(crate) const fn enabled(&self) -> bool {
        self.enabled
    }

    /// Lets rules fire, or stops them, without stopping detection.
    pub(crate) const fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// The current state.
    pub(crate) fn state(&self) -> State {
        if self.problem.is_some() {
            State::Degraded
        } else if self.attached.is_empty() {
            State::Idle
        } else {
            State::Attached
        }
    }

    /// Applies what detection found and returns the new state when it
    /// changed.
    pub(crate) fn apply(&mut self, change: Change) -> Option<State> {
        let before = self.state();
        match change {
            Change::Started { game, process } => {
                self.attached.insert(game, process);
            },
            Change::Exited { game, .. } => {
                self.attached.remove(&game);
            },
            Change::ScanFailed(problem) => self.problem = Some(problem),
            Change::ScanRestored => self.problem = None,
        }
        let after = self.state();
        (after != before).then_some(after)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn started(game: usize) -> Change {
        Change::Started {
            game: GameIndex(game),
            process: ProcessId::new(1),
        }
    }

    fn exited(game: usize) -> Change {
        Change::Exited {
            game: GameIndex(game),
            process: ProcessId::new(1),
        }
    }

    #[test]
    fn the_runtime_is_attached_while_any_game_runs() {
        let mut model = Model::default();
        assert_eq!(model.state(), State::Idle);
        assert_eq!(model.apply(started(0)), Some(State::Attached));
        assert_eq!(model.apply(started(1)), None);
        assert_eq!(model.apply(exited(0)), None);
        assert_eq!(model.apply(exited(1)), Some(State::Idle));
    }

    #[test]
    fn a_failing_scan_degrades_the_runtime_until_it_recovers() {
        let mut model = Model::default();
        model.apply(started(0));
        assert_eq!(
            model.apply(Change::ScanFailed("denied".to_owned())),
            Some(State::Degraded)
        );
        assert_eq!(model.apply(exited(0)), None, "degraded overrides attached");
        assert_eq!(model.apply(Change::ScanRestored), Some(State::Idle));
    }

    #[test]
    fn automations_start_enabled_and_can_be_turned_off() {
        let mut model = Model::default();
        assert!(model.enabled());
        model.set_enabled(false);
        assert!(!model.enabled());
        assert_eq!(
            model.apply(started(0)),
            Some(State::Attached),
            "detection goes on"
        );
    }

    #[test]
    fn states_read_as_lowercase_words() {
        let names = [State::Idle, State::Attached, State::Degraded].map(|state| state.to_string());
        assert_eq!(names, ["idle", "attached", "degraded"]);
    }
}
