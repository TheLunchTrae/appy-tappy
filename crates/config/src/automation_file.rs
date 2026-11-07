//! The authoring file for automations: when a value becomes true, run
//! actions in order.
//!
//! The setup app keeps every automation in one `automations.json`, apart
//! from the applications' files, because an automation is its own thing: it
//! names a value by its application's ID and the value's ID, and any number
//! of automations can use the same value. The file is JSON, so a person can
//! read it and a text diff shows a change.
//!
//! [`AutomationsFile::parse`] checks everything one automation can get wrong
//! on its own. Whether the value it names still exists, and reads the kind of
//! value its condition compares, depends on the applications' files, which
//! [`AutomationDef::check_against`] checks.

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::action::{Action, AppName, MediaCommand, MediaTarget, VolumeLevel, VolumeTarget};
use crate::app_file::{AppFile, id_from_name};
use crate::rule::{Comparison, Condition, Rearm, Restore, Rule, RuleError, Trigger};
use crate::signal::{Number, SignalName, Value};

/// The format version this build writes and reads.
pub const FORMAT: u32 = 1;

/// The longest ID [`new_automation_id`] makes, in bytes, so a numbered
/// ID still fits [`SignalName`]'s limit.
const MAX_AUTOMATION_ID_LEN: usize = 44;

/// The longest cooldown, in milliseconds: a day.
pub const MAX_COOLDOWN_MS: u32 = 86_400_000;

/// What an ID must look like, as a sentence.
const ID_RULES: &str = "must start with a lowercase letter and use only lowercase letters, \
                        digits and underscores";

/// Every automation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutomationsFile {
    /// The format version, [`FORMAT`].
    pub format: u32,
    /// The automations, in the order the user sees them.
    pub automations: Vec<AutomationDef>,
}

impl Default for AutomationsFile {
    fn default() -> Self {
        Self {
            format: FORMAT,
            automations: Vec::new(),
        }
    }
}

/// One automation: a condition on a value, the actions to run in order when
/// it becomes true, and whether to undo them when it stops being true.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutomationDef {
    /// The automation's ID, unique within the file, such as `pause_music`.
    pub id: String,
    /// The automation's name as the user sees it, such as `Pause music`.
    pub name: String,
    /// Whether the runtime runs it.
    pub enabled: bool,
    /// The condition that makes it run.
    pub when: WhenDef,
    /// The actions, run in order.
    pub actions: Vec<ActionDef>,
    /// Whether to undo the actions when the condition stops being true.
    pub undo: bool,
    /// How long after running it cannot run again, in milliseconds.
    #[serde(default)]
    pub cooldown_ms: u32,
}

/// A comparison of one saved value with a target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WhenDef {
    /// The ID of the value's application, such as `example`.
    pub app: String,
    /// The value's ID within that application, such as `progress`.
    pub value: String,
    /// How the value relates to the target.
    pub is: ComparisonDef,
    /// What the value is compared with.
    pub target: TargetDef,
}

impl WhenDef {
    /// The condition, comparing `signal` with the target.
    ///
    /// # Errors
    ///
    /// Returns the problem as a sentence the setup app can show: a target
    /// that is not a finite number, or a yes-or-no target compared by order.
    pub fn condition(&self, signal: SignalName) -> Result<Condition, String> {
        let target = match self.target {
            TargetDef::Number(number) => Value::Number(
                Number::new(number)
                    .ok_or_else(|| "the number to compare with is not finite".to_owned())?,
            ),
            TargetDef::Flag(flag) => Value::Flag(flag),
        };
        Condition::compare(signal, self.is.into(), target).map_err(|_ordered| {
            "a yes-or-no value can only be compared with \"is\" or \"is not\"".to_owned()
        })
    }
}

/// [`Comparison`] as the file spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ComparisonDef {
    /// [`Comparison::Below`].
    Below,
    /// [`Comparison::AtMost`].
    AtMost,
    /// [`Comparison::Equals`].
    Equals,
    /// [`Comparison::NotEquals`].
    NotEquals,
    /// [`Comparison::AtLeast`].
    AtLeast,
    /// [`Comparison::Above`].
    Above,
}

impl From<ComparisonDef> for Comparison {
    fn from(comparison: ComparisonDef) -> Self {
        match comparison {
            ComparisonDef::Below => Self::Below,
            ComparisonDef::AtMost => Self::AtMost,
            ComparisonDef::Equals => Self::Equals,
            ComparisonDef::NotEquals => Self::NotEquals,
            ComparisonDef::AtLeast => Self::AtLeast,
            ComparisonDef::Above => Self::Above,
        }
    }
}

/// What a value is compared with: a number, or yes or no.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TargetDef {
    /// A number, for a value a number or bar fill check reads.
    Number(f64),
    /// Yes or no, for a value a color or picture check reads.
    Flag(bool),
}

/// [`MediaCommand`] as the file spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MediaCommandDef {
    /// [`MediaCommand::Play`].
    Play,
    /// [`MediaCommand::Pause`].
    Pause,
    /// [`MediaCommand::Next`].
    Next,
    /// [`MediaCommand::Previous`].
    Previous,
}

impl From<MediaCommandDef> for MediaCommand {
    fn from(command: MediaCommandDef) -> Self {
        match command {
            MediaCommandDef::Play => Self::Play,
            MediaCommandDef::Pause => Self::Pause,
            MediaCommandDef::Next => Self::Next,
            MediaCommandDef::Previous => Self::Previous,
        }
    }
}

/// One action, as the file spells it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ActionDef {
    /// Controls media playback.
    Media {
        /// What to do.
        command: MediaCommandDef,
        /// The executable whose media session to control, such as
        /// `Player.exe`, or `None` for the session Windows considers current.
        #[serde(default)]
        app: Option<String>,
    },
    /// Sets a volume.
    Volume {
        /// The level, from 0 to 100.
        percent: u8,
        /// The executable whose volume to set, such as `Player.exe`, or
        /// `None` for the whole system.
        #[serde(default)]
        app: Option<String>,
    },
}

impl ActionDef {
    /// The action this describes.
    ///
    /// # Errors
    ///
    /// Returns the problem as a sentence: an executable name that is empty or
    /// too long, or a volume over 100.
    pub fn to_action(&self) -> Result<Action, String> {
        let app = |name: &Option<String>| {
            name.as_deref()
                .map(|name| {
                    AppName::new(name.trim()).map_err(|_invalid| {
                        "an action's application must be an executable name, such as Player.exe"
                            .to_owned()
                    })
                })
                .transpose()
        };
        Ok(match self {
            Self::Media { command, app: name } => Action::Media {
                target: app(name)?.map_or(MediaTarget::Current, MediaTarget::App),
                command: (*command).into(),
            },
            Self::Volume { percent, app: name } => Action::SetVolume {
                target: app(name)?.map_or(VolumeTarget::System, VolumeTarget::App),
                level: VolumeLevel::from_percent(*percent)
                    .ok_or_else(|| "a volume runs from 0 to 100 percent".to_owned())?,
            },
        })
    }
}

impl AutomationDef {
    /// The rule the runtime runs for this automation, reading the watched
    /// value as the signal `signal`.
    ///
    /// The caller names the signal because a value's ID is unique only
    /// within its application: two applications can each have a `progress`,
    /// so whoever feeds the runtime's readings picks a name that tells them
    /// apart.
    ///
    /// The rule runs its actions when the condition becomes true, and may run
    /// again once the condition stopped being true. With `undo`, it undoes
    /// what it changed at that point.
    ///
    /// # Errors
    ///
    /// Returns the problem as a sentence the setup app can show.
    pub fn to_rule(&self, signal: SignalName) -> Result<Rule, String> {
        if self.cooldown_ms > MAX_COOLDOWN_MS {
            return Err("the wait before it can run again is longer than a day".to_owned());
        }
        let condition = self.when.condition(signal)?;
        if self.actions.is_empty() {
            return Err("add an action".to_owned());
        }
        let actions = self
            .actions
            .iter()
            .map(ActionDef::to_action)
            .collect::<Result<Box<[Action]>, String>>()?;
        let restore = if self.undo {
            Restore::OnRelease
        } else {
            Restore::Never
        };
        Rule::new(
            Trigger::Becomes {
                condition,
                rearm: Rearm::WhenFalse,
            },
            actions,
            Duration::from_millis(self.cooldown_ms.into()),
            restore,
        )
        .map_err(|error| match error {
            RuleError::NothingToRestore => "none of these actions can be undone, because a skipped \
                                            track cannot be unskipped; turn off undo or add another \
                                            action"
                .to_owned(),
            other => other.to_string(),
        })
    }

    /// Checks everything the automation can get wrong on its own: the ID of
    /// the value it watches, and that it makes a valid rule.
    ///
    /// # Errors
    ///
    /// Returns the problem as a sentence the setup app can show.
    pub fn validate(&self) -> Result<(), String> {
        let signal = SignalName::new(&self.when.value)
            .map_err(|_invalid| format!("the ID of the value it watches {ID_RULES}"))?;
        self.to_rule(signal).map(drop)
    }

    /// Checks the automation against the saved applications: its value must
    /// exist, and its target must be the kind of value the value's check
    /// reads.
    ///
    /// # Errors
    ///
    /// Returns the problem as a sentence the setup app can show.
    pub fn check_against<'file>(
        &self,
        apps: impl IntoIterator<Item = (&'file str, &'file AppFile)>,
    ) -> Result<(), String> {
        let Some((_, app)) = apps.into_iter().find(|(id, _)| *id == self.when.app) else {
            return Err("its application is no longer saved".to_owned());
        };
        let Some(value) = app.values.iter().find(|value| value.id == self.when.value) else {
            return Err(format!("{} no longer has the value it watches", app.name));
        };
        let numeric = value.screen.check.reads_number();
        match (numeric, self.when.target) {
            (true, TargetDef::Number(_)) | (false, TargetDef::Flag(_)) => Ok(()),
            (true, TargetDef::Flag(_)) => Err(format!(
                "{} reads a number, so compare it with a number",
                value.name
            )),
            (false, TargetDef::Number(_)) => Err(format!(
                "{} reads yes or no, so compare it with yes or no",
                value.name
            )),
        }
    }

    /// Whether the automation watches the value `value` of the application
    /// `app`.
    #[must_use]
    pub fn watches(&self, app: &str, value: &str) -> bool {
        self.when.app == app && self.when.value == value
    }
}

impl AutomationsFile {
    /// Reads and checks a file's JSON.
    ///
    /// # Errors
    ///
    /// Returns an [`AutomationsError`] naming the first problem: JSON that
    /// does not fit the format, another format version, or an automation
    /// that breaks a rule in [`AutomationsFile::check`].
    pub fn parse(json: &str) -> Result<Self, AutomationsError> {
        let file: Self = serde_json::from_str(json)
            .map_err(|error| AutomationsError::Json(error.to_string()))?;
        file.check()?;
        Ok(file)
    }

    /// The file as pretty JSON, ending with a newline.
    #[must_use]
    pub fn to_json(&self) -> String {
        // The format holds only strings, numbers, booleans, and lists, and
        // [`AutomationsFile::check`] keeps every number finite, so
        // serializing cannot fail; an empty object stands in should that
        // ever change.
        let mut json = serde_json::to_string_pretty(self).unwrap_or_else(|_error| "{}".to_owned());
        json.push('\n');
        json
    }

    /// Checks the rules JSON alone cannot: the format version, a name and a
    /// valid, unique ID for each automation, and that each one makes a valid
    /// rule.
    ///
    /// # Errors
    ///
    /// Returns an [`AutomationsError`] naming the first problem.
    pub fn check(&self) -> Result<(), AutomationsError> {
        if self.format != FORMAT {
            return Err(AutomationsError::Format(self.format));
        }
        let mut ids = BTreeSet::new();
        for automation in &self.automations {
            let problem =
                |problem: String| AutomationsError::Automation(automation.id.clone(), problem);
            SignalName::new(&automation.id)
                .map_err(|_invalid| problem(format!("its ID {ID_RULES}")))?;
            if !ids.insert(automation.id.as_str()) {
                return Err(problem("another automation has the same ID".to_owned()));
            }
            if automation.name.trim().is_empty() {
                return Err(problem("it has no name".to_owned()));
            }
            automation.validate().map_err(problem)?;
        }
        Ok(())
    }

    /// The automations that watch the value `value` of the application
    /// `app`.
    pub fn watching<'file>(
        &'file self,
        app: &'file str,
        value: &'file str,
    ) -> impl Iterator<Item = &'file AutomationDef> {
        self.automations
            .iter()
            .filter(move |automation| automation.watches(app, value))
    }
}

/// The automations file cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutomationsError {
    /// The JSON does not fit the format.
    Json(String),
    /// The file has a format version this build does not know.
    Format(u32),
    /// An automation, named by its ID, breaks a rule.
    Automation(String, String),
}

impl fmt::Display for AutomationsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "the automations file is not valid: {error}"),
            Self::Format(format) => write!(
                formatter,
                "the automations file has format {format}, but this version of AppyTappy reads \
                 format {FORMAT}"
            ),
            Self::Automation(id, problem) => write!(formatter, "automation {id:?}: {problem}"),
        }
    }
}

impl Error for AutomationsError {}

/// An automation ID made from a name the user typed that `taken` does not
/// hold yet, such as `pause_music` from `Pause music`, or `pause_music_2`
/// when that is taken.
#[must_use]
pub fn new_automation_id(name: &str, taken: &[&str]) -> String {
    let base = id_from_name(name, "automation", MAX_AUTOMATION_ID_LEN);
    let base = base.as_str();
    if !taken.contains(&base) {
        return base.to_owned();
    }
    // `taken` is finite, so one of its length plus one numbers is free.
    (2_usize..=taken.len().saturating_add(2))
        .map(|number| format!("{base}_{number}"))
        .find(|candidate| !taken.contains(&candidate.as_str()))
        .unwrap_or_else(|| base.to_owned())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::app_file::{
        AntiCheatDef, CheckDef, ColorDef, FORMAT as APP_FORMAT, RegionDef, SampleDef, ScreenDef,
        SizeDef, ValueDef, WindowDef,
    };
    use crate::window_match::WindowFacts;

    fn value(id: &str, name: &str, check: CheckDef) -> ValueDef {
        ValueDef {
            id: id.to_owned(),
            name: name.to_owned(),
            screen: ScreenDef {
                window_size: SizeDef {
                    width: 100,
                    height: 50,
                },
                region: RegionDef {
                    x: 0,
                    y: 0,
                    width: 10,
                    height: 10,
                },
                check,
            },
        }
    }

    fn app() -> AppFile {
        let white = ColorDef {
            red: 255,
            green: 255,
            blue: 255,
        };
        AppFile {
            format: APP_FORMAT,
            name: "Example App".to_owned(),
            window: WindowDef::exact(WindowFacts {
                title: "Example App",
                class: "ExampleWindow",
                executable: "Example.exe",
            }),
            anti_cheat: AntiCheatDef::Unknown,
            values: vec![
                value(
                    "progress",
                    "Progress",
                    CheckDef::Number {
                        ink: white,
                        tolerance: 10,
                        samples: vec![SampleDef {
                            image: "progress-1.png".to_owned(),
                            text: "12%".to_owned(),
                        }],
                    },
                ),
                value(
                    "health",
                    "Health",
                    CheckDef::BarFill {
                        filled: white,
                        empty: ColorDef {
                            red: 0,
                            green: 0,
                            blue: 0,
                        },
                    },
                ),
                value(
                    "icon_visible",
                    "Icon visible",
                    CheckDef::Color {
                        color: white,
                        tolerance: 10,
                    },
                ),
                value(
                    "score_shown",
                    "Score shown",
                    CheckDef::Reference {
                        image: "score_shown.png".to_owned(),
                        tolerance: 10,
                    },
                ),
            ],
        }
    }

    fn progress() -> SignalName {
        SignalName::new("example_progress").expect("valid")
    }

    fn pause_music() -> AutomationDef {
        AutomationDef {
            id: "pause_music".to_owned(),
            name: "Pause music".to_owned(),
            enabled: true,
            when: WhenDef {
                app: "example".to_owned(),
                value: "progress".to_owned(),
                is: ComparisonDef::AtLeast,
                target: TargetDef::Number(90.0),
            },
            actions: vec![
                ActionDef::Media {
                    command: MediaCommandDef::Pause,
                    app: Some("Player.exe".to_owned()),
                },
                ActionDef::Volume {
                    percent: 20,
                    app: None,
                },
            ],
            undo: true,
            cooldown_ms: 0,
        }
    }

    fn file(automations: Vec<AutomationDef>) -> AutomationsFile {
        AutomationsFile {
            format: FORMAT,
            automations,
        }
    }

    fn problem(automation: AutomationDef) -> String {
        automation.validate().expect_err("broken")
    }

    #[test]
    fn a_file_reads_back_what_it_wrote() {
        let mut flag = pause_music();
        flag.id = "lower_volume".to_owned();
        flag.when.value = "icon_visible".to_owned();
        flag.when.is = ComparisonDef::Equals;
        flag.when.target = TargetDef::Flag(true);
        flag.enabled = false;
        flag.cooldown_ms = 1500;
        let written = file(vec![pause_music(), flag]);
        let json = written.to_json();
        assert!(json.ends_with("}\n"));
        assert!(json.contains("\"is\": \"atLeast\""));
        assert!(json.contains("\"target\": 90.0"));
        assert!(json.contains("\"target\": true"));
        assert!(json.contains("\"cooldownMs\": 1500"));
        assert!(json.contains("\"kind\": \"volume\""));
        assert_eq!(AutomationsFile::parse(&json), Ok(written));
        assert_eq!(AutomationsFile::default(), file(Vec::new()));
    }

    #[test]
    fn short_spellings_fill_in_their_defaults() {
        let json = r#"{"format":1,"automations":[{"id":"skip","name":"Skip","enabled":true,
            "when":{"app":"example","value":"progress","is":"below","target":5},
            "actions":[{"kind":"media","command":"next"}],"undo":false}]}"#;
        let parsed = AutomationsFile::parse(json).expect("valid");
        let [automation] = parsed.automations.as_slice() else {
            panic!("one automation");
        };
        assert_eq!(automation.cooldown_ms, 0);
        assert_eq!(automation.when.target, TargetDef::Number(5.0));
        assert_eq!(
            automation.actions,
            [ActionDef::Media {
                command: MediaCommandDef::Next,
                app: None
            }]
        );
    }

    #[test]
    fn an_automation_becomes_a_rule_that_runs_when_its_condition_starts_to_hold() {
        let rule = pause_music().to_rule(progress()).expect("valid");
        let Trigger::Becomes { condition, rearm } = rule.trigger() else {
            panic!("a becomes trigger");
        };
        assert_eq!(rearm, &Rearm::WhenFalse);
        // The rule reads the signal it was given, not the value's bare ID.
        let signal = progress();
        let reading = |raw| {
            BTreeMap::from([(
                signal.clone(),
                Value::Number(Number::new(raw).expect("finite")),
            )])
        };
        assert!(condition.holds(&reading(90.0)));
        assert!(!condition.holds(&reading(89.0)));
        assert_eq!(
            rule.actions(),
            [
                Action::Media {
                    target: MediaTarget::App(AppName::new("Player.exe").expect("valid")),
                    command: MediaCommand::Pause,
                },
                Action::SetVolume {
                    target: VolumeTarget::System,
                    level: VolumeLevel::from_percent(20).expect("valid"),
                },
            ]
        );
        assert_eq!(rule.restore(), Restore::OnRelease);
        assert_eq!(rule.cooldown(), Duration::ZERO);

        let mut kept = pause_music();
        kept.undo = false;
        kept.cooldown_ms = 2500;
        kept.actions = vec![ActionDef::Media {
            command: MediaCommandDef::Play,
            app: None,
        }];
        let rule = kept.to_rule(progress()).expect("valid");
        assert_eq!(rule.restore(), Restore::Never);
        assert_eq!(rule.cooldown(), Duration::from_millis(2500));
        assert_eq!(
            rule.actions(),
            [Action::Media {
                target: MediaTarget::Current,
                command: MediaCommand::Play,
            }]
        );
        let mut app_volume = pause_music();
        app_volume.actions = vec![ActionDef::Volume {
            percent: 0,
            app: Some(" Player.exe ".to_owned()),
        }];
        assert_eq!(
            app_volume.to_rule(progress()).expect("valid").actions(),
            [Action::SetVolume {
                target: VolumeTarget::App(AppName::new("Player.exe").expect("valid")),
                level: VolumeLevel::from_percent(0).expect("valid"),
            }]
        );
    }

    #[test]
    fn each_comparison_keeps_its_meaning() {
        let cases = [
            (ComparisonDef::Below, Comparison::Below),
            (ComparisonDef::AtMost, Comparison::AtMost),
            (ComparisonDef::Equals, Comparison::Equals),
            (ComparisonDef::NotEquals, Comparison::NotEquals),
            (ComparisonDef::AtLeast, Comparison::AtLeast),
            (ComparisonDef::Above, Comparison::Above),
        ];
        for (def, comparison) in cases {
            assert_eq!(Comparison::from(def), comparison);
        }
        let commands = [
            (MediaCommandDef::Play, MediaCommand::Play),
            (MediaCommandDef::Pause, MediaCommand::Pause),
            (MediaCommandDef::Next, MediaCommand::Next),
            (MediaCommandDef::Previous, MediaCommand::Previous),
        ];
        for (def, command) in commands {
            assert_eq!(MediaCommand::from(def), command);
        }
    }

    #[test]
    fn mistakes_come_back_as_sentences() {
        let mut no_actions = pause_music();
        no_actions.actions.clear();
        assert_eq!(problem(no_actions), "add an action");

        let mut ordered_flag = pause_music();
        ordered_flag.when.target = TargetDef::Flag(true);
        assert!(problem(ordered_flag).contains("yes-or-no"));

        let mut infinite = pause_music();
        infinite.when.target = TargetDef::Number(f64::INFINITY);
        assert!(problem(infinite).contains("not finite"));

        let mut loud = pause_music();
        loud.actions = vec![ActionDef::Volume {
            percent: 101,
            app: None,
        }];
        assert!(problem(loud).contains("0 to 100"));

        for name in ["", "   ", "Player\n.exe"] {
            let mut unnamed = pause_music();
            unnamed.actions = vec![ActionDef::Media {
                command: MediaCommandDef::Pause,
                app: Some(name.to_owned()),
            }];
            assert!(problem(unnamed).contains("executable name"), "{name:?}");
        }

        let mut skip_only = pause_music();
        skip_only.actions = vec![ActionDef::Media {
            command: MediaCommandDef::Next,
            app: None,
        }];
        assert!(problem(skip_only).contains("cannot be unskipped"));
    }

    #[test]
    fn a_file_names_the_automation_that_breaks_a_rule() {
        let other_format = AutomationsFile {
            format: 2,
            automations: Vec::new(),
        };
        assert_eq!(other_format.check(), Err(AutomationsError::Format(2)));
        assert!(
            AutomationsError::Format(2)
                .to_string()
                .contains("reads format 1")
        );

        let twice = file(vec![pause_music(), pause_music()]);
        assert_eq!(
            twice.check(),
            Err(AutomationsError::Automation(
                "pause_music".to_owned(),
                "another automation has the same ID".to_owned()
            ))
        );

        let mut unnamed = pause_music();
        unnamed.name = " ".to_owned();
        assert_eq!(
            file(vec![unnamed]).check(),
            Err(AutomationsError::Automation(
                "pause_music".to_owned(),
                "it has no name".to_owned()
            ))
        );

        let mut bad_id = pause_music();
        bad_id.id = "Pause music".to_owned();
        assert_eq!(
            file(vec![bad_id]).check(),
            Err(AutomationsError::Automation(
                "Pause music".to_owned(),
                format!("its ID {ID_RULES}")
            ))
        );

        let mut bad_value = pause_music();
        bad_value.when.value = "Progress".to_owned();
        assert_eq!(
            problem(bad_value),
            format!("the ID of the value it watches {ID_RULES}")
        );

        let mut slow = pause_music();
        slow.cooldown_ms = MAX_COOLDOWN_MS;
        assert_eq!(slow.validate(), Ok(()));
        slow.cooldown_ms = MAX_COOLDOWN_MS + 1;
        assert_eq!(
            problem(slow),
            "the wait before it can run again is longer than a day"
        );

        let mut broken = pause_music();
        broken.actions.clear();
        let error = file(vec![broken]).check().expect_err("no actions");
        assert_eq!(
            error.to_string(),
            "automation \"pause_music\": add an action"
        );

        let json = AutomationsFile::parse("{\"format\":1}").expect_err("no list");
        assert!(
            json.to_string()
                .starts_with("the automations file is not valid")
        );
        let unknown = r#"{"format":1,"automations":[],"extra":true}"#;
        assert!(matches!(
            AutomationsFile::parse(unknown),
            Err(AutomationsError::Json(_))
        ));
        let mut broken_json = file(vec![pause_music()]).to_json();
        broken_json = broken_json.replace("\"undo\": true", "\"undo\": true, \"when2\": 1");
        assert!(AutomationsFile::parse(&broken_json).is_err());
    }

    #[test]
    fn the_value_must_exist_and_read_the_kind_its_target_is() {
        let app = app();
        let apps = || [("example", &app)];
        assert_eq!(pause_music().check_against(apps()), Ok(()));

        let mut bar = pause_music();
        bar.when.value = "health".to_owned();
        assert_eq!(bar.check_against(apps()), Ok(()));
        bar.when.target = TargetDef::Flag(false);
        assert_eq!(
            bar.check_against(apps()),
            Err("Health reads a number, so compare it with a number".to_owned())
        );

        for flag in ["icon_visible", "score_shown"] {
            let mut shown = pause_music();
            shown.when.value = flag.to_owned();
            shown.when.target = TargetDef::Flag(true);
            assert_eq!(shown.check_against(apps()), Ok(()), "{flag}");
            shown.when.target = TargetDef::Number(1.0);
            assert!(
                shown
                    .check_against(apps())
                    .is_err_and(|error| error.ends_with("compare it with yes or no")),
                "{flag}"
            );
        }

        let mut gone = pause_music();
        gone.when.value = "lap".to_owned();
        assert_eq!(
            gone.check_against(apps()),
            Err("Example App no longer has the value it watches".to_owned())
        );
        let mut elsewhere = pause_music();
        elsewhere.when.app = "other".to_owned();
        assert_eq!(
            elsewhere.check_against(apps()),
            Err("its application is no longer saved".to_owned())
        );
    }

    #[test]
    fn automations_are_found_by_the_value_they_watch() {
        let mut other = pause_music();
        other.id = "other".to_owned();
        other.when.value = "icon_visible".to_owned();
        let mut elsewhere = pause_music();
        elsewhere.id = "elsewhere".to_owned();
        elsewhere.when.app = "other".to_owned();
        let all = file(vec![pause_music(), other, elsewhere]);
        let ids: Vec<&str> = all
            .watching("example", "progress")
            .map(|automation| automation.id.as_str())
            .collect();
        assert_eq!(ids, ["pause_music"]);
        assert_eq!(all.watching("example", "lap").count(), 0);
    }

    #[test]
    fn new_ids_come_from_the_name_and_avoid_taken_ones() {
        assert_eq!(new_automation_id("Pause music", &[]), "pause_music");
        assert_eq!(
            new_automation_id("Pause music", &["pause_music"]),
            "pause_music_2"
        );
        assert_eq!(
            new_automation_id("Pause music", &["pause_music", "pause_music_2"]),
            "pause_music_3"
        );
        assert_eq!(new_automation_id("", &[]), "automation");
        assert_eq!(new_automation_id("!!", &["automation"]), "automation_2");
        assert_eq!(new_automation_id("2 steps", &[]), "automation_2_steps");
        assert_eq!(new_automation_id("Value check", &[]), "value_check");
        assert_eq!(new_automation_id("Éclair", &[]), "clair");
        let long = new_automation_id(&"a".repeat(80), &[]);
        assert_eq!(long.len(), MAX_AUTOMATION_ID_LEN);
        let underscore = format!("{} b", "a".repeat(MAX_AUTOMATION_ID_LEN.saturating_sub(1)));
        assert!(!new_automation_id(&underscore, &[]).ends_with('_'));
        for id in [
            long,
            new_automation_id(&"a".repeat(80), &[&"a".repeat(44)]),
            new_automation_id("2 steps", &[]),
        ] {
            assert!(SignalName::new(&id).is_ok(), "{id}");
        }
    }
}
