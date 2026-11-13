//! The saved applications and automations, compiled into what the runtime
//! runs.
//!
//! The setup app saves each application's values in its own folder and the
//! automations in one file. [`load`] reads them and keeps what can run: each
//! enabled automation that [`Runnable::new`] accepts becomes a rule, the
//! same check the setup app's save makes, and each value an automation
//! watches becomes a ready check. An application is watched only when at
//! least one of its automations can run. Whatever cannot run is named in
//! [`Plan::problems`] for the log, and the rest still runs. Only a store that
//! cannot be read as a whole makes [`load`] fail.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::sync::Arc;

use appytappy_config::app_file::{AppFile, BuildDef};
use appytappy_config::automation_file::{AutomationsFile, Compiled};
use appytappy_config::rule::Rule;
use appytappy_config::signal::SignalName;
use appytappy_config::store::{APP_FILE, AppStore, AutomationStore, StoredApp};
use appytappy_config::window_match::WindowMatch;
use appytappy_sources::automation::{AppReaders, Runnable};
use appytappy_sources::screen::saved::SavedValue;

use crate::click::ClickTargets;

/// What the runtime runs.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Plan {
    /// The applications with at least one automation that can run, by ID.
    pub(crate) apps: BTreeMap<String, Arc<AppPlan>>,
    /// Why saved automations cannot run, one sentence each.
    pub(crate) problems: Vec<String>,
}

/// One application's part of the plan: what runs, and what the log and the
/// tray call it. A reload restarts an application's automations only when
/// what runs changed, so renaming the application, a value or an
/// automation, or marking its values as checked, restarts nothing.
#[derive(Debug, PartialEq)]
pub(crate) struct AppPlan {
    /// What the log and the tray call the application and its parts.
    pub(crate) labels: Labels,
    /// What runs.
    pub(crate) runs: Runs,
}

/// What the log and the tray call an application and its values. A change
/// to them alone restarts nothing.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Labels {
    /// The application's name, such as `Example App`.
    pub(crate) name: String,
    /// Each value's name, such as `Progress`, in the order of
    /// [`Runs::values`].
    pub(crate) values: Vec<String>,
    /// The build its values were last checked against, if the setup app
    /// recorded one.
    pub(crate) checked: Option<BuildDef>,
}

/// What runs for an application. A change to it, other than to an
/// automation's name, restarts the application's automations.
#[derive(Debug, PartialEq)]
pub(crate) struct Runs {
    /// How to find its window.
    pub(crate) window: WindowMatch,
    /// The values its automations watch.
    pub(crate) values: Vec<ValuePlan>,
    /// One rule for each automation that can run, with its name.
    pub(crate) rules: Vec<RulePlan>,
    /// What placing the rules' clicks needs.
    pub(crate) clicks: Arc<ClickTargets>,
}

/// An automation that can run: its rule, and its name for the log, the
/// tray, and the trace.
///
/// Two are equal when their rules are, whatever their names, since a name
/// is a label and renaming an automation restarts nothing.
#[derive(Debug)]
pub(crate) struct RulePlan {
    /// The automation's name.
    pub(crate) automation: String,
    /// What it watches and does.
    pub(crate) rule: Rule,
}

impl PartialEq for RulePlan {
    fn eq(&self, other: &Self) -> bool {
        self.rule == other.rule
    }
}

/// A value an automation watches.
#[derive(Debug, PartialEq)]
pub(crate) struct ValuePlan {
    /// The signal the rules compare: the value's ID, which is unique within
    /// its application.
    pub(crate) signal: SignalName,
    /// Where it is read and how, at each window size.
    pub(crate) check: SavedValue,
}

impl AppPlan {
    /// The name of the value at `position`, for the log.
    pub(crate) fn value_name(&self, position: usize) -> &str {
        self.labels
            .values
            .get(position)
            .map_or("a value", String::as_str)
    }
}

impl Plan {
    /// The apps detection watches: each one's ID and its window match.
    pub(crate) fn windows(&self) -> impl Iterator<Item = (String, WindowMatch)> + '_ {
        self.apps
            .iter()
            .map(|(id, app)| (id.clone(), app.runs.window.clone()))
    }
}

/// Reads the saved files and compiles them.
///
/// # Errors
///
/// Returns why, as a sentence for the log, when the applications' folder
/// or the automations' file cannot be read as a whole, such as while
/// another program holds the file.
pub(crate) fn load(apps: &AppStore, automations: &AutomationStore) -> Result<Plan, String> {
    let stored = apps
        .load()
        .map_err(|error| format!("the applications cannot be read: {error}"))?;
    let file = automations
        .load()
        .map_err(|error| format!("the automations cannot be read: {error}"))?;
    Ok(compile(
        &stored,
        &file,
        |app, name| fs::read(apps.folder(app).join(name)),
        |name| automations.picture(name),
    ))
}

/// An application's part of the plan while it is being compiled.
struct Draft<'apps> {
    /// The application's file.
    file: &'apps AppFile,
    values: Vec<ValuePlan>,
    value_names: Vec<String>,
    rules: Vec<RulePlan>,
    clicks: ClickTargets,
}

/// Compiles `automations` against the saved applications `apps`, each
/// enabled one through [`Runnable::new`]. An application whose file does not
/// read is named in the problems. `picture` returns the bytes of a picture
/// in an application's folder, given the application's ID and the picture's
/// file name, and `click_picture` the bytes of a click picture, given its
/// file name.
pub(crate) fn compile(
    apps: &[StoredApp],
    automations: &AutomationsFile,
    mut picture: impl FnMut(&str, &str) -> io::Result<Vec<u8>>,
    mut click_picture: impl FnMut(&str) -> io::Result<Vec<u8>>,
) -> Plan {
    let mut drafts: BTreeMap<&str, Draft<'_>> = BTreeMap::new();
    let mut problems = Vec::new();
    // The applications whose files read, which automations run in.
    let mut readable: Vec<(&str, &AppFile)> = Vec::new();
    for app in apps {
        match &app.file {
            Ok(file) => readable.push((&app.id, file)),
            Err(error) => problems.push(format!(
                "the {APP_FILE} in the {} folder cannot be read: {error}",
                app.id
            )),
        }
    }
    let mut readers = AppReaders::new();
    for automation in automations
        .automations
        .iter()
        .filter(|automation| automation.enabled)
    {
        let Runnable {
            compiled:
                Compiled {
                    rule,
                    signal,
                    app_id,
                    app,
                    value,
                    targets,
                },
            check,
            pictures,
        } = match Runnable::new(
            automation,
            &readable,
            &mut readers,
            &mut picture,
            &mut click_picture,
        ) {
            Ok(runnable) => runnable,
            Err(problem) => {
                problems.push(format!("{} cannot run: {problem}", automation.name));
                continue;
            },
        };
        let draft = drafts.entry(app_id).or_insert_with(|| Draft {
            file: app,
            values: Vec::new(),
            value_names: Vec::new(),
            rules: Vec::new(),
            clicks: ClickTargets::default(),
        });
        // Each value is read once, however many automations watch it.
        if !draft.values.iter().any(|known| known.signal == signal) {
            draft.values.push(ValuePlan { signal, check });
            draft.value_names.push(value.name.clone());
        }
        draft.clicks.add(&targets, pictures);
        draft.rules.push(RulePlan {
            automation: automation.name.clone(),
            rule,
        });
    }
    let apps = drafts
        .into_iter()
        .map(|(id, draft)| {
            let file = draft.file;
            let plan = AppPlan {
                labels: Labels {
                    name: file.name.clone(),
                    values: draft.value_names,
                    checked: file.checked.as_ref().map(|checked| checked.build.clone()),
                },
                runs: Runs {
                    window: file.window.to_match(),
                    values: draft.values,
                    rules: draft.rules,
                    clicks: Arc::new(draft.clicks),
                },
            };
            (id.to_owned(), Arc::new(plan))
        })
        .collect();
    Plan { apps, problems }
}

/// Plans for the runtime's tests.
#[cfg(test)]
pub(crate) mod examples {
    use std::time::Duration;

    use appytappy_config::action::{Action, MediaCommand, MediaTarget};
    use appytappy_config::app_file::{PinDef, SizeChangesDef};
    use appytappy_config::rule::{Comparison, Condition, Restore, Rule};
    use appytappy_config::signal::{Number, SignalName, Value};
    use appytappy_config::window_match::{MustMatch, Pattern, WindowMatch};
    use appytappy_sources::screen::check::{BarFill, Check};
    use appytappy_sources::screen::saved::{SavedCheck, SavedValue};
    use appytappy_sources::screen::{Region, Rgb, Size};

    use super::*;

    pub(crate) fn signal() -> SignalName {
        SignalName::new("progress").expect("valid")
    }

    pub(crate) fn number(raw: f64) -> Value {
        Value::Number(Number::new(raw).expect("finite"))
    }

    pub(crate) fn pause() -> Box<[Action]> {
        Box::new([Action::Media {
            target: MediaTarget::Current,
            command: MediaCommand::Pause,
        }])
    }

    /// An app whose one automation pauses media while progress is at least
    /// `threshold`, and undoes it after.
    pub(crate) fn plan(name: &str, threshold: f64) -> Arc<AppPlan> {
        plan_of(name, threshold, Duration::ZERO)
    }

    /// An app whose one automation pauses media once progress is at least
    /// 90, at most once every `cooldown`.
    pub(crate) fn plan_with_cooldown(name: &str, cooldown: Duration) -> Arc<AppPlan> {
        plan_of(name, 90.0, cooldown)
    }

    fn plan_of(name: &str, threshold: f64, cooldown: Duration) -> Arc<AppPlan> {
        Arc::new(AppPlan {
            labels: Labels {
                name: name.to_owned(),
                values: vec!["Progress".to_owned()],
                checked: None,
            },
            runs: Runs {
                window: WindowMatch {
                    title: Pattern::new("*"),
                    class: Pattern::new("*"),
                    executable: Pattern::new("Example.exe"),
                    must_match: MustMatch {
                        title: false,
                        class: false,
                    },
                    fallback: false,
                },
                values: vec![value(signal())],
                rules: vec![pause_at(&format!("Pause {name}"), threshold, cooldown)],
                clicks: Arc::default(),
            },
        })
    }

    /// The automation `automation`, which pauses media while progress is at
    /// least `threshold`, at most once every `cooldown`, and undoes it after.
    pub(crate) fn pause_at(automation: &str, threshold: f64, cooldown: Duration) -> RulePlan {
        let condition =
            Condition::compare(signal(), Comparison::AtLeast, number(threshold)).expect("valid");
        RulePlan {
            automation: automation.to_owned(),
            rule: Rule::new(condition, pause(), cooldown, Restore::OnRelease).expect("valid"),
        }
    }

    /// The value `signal`, read as the fill of a bar at the top left of an
    /// 800 by 600 window.
    pub(crate) fn value(signal: SignalName) -> ValuePlan {
        let black = Rgb {
            red: 0,
            green: 0,
            blue: 0,
        };
        let white = Rgb {
            red: 255,
            green: 255,
            blue: 255,
        };
        ValuePlan {
            signal,
            check: SavedValue {
                drawn: SavedCheck {
                    window_size: Size {
                        width: 800,
                        height: 600,
                    },
                    region: Region::new(0, 0, 10, 2).expect("not empty"),
                    check: Check::BarFill(BarFill::new(white, black).expect("different")),
                },
                fixes: Vec::new(),
                pin: PinDef::TopLeft,
                size_changes: SizeChangesDef::Often,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::process;

    use appytappy_config::app_file::{
        AntiCheatDef, CheckDef, ColorDef, FORMAT as APP_FORMAT, FallbackDef, MustMatchDef,
        OriginDef, ReferenceDef, RegionDef, SampleDef, ScreenDef, SizeChangesDef, SizeDef,
        StateDef, ValueDef, WindowDef,
    };
    use appytappy_config::automation_file::{
        ActionDef, AutomationDef, ComparisonDef, FORMAT, MediaCommandDef, TargetDef, WhenDef,
    };
    use appytappy_config::names::sample_name;
    use appytappy_config::window_match::Pattern;
    use appytappy_sources::screen::{Image, Size};

    use super::*;

    const RED: ColorDef = ColorDef {
        red: 255,
        green: 0,
        blue: 0,
    };

    fn value(id: &str, check: CheckDef) -> ValueDef {
        // Setup guidance the plan never reads: each check that may have a
        // reading area has one here, and plans as it does without.
        let reading_area = check.reads_anywhere();
        ValueDef {
            id: id.to_owned(),
            name: format!("The {id}"),
            screen: ScreenDef {
                window_size: SizeDef {
                    width: 800,
                    height: 600,
                },
                region: RegionDef {
                    x: 0,
                    y: 0,
                    width: 4,
                    height: 2,
                },
                check,
                pin: None,
                reading_area,
                fixes: Vec::new(),
            },
        }
    }

    fn app(name: &str, values: Vec<ValueDef>) -> AppFile {
        AppFile {
            format: APP_FORMAT,
            name: name.to_owned(),
            window: WindowDef {
                title: "*".to_owned(),
                class: "*".to_owned(),
                executable: "Example*.exe".to_owned(),
                must_match: MustMatchDef {
                    title: false,
                    class: false,
                },
                fallback: FallbackDef::None,
            },
            size_changes: SizeChangesDef::Often,
            anti_cheat: AntiCheatDef::None,
            overlay: None,
            checked: None,
            origin: OriginDef::ByHand,
            values,
            references: Vec::new(),
        }
    }

    fn stored(id: &str, file: Result<AppFile, String>) -> StoredApp {
        StoredApp {
            id: id.to_owned(),
            file,
        }
    }

    fn bar() -> CheckDef {
        CheckDef::BarFill {
            filled: RED,
            empty: ColorDef {
                red: 0,
                green: 0,
                blue: 0,
            },
        }
    }

    fn icon() -> CheckDef {
        CheckDef::Color {
            color: RED,
            tolerance: 10,
        }
    }

    fn automation(id: &str, app: &str, value: &str, target: TargetDef) -> AutomationDef {
        AutomationDef {
            id: id.to_owned(),
            name: format!("Automation {id}"),
            enabled: true,
            when: WhenDef {
                app: app.to_owned(),
                value: value.to_owned(),
                is: ComparisonDef::Equals,
                target,
            },
            actions: vec![ActionDef::Media {
                command: MediaCommandDef::Pause,
                app: None,
            }],
            undo: true,
            cooldown_ms: 0,
        }
    }

    fn state(name: &str) -> TargetDef {
        TargetDef::State(name.to_owned())
    }

    fn file(automations: Vec<AutomationDef>) -> AutomationsFile {
        AutomationsFile {
            format: FORMAT,
            automations,
        }
    }

    fn no_pictures(_app: &str, _name: &str) -> io::Result<Vec<u8>> {
        Err(io::Error::from(io::ErrorKind::NotFound))
    }

    fn no_click_pictures(_name: &str) -> io::Result<Vec<u8>> {
        Err(io::Error::from(io::ErrorKind::NotFound))
    }

    /// The names of `app`'s automations, in the order of its rules.
    fn automation_names(app: &AppPlan) -> Vec<&str> {
        app.runs
            .rules
            .iter()
            .map(|rule| rule.automation.as_str())
            .collect()
    }

    #[test]
    fn automations_run_the_same_when_only_their_names_differ() {
        let pause = examples::pause_at("Pause", 90.0, std::time::Duration::ZERO);
        let renamed = examples::pause_at("Renamed", 90.0, std::time::Duration::ZERO);
        let lower = examples::pause_at("Pause", 80.0, std::time::Duration::ZERO);
        assert_ne!(pause.automation, renamed.automation);
        assert_eq!(pause, renamed);
        assert_eq!(renamed, pause, "both ways");
        assert_ne!(pause, lower);
        assert_ne!(lower, pause, "both ways");
        let renamed_lower = examples::pause_at("Renamed", 80.0, std::time::Duration::ZERO);
        assert_ne!(pause, renamed_lower);
        assert_ne!(renamed_lower, pause);
    }

    #[test]
    fn each_enabled_automation_becomes_a_rule_of_its_application() {
        let apps = [
            stored(
                "example",
                Ok(app(
                    "Example App",
                    vec![value("progress", bar()), value("icon", icon())],
                )),
            ),
            stored("unused", Ok(app("Unused App", vec![value("icon", icon())]))),
        ];
        let mut off = automation("off", "example", "icon", TargetDef::Flag(true));
        off.enabled = false;
        let automations = file(vec![
            automation("first", "example", "progress", TargetDef::Number(90.0)),
            off,
            automation("second", "example", "progress", TargetDef::Number(10.0)),
        ]);
        let plan = compile(&apps, &automations, no_pictures, no_click_pictures);
        assert_eq!(plan.problems, Vec::<String>::new());
        assert_eq!(plan.apps.keys().collect::<Vec<_>>(), ["example"]);
        let example = plan.apps.get("example").expect("planned");
        assert_eq!(example.labels.name, "Example App");
        assert_eq!(example.runs.window.executable, Pattern::new("Example*.exe"));
        assert_eq!(
            automation_names(example),
            ["Automation first", "Automation second"]
        );
        let values: Vec<&str> = example.labels.values.iter().map(String::as_str).collect();
        assert_eq!(
            values,
            ["The progress"],
            "only watched values are read, once each"
        );
        assert_eq!(
            example
                .runs
                .values
                .first()
                .map(|value| value.signal.clone()),
            SignalName::new("progress").ok()
        );
        assert_eq!(
            plan.windows().collect::<Vec<_>>(),
            [("example".to_owned(), example.runs.window.clone())]
        );
    }

    #[test]
    fn what_cannot_run_is_named_and_the_rest_still_runs() {
        let reference = CheckDef::State {
            states: vec![StateDef {
                name: "Shown".to_owned(),
                image: "icon.png".to_owned(),
            }],
            tolerance: 0,
        };
        let apps = [
            stored("broken", Err("not JSON".to_owned())),
            stored(
                "example",
                Ok(app(
                    "Example App",
                    vec![value("progress", bar()), value("icon", reference)],
                )),
            ),
        ];
        let automations = file(vec![
            automation("gone", "removed", "progress", TargetDef::Number(1.0)),
            automation("broken", "broken", "progress", TargetDef::Number(1.0)),
            automation("missing", "example", "speed", TargetDef::Number(1.0)),
            automation("kind", "example", "progress", TargetDef::Flag(true)),
            automation("picture", "example", "icon", state("Shown")),
            automation("again", "example", "icon", state("none")),
            automation("good", "example", "progress", TargetDef::Number(90.0)),
            // The runtime never reads an automation's ID, so one that a hand
            // edit broke, and repeated, still runs.
            AutomationDef {
                id: "Good".to_owned(),
                ..automation("good", "example", "progress", TargetDef::Number(80.0))
            },
            AutomationDef {
                actions: Vec::new(),
                ..automation("empty", "example", "progress", TargetDef::Number(90.0))
            },
        ]);
        let plan = compile(&apps, &automations, no_pictures, no_click_pictures);
        assert_eq!(
            plan.problems,
            [
                "the app.json in the broken folder cannot be read: not JSON",
                "Automation gone cannot run: its application removed is no longer saved or cannot \
                 be read",
                "Automation broken cannot run: its application broken is no longer saved or \
                 cannot be read",
                "Automation missing cannot run: Example App no longer has the value it watches",
                "Automation kind cannot run: The progress reads a number, so compare it with a number",
                "Automation picture cannot run: The icon of Example App: its picture icon.png cannot be read",
                "Automation again cannot run: The icon of Example App: its picture icon.png cannot be read",
                "Automation empty cannot run: add an action",
            ]
        );
        let example = plan.apps.get("example").expect("the good automation runs");
        assert_eq!(
            automation_names(example),
            ["Automation good", "Automation good"]
        );
        assert_eq!(example.runs.values.len(), 1);
    }

    #[test]
    fn keys_are_pressed_only_into_an_application_without_anti_cheat() {
        let mut protected = app("Protected App", vec![value("progress", bar())]);
        protected.anti_cheat = AntiCheatDef::KernelLevel;
        let apps = [
            stored("free", Ok(app("Free App", vec![value("progress", bar())]))),
            stored("protected", Ok(protected)),
        ];
        let press = |id: &str, app: &str| {
            let mut automation = automation(id, app, "progress", TargetDef::Number(90.0));
            automation.actions = vec![ActionDef::Keys {
                keys: "Ctrl+Shift+M".to_owned(),
            }];
            automation.undo = false;
            automation
        };
        let automations = file(vec![press("free", "free"), press("protected", "protected")]);
        let plan = compile(&apps, &automations, no_pictures, no_click_pictures);
        assert_eq!(
            plan.problems,
            [
                "Automation protected cannot run: it presses keys, which anti-cheat can treat as \
                 cheating, so it can run only once Protected App's anti-cheat is set to none"
            ]
        );
        assert_eq!(plan.apps.keys().collect::<Vec<_>>(), ["free"]);
    }

    #[test]
    fn clicks_keep_their_targets_and_pictures() {
        use appytappy_config::automation_file::{
            ClickAtDef, ClickButtonDef, ClickDeliveryDef, CornerDef, PointDef,
        };
        use appytappy_config::click::ClickAt;
        use appytappy_sources::screen::click::ClickSearch;

        let apps = [
            stored(
                "example",
                Ok(app("Example App", vec![value("progress", bar())])),
            ),
            stored("target", Ok(app("Target App", Vec::new()))),
        ];
        let click = |id: &str, at: ClickAtDef| {
            let mut automation = automation(id, "example", "progress", TargetDef::Number(90.0));
            automation.actions = vec![ActionDef::Click {
                app: "target".to_owned(),
                button: ClickButtonDef::Left,
                delivery: ClickDeliveryDef::Cursor,
                return_focus: true,
                at,
            }];
            automation.undo = false;
            automation
        };
        let spot = ClickAtDef::Spot {
            corner: CornerDef::TopLeft,
            x: 1,
            y: 1,
        };
        let picture = |image: &str| ClickAtDef::Picture {
            image: image.to_owned(),
            window_size: SizeDef {
                width: 800,
                height: 600,
            },
            search: RegionDef {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
            },
            press: PointDef { x: 0, y: 0 },
            tolerance: 10,
        };
        let png = Image::new(
            Size {
                width: 2,
                height: 2,
            },
            vec![9; 16],
        )
        .expect("matching length")
        .encode_png()
        .expect("encodes");
        let automations = file(vec![
            click("spot", spot),
            click("picture", picture("button-1.png")),
            click("missing", picture("gone-1.png")),
        ]);
        let mut asked = Vec::new();
        let plan = compile(&apps, &automations, no_pictures, |name| {
            asked.push(name.to_owned());
            if name == "button-1.png" {
                Ok(png.clone())
            } else {
                Err(io::Error::from(io::ErrorKind::NotFound))
            }
        });
        assert_eq!(asked, ["button-1.png", "gone-1.png"]);
        assert_eq!(plan.problems.len(), 1, "{:?}", plan.problems);
        assert!(
            plan.problems[0].starts_with(
                "Automation missing cannot run: its picture gone-1.png cannot be read: "
            ),
            "{:?}",
            plan.problems
        );
        let example = &plan.apps["example"];
        assert_eq!(
            automation_names(example),
            ["Automation spot", "Automation picture"]
        );
        let mut expected = ClickTargets::default();
        let target = app("Target App", Vec::new());
        let Ok(ClickAt::Picture(wanted)) = picture("button-1.png").to_at() else {
            panic!("a picture click");
        };
        let search = ClickSearch::new(&wanted, &png).expect("fits");
        expected.add(&[("target", &target)], [(wanted, search)]);
        assert_eq!(*example.runs.clicks, expected);

        let only_missing = file(vec![click("missing", picture("gone-1.png"))]);
        let plan = compile(&apps, &only_missing, no_pictures, no_click_pictures);
        assert_eq!(plan.apps.len(), 0, "a value only it watches is not read");
    }

    #[test]
    fn pictures_come_from_the_applications_folder() {
        let picture = Image::new(
            appytappy_sources::screen::Size {
                width: 4,
                height: 2,
            },
            [0, 0, 255, 255].repeat(8),
        )
        .expect("matching length")
        .encode_png()
        .expect("encodes");
        let reference = CheckDef::State {
            states: vec![StateDef {
                name: "Shown".to_owned(),
                image: "icon.png".to_owned(),
            }],
            tolerance: 0,
        };
        let apps = [stored(
            "example",
            Ok(app("Example App", vec![value("icon", reference)])),
        )];
        let automations = file(vec![automation("on", "example", "icon", state("Shown"))]);
        let mut asked = Vec::new();
        let plan = compile(
            &apps,
            &automations,
            |app, name| {
                asked.push(format!("{app}/{name}"));
                Ok(picture.clone())
            },
            no_click_pictures,
        );
        assert_eq!(asked, ["example/icon.png"]);
        assert_eq!(plan.problems, Vec::<String>::new());
        assert!(plan.apps.contains_key("example"));
    }

    #[test]
    fn a_number_reads_with_its_applications_reference() {
        let red = [0, 0, 255, 255];
        let gray = [40, 40, 40, 255];
        let picture = Image::new(
            Size {
                width: 3,
                height: 1,
            },
            [red, gray, red].concat(),
        )
        .expect("matching length")
        .encode_png()
        .expect("encodes");
        let count = CheckDef::Number {
            reference: "digits".to_owned(),
            whole: true,
            characters: None,
        };
        let mut example = app("Example App", vec![value("count", count)]);
        example.references.push(ReferenceDef {
            id: "digits".to_owned(),
            name: "Digits".to_owned(),
            ink: RED,
            tolerance: 10,
            samples: vec![SampleDef {
                image: sample_name("digits", 1),
                text: "11".to_owned(),
            }],
            expects: None,
        });
        let mut missing = example.clone();
        missing.references.clear();
        let automations = file(vec![automation(
            "on",
            "example",
            "count",
            TargetDef::Number(11.0),
        )]);
        let mut asked = Vec::new();
        let plan = compile(
            &[stored("example", Ok(example.clone()))],
            &automations,
            |app, name| {
                asked.push(format!("{app}/{name}"));
                Ok(picture.clone())
            },
            no_click_pictures,
        );
        assert_eq!(asked, ["example/ref-digits-1.png"]);
        assert_eq!(plan.problems, Vec::<String>::new());
        // What the reference expects is setup guidance. Whatever a file
        // holds there, even text no setup app would save, the file reads
        // and the plan is the same.
        for expected in ["0123456789", "11 ", "", &"x".repeat(5000)] {
            let mut guided = example.clone();
            guided.references[0].expects = Some(expected.to_owned());
            let loaded = AppFile::parse(&guided.to_json()).map_err(|error| error.to_string());
            let guided = compile(
                &[stored("example", loaded)],
                &automations,
                |_, _| Ok(picture.clone()),
                no_click_pictures,
            );
            assert_eq!(guided, plan, "{expected:?}");
        }
        let plan = compile(
            &[stored("example", Ok(missing))],
            &automations,
            no_pictures,
            no_click_pictures,
        );
        assert_eq!(
            plan.problems,
            [
                "Automation on cannot run: The count of Example App: its reference digits is not saved"
            ]
        );
    }

    #[test]
    fn loading_reads_the_stores_and_reports_unreadable_files() {
        let root = env::temp_dir().join(format!("appytappy-plan-{}", process::id()));
        let apps = AppStore::new(root.join("apps"));
        let automations = AutomationStore::new(root.join("automations.json"));
        assert_eq!(
            load(&apps, &automations),
            Ok(Plan::default()),
            "nothing saved yet"
        );
        apps.save(
            "example",
            &app("Example App", vec![value("progress", bar())]),
            &[],
        )
        .expect("save the app");
        automations
            .save(&file(vec![automation(
                "on",
                "example",
                "progress",
                TargetDef::Number(5.0),
            )]))
            .expect("save the automations");
        let plan = load(&apps, &automations).expect("loads");
        assert!(plan.apps.contains_key("example"), "{plan:?}");
        fs::write(root.join("automations.json"), "not JSON").expect("break the file");
        let broken = load(&apps, &automations).expect_err("unreadable");
        assert!(
            broken.starts_with("the automations cannot be read: "),
            "{broken:?}"
        );
        fs::remove_dir_all(&root).expect("clean up");
        // A file where the store's folder should be. Windows calls a path
        // through a file missing, which reads as an empty store, so the store
        // is the file itself.
        fs::write(&root, "a file where the folder should be").expect("block the folder");
        let blocked = load(&AppStore::new(root.clone()), &automations).expect_err("unreadable");
        fs::remove_file(&root).expect("clean up");
        assert!(
            blocked.starts_with("the applications cannot be read: "),
            "{blocked:?}"
        );
    }
}
