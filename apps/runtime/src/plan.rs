//! The saved applications and automations, compiled into what the runtime
//! runs.
//!
//! The setup app saves each application's values in its own folder and the
//! automations in one file. [`load`] reads them and keeps what can run: each
//! enabled automation becomes a rule, and each value an automation watches
//! becomes a ready check. An application is watched only when at least one
//! of its automations can run. Whatever cannot run is named in
//! [`Plan::problems`] for the log, and the rest still runs. Only a store that
//! cannot be read as a whole makes [`load`] fail.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::sync::Arc;

use appytappy_config::app_file::AppFile;
use appytappy_config::automation_file::AutomationsFile;
use appytappy_config::rule::Rule;
use appytappy_config::signal::SignalName;
use appytappy_config::store::{AppStore, AutomationStore, StoredApp};
use appytappy_config::window_match::WindowMatch;
use appytappy_sources::screen::saved::{self, SavedCheck};

use crate::lifecycle::Watchlist;

/// What the runtime runs.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Plan {
    /// The applications with at least one automation that can run, by ID.
    pub(crate) apps: BTreeMap<String, Arc<AppPlan>>,
    /// Why saved automations cannot run, one sentence each.
    pub(crate) problems: Vec<String>,
}

/// One application's part of the plan.
#[derive(Debug, PartialEq)]
pub(crate) struct AppPlan {
    /// The application's name, such as `Example App`.
    pub(crate) name: String,
    /// How to find its window.
    pub(crate) window: WindowMatch,
    /// The values its automations watch.
    pub(crate) values: Vec<ValuePlan>,
    /// One rule for each automation that can run.
    pub(crate) rules: Arc<[Rule]>,
    /// Each rule's automation's name, in the same order.
    pub(crate) automations: Vec<String>,
}

/// A value an automation watches.
#[derive(Debug, PartialEq)]
pub(crate) struct ValuePlan {
    /// The signal the rules compare: the value's ID, which is unique within
    /// its application.
    pub(crate) signal: SignalName,
    /// The value's name, such as `Progress`, for the log.
    pub(crate) name: String,
    /// Where it is read and how.
    pub(crate) check: SavedCheck,
}

impl Plan {
    /// The apps detection watches: each one's executable pattern.
    pub(crate) fn watchlist(&self) -> Watchlist {
        Watchlist::new(
            self.apps
                .iter()
                .map(|(id, app)| (id.clone(), app.window.executable.clone())),
        )
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
    Ok(compile(&stored, &file, |app, name| {
        fs::read(apps.folder(app).join(name))
    }))
}

/// An application's part of the plan while it is being compiled.
#[derive(Default)]
struct Draft {
    values: Vec<ValuePlan>,
    rules: Vec<Rule>,
    automations: Vec<String>,
}

/// Compiles `automations` against the saved applications `apps`. `picture`
/// returns the bytes of a picture in an application's folder, given the
/// application's ID and the picture's file name.
pub(crate) fn compile(
    apps: &[StoredApp],
    automations: &AutomationsFile,
    mut picture: impl FnMut(&str, &str) -> io::Result<Vec<u8>>,
) -> Plan {
    let good: Vec<(&str, &AppFile)> = apps
        .iter()
        .filter_map(|app| Some((app.id.as_str(), app.file.as_ref().ok()?)))
        .collect();
    let mut drafts: BTreeMap<&str, Draft> = BTreeMap::new();
    // Each value's check is built once, however many automations watch it.
    let mut built: BTreeMap<(&str, &str), Result<(), String>> = BTreeMap::new();
    let mut problems = Vec::new();
    for automation in automations
        .automations
        .iter()
        .filter(|automation| automation.enabled)
    {
        let when = &automation.when;
        let mut problem = |problem: &str| {
            problems.push(format!("{} cannot run: {problem}", automation.name));
        };
        if let Some(reason) = automations.problem(automation) {
            problem(&reason);
            continue;
        }
        if let Some(broken) = apps
            .iter()
            .find(|app| app.id == when.app && app.file.is_err())
        {
            problem(&format!(
                "the app.json in the {} folder cannot be read",
                broken.id
            ));
            continue;
        }
        if let Err(reason) = automation.check_against(good.iter().copied()) {
            problem(&reason);
            continue;
        }
        let Some((app_id, file, value)) = good.iter().find_map(|&(id, file)| {
            let value = file.values.iter().find(|value| value.id == when.value)?;
            (id == when.app).then_some((id, file, value))
        }) else {
            continue;
        };
        let Ok(signal) = SignalName::new(&value.id) else {
            problem(&format!("{} has an ID it cannot use", value.name));
            continue;
        };
        let rule = match automation.to_rule(signal.clone()) {
            Ok(rule) => rule,
            Err(reason) => {
                problem(&reason);
                continue;
            },
        };
        let draft = drafts.entry(app_id).or_default();
        let ready = built.entry((app_id, value.id.as_str())).or_insert_with(|| {
            let check = saved::build(&value.screen, &file.references, |name| {
                picture(app_id, name)
            })
            .map_err(|error| format!("{} of {}: {error}", value.name, file.name))?;
            draft.values.push(ValuePlan {
                signal,
                name: value.name.clone(),
                check,
            });
            Ok(())
        });
        if let Err(reason) = ready {
            problem(reason);
            continue;
        }
        draft.rules.push(rule);
        draft.automations.push(automation.name.clone());
    }
    let apps = drafts
        .into_iter()
        .filter(|(_, draft)| !draft.rules.is_empty())
        .filter_map(|(id, draft)| {
            let (_, file) = good.iter().find(|(good_id, _)| *good_id == id)?;
            let plan = AppPlan {
                name: file.name.clone(),
                window: file.window.to_match(),
                values: draft.values,
                rules: draft.rules.into(),
                automations: draft.automations,
            };
            Some((id.to_owned(), Arc::new(plan)))
        })
        .collect();
    Plan { apps, problems }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::process;

    use appytappy_config::app_file::{
        AntiCheatDef, CheckDef, ColorDef, FORMAT as APP_FORMAT, FallbackDef, MustMatchDef,
        ReferenceDef, RegionDef, SampleDef, ScreenDef, SizeDef, ValueDef, WindowDef, sample_name,
    };
    use appytappy_config::automation_file::{
        ActionDef, AutomationDef, ComparisonDef, FORMAT, MediaCommandDef, TargetDef, WhenDef,
    };
    use appytappy_config::window_match::Pattern;
    use appytappy_sources::screen::{Image, Size};

    use super::*;

    const RED: ColorDef = ColorDef {
        red: 255,
        green: 0,
        blue: 0,
    };

    fn value(id: &str, check: CheckDef) -> ValueDef {
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
            anti_cheat: AntiCheatDef::None,
            overlay: None,
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

    fn file(automations: Vec<AutomationDef>) -> AutomationsFile {
        AutomationsFile {
            format: FORMAT,
            automations,
        }
    }

    fn no_pictures(_app: &str, _name: &str) -> io::Result<Vec<u8>> {
        Err(io::Error::from(io::ErrorKind::NotFound))
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
        let plan = compile(&apps, &automations, no_pictures);
        assert_eq!(plan.problems, Vec::<String>::new());
        assert_eq!(plan.apps.keys().collect::<Vec<_>>(), ["example"]);
        let example = plan.apps.get("example").expect("planned");
        assert_eq!(example.name, "Example App");
        assert_eq!(example.window.executable, Pattern::new("Example*.exe"));
        assert_eq!(
            example.automations,
            ["Automation first", "Automation second"]
        );
        assert_eq!(example.rules.len(), 2);
        let values: Vec<&str> = example
            .values
            .iter()
            .map(|value| value.name.as_str())
            .collect();
        assert_eq!(
            values,
            ["The progress"],
            "only watched values are read, once each"
        );
        assert_eq!(
            example.values.first().map(|value| value.signal.clone()),
            SignalName::new("progress").ok()
        );
        assert_eq!(
            plan.watchlist(),
            Watchlist::new([("example".to_owned(), Pattern::new("Example*.exe"))])
        );
    }

    #[test]
    fn what_cannot_run_is_named_and_the_rest_still_runs() {
        let reference = CheckDef::Image {
            image: "icon.png".to_owned(),
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
            automation("picture", "example", "icon", TargetDef::Flag(true)),
            automation("again", "example", "icon", TargetDef::Flag(false)),
            automation("good", "example", "progress", TargetDef::Number(90.0)),
            AutomationDef {
                actions: Vec::new(),
                ..automation("empty", "example", "progress", TargetDef::Number(90.0))
            },
        ]);
        let plan = compile(&apps, &automations, no_pictures);
        assert_eq!(
            plan.problems,
            [
                "Automation gone cannot run: its application is no longer saved",
                "Automation broken cannot run: the app.json in the broken folder cannot be read",
                "Automation missing cannot run: Example App no longer has the value it watches",
                "Automation kind cannot run: The progress reads a number, so compare it with a number",
                "Automation picture cannot run: The icon of Example App: its picture icon.png cannot be read",
                "Automation again cannot run: The icon of Example App: its picture icon.png cannot be read",
                "Automation empty cannot run: add an action",
            ]
        );
        let example = plan.apps.get("example").expect("the good automation runs");
        assert_eq!(example.automations, ["Automation good"]);
        assert_eq!(example.values.len(), 1);
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
        let plan = compile(&apps, &automations, no_pictures);
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
        let reference = CheckDef::Image {
            image: "icon.png".to_owned(),
            tolerance: 0,
        };
        let apps = [stored(
            "example",
            Ok(app("Example App", vec![value("icon", reference)])),
        )];
        let automations = file(vec![automation(
            "on",
            "example",
            "icon",
            TargetDef::Flag(true),
        )]);
        let mut asked = Vec::new();
        let plan = compile(&apps, &automations, |app, name| {
            asked.push(format!("{app}/{name}"));
            Ok(picture.clone())
        });
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
            &[stored("example", Ok(example))],
            &automations,
            |app, name| {
                asked.push(format!("{app}/{name}"));
                Ok(picture.clone())
            },
        );
        assert_eq!(asked, ["example/ref-digits-1.png"]);
        assert_eq!(plan.problems, Vec::<String>::new());
        let plan = compile(&[stored("example", Ok(missing))], &automations, no_pictures);
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
