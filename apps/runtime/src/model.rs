//! The root model on the owner thread: which games are attached and what
//! problem, if any, stops the runtime from watching.

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
#[derive(Debug, Default)]
pub(crate) struct Model {
    /// The process each attached game's session follows.
    attached: BTreeMap<GameIndex, ProcessId>,
    /// The problem that degrades the runtime.
    problem: Option<String>,
}

impl Model {
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
    fn states_read_as_lowercase_words() {
        let names = [State::Idle, State::Attached, State::Degraded].map(|state| state.to_string());
        assert_eq!(names, ["idle", "attached", "degraded"]);
    }
}
