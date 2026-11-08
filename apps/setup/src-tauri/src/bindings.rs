//! The page's TypeScript bindings, `apps/setup/src/lib/bindings.ts`.
//!
//! ts-rs writes the TypeScript type of every command's arguments and answer
//! from its Rust definition, reading the serde attributes, so the page sends
//! and reads the JSON the Rust side does. The types it reaches from those
//! roots come along. This module adds what the page checks a draft against
//! before it saves: `appytappy-config`'s limits, the keys a key press can
//! name, what each kind of action does, and the default settings. The save
//! still checks everything again.
//!
//! The test renders the file and fails when the checked-in one differs, so
//! the gate's `cargo test` catches drift. With `UPDATE_BINDINGS=1` set, it
//! writes the file instead.

use std::any::TypeId;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::PathBuf;

use appytappy_config::action::{
    MAX_APP_NAME_LEN, MAX_FUNCTION_KEY, MAX_TARGET_LEN, Modifier, NAMED_KEYS,
};
use appytappy_config::app_file::{AntiCheatDef, MAX_STATES, OverlayModeDef, WindowDef};
use appytappy_config::automation_file::{
    ActionDef, AutomationDef, MAX_COOLDOWN_MS, MediaCommandDef, WhenDef,
};
use appytappy_config::settings_file::{
    FASTEST_READ_MS, MAX_TRACE_FILE_MB, MAX_TRACE_FILES, SLOWEST_READ_MS, SettingsFile,
};
use appytappy_config::signal::NO_STATE;
use serde::Serialize;
use serde_json::json;
use ts_rs::{Config, TS, TypeVisitor};

use crate::applications::AppSpec;
use crate::automations::AutomationSummary;
use crate::check_spec::{CheckSpec, ColorSpec, RegionSpec};
use crate::overlay::OverlayView;
use crate::owner::{
    AppSummary, FixSpec, Live, Reading, ReferenceSaveSpec, SaveSpec, SavedReferenceId, SavedValue,
};
use crate::recording::RecordingView;
use crate::selection::{SelectionSpec, SelectionView};
use crate::settings::SettingsView;
use crate::updates::UpdateOffer;
use crate::window_list::{WindowFit, WindowList};

/// The command to write the file again.
const REGENERATE: &str = "UPDATE_BINDINGS=1 cargo test -p appytappy-setup bindings";

/// The checked-in file.
fn path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src/lib/bindings.ts")
}

/// Every type a reached type declares, by its TypeScript name.
struct Declarations<'cfg> {
    cfg: &'cfg Config,
    seen: BTreeSet<TypeId>,
    declared: BTreeMap<String, String>,
}

impl TypeVisitor for Declarations<'_> {
    fn visit<T: TS + 'static + ?Sized>(&mut self) {
        if !self.seen.insert(TypeId::of::<T>()) {
            return;
        }
        // Only a type that ts-rs could write to a file of its own has a
        // declaration; `Vec<T>` and `Option<T>` only lead on to `T`.
        if T::output_path().is_some() {
            let name = T::name(self.cfg);
            let declaration = T::decl(self.cfg);
            let body = declaration
                .strip_prefix(&format!("type {name} = "))
                .and_then(|rest| rest.strip_suffix(';'))
                .unwrap_or_else(|| panic!("{name} is not declared as a type alias: {declaration}"));
            let text = format!(
                "{}export type {name} = DeepReadonly<{body}>;\n",
                T::docs().unwrap_or_default()
            );
            let earlier = self.declared.insert(name.clone(), text);
            assert!(
                earlier.is_none(),
                "two Rust types are both {name} in TypeScript; rename one with #[ts(rename)]"
            );
        }
        T::visit_dependencies(self);
        T::visit_generics(self);
    }
}

/// The type declarations: each command's arguments and answer, and every
/// type they reach, by name.
fn declarations() -> String {
    let cfg = Config::new();
    let mut found = Declarations {
        cfg: &cfg,
        seen: BTreeSet::new(),
        declared: BTreeMap::new(),
    };
    found.visit::<AntiCheatDef>();
    found.visit::<AppSpec>();
    found.visit::<AppSummary>();
    found.visit::<AutomationDef>();
    found.visit::<AutomationSummary>();
    found.visit::<CheckSpec>();
    found.visit::<ColorSpec>();
    found.visit::<FixSpec>();
    found.visit::<Live>();
    found.visit::<OverlayModeDef>();
    found.visit::<OverlayView>();
    found.visit::<Reading>();
    found.visit::<RecordingView>();
    found.visit::<ReferenceSaveSpec>();
    found.visit::<RegionSpec>();
    found.visit::<SaveSpec>();
    found.visit::<SavedReferenceId>();
    found.visit::<SavedValue>();
    found.visit::<SelectionSpec>();
    found.visit::<SelectionView>();
    found.visit::<SettingsFile>();
    found.visit::<SettingsView>();
    found.visit::<UpdateOffer>();
    found.visit::<WhenDef>();
    found.visit::<WindowDef>();
    found.visit::<WindowFit>();
    found.visit::<WindowList>();
    found.declared.into_values().collect::<Vec<_>>().join("\n")
}

/// `value` as its JSON text, which TypeScript reads as a literal.
fn literal(value: &impl Serialize) -> String {
    serde_json::to_string_pretty(value).expect("plain data serializes")
}

/// The name serde gives `value`, a unit variant.
fn spelled(value: &impl Serialize) -> String {
    serde_json::to_value(value)
        .expect("plain data serializes")
        .as_str()
        .expect("a unit variant serializes as a string")
        .to_owned()
}

/// One action of each kind, and of a media action, each command. A kind
/// added to [`ActionDef`] needs one here; until then the page's lookups by
/// kind fail to type-check.
fn sample_actions() -> Vec<ActionDef> {
    let mut actions: Vec<ActionDef> = [
        MediaCommandDef::Play,
        MediaCommandDef::Pause,
        MediaCommandDef::Next,
        MediaCommandDef::Previous,
    ]
    .into_iter()
    .map(|command| ActionDef::Media { command, app: None })
    .collect();
    actions.extend([
        ActionDef::Volume {
            percent: 50,
            app: None,
        },
        ActionDef::Keys {
            keys: "A".to_owned(),
        },
        ActionDef::Open {
            target: "C:\\Timer.exe".to_owned(),
        },
        ActionDef::Sound {
            file: "C:\\Ding.wav".to_owned(),
        },
    ]);
    actions
}

/// What each kind of action does, as `Action` says: whether it sends input,
/// by kind, and whether undo can put it back, by kind and for a media
/// action by command, such as `media.pause`.
fn action_facts() -> serde_json::Value {
    let mut injects_input = BTreeMap::new();
    let mut undoable = BTreeMap::new();
    for def in sample_actions() {
        let action = def.to_action().expect("each sample is a valid action");
        let kind = spelled(&serde_json::to_value(&def).expect("plain data serializes")["kind"]);
        let earlier = injects_input.insert(kind.clone(), action.injects_input());
        assert!(
            earlier.is_none_or(|earlier| earlier == action.injects_input()),
            "whether a {kind} action sends input depends on more than its kind; give the page a finer table"
        );
        let key = match &def {
            ActionDef::Media { command, .. } => format!("{kind}.{}", spelled(command)),
            _ => kind,
        };
        undoable.insert(key, action.restorable());
    }
    json!({ "injectsInput": injects_input, "undoable": undoable })
}

/// The limits and facts the page checks a draft against.
fn constants() -> String {
    let limits = json!({
        "maxAppNameBytes": MAX_APP_NAME_LEN,
        "maxCooldownMs": MAX_COOLDOWN_MS,
        "maxStates": MAX_STATES,
        "maxTargetBytes": MAX_TARGET_LEN,
        "maxTraceFileMb": MAX_TRACE_FILE_MB,
        "maxTraceFiles": MAX_TRACE_FILES,
        "fastestReadMs": FASTEST_READ_MS,
        "slowestReadMs": SLOWEST_READ_MS,
    });
    let keys = json!({
        "modifiers": Modifier::ALL.map(|modifier| modifier.to_string()),
        "named": NAMED_KEYS.map(|(name, _)| name),
        "maxFunctionKey": MAX_FUNCTION_KEY,
    });
    [
        format!(
            "/** The longest names, paths and cooldowns, the most states, and the settings' ranges the saved files take. */\nexport const LIMITS = {} as const;\n",
            literal(&limits)
        ),
        format!(
            "/** What a state check reads when its region shows none of its states. */\nexport const NO_STATE = {};\n",
            literal(&NO_STATE)
        ),
        format!(
            "/** The keys a key press action can name: modifiers in the order they go down, named keys, and function keys from F1. */\nexport const KEY_NAMES = {} as const;\n",
            literal(&keys)
        ),
        format!(
            "/** Whether each kind of action sends input, and whether undo can put it back, by kind and a media action's command. */\nexport const ACTIONS = {} as const;\n",
            literal(&action_facts())
        ),
        format!(
            "/** The settings a new install starts with. */\nexport const DEFAULT_SETTINGS: SettingsFile = {};\n",
            literal(&SettingsFile::default())
        ),
    ]
    .join("\n")
}

/// The whole file, without the spaces ts-rs leaves at the ends of lines.
fn render() -> String {
    let text = format!(
        "// Generated from the setup app's Rust types by its bindings test. Do not edit it;\n\
         // change the Rust types and run `{REGENERATE}`.\n\
         \n\
         /** `T` with every property and array readonly, all the way down. */\n\
         type DeepReadonly<T> = T extends object ? {{ readonly [K in keyof T]: DeepReadonly<T[K]> }} : T;\n\
         \n\
         {}\n\
         {}",
        declarations(),
        constants()
    );
    let mut file = text
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    file.push('\n');
    file
}

#[test]
fn bindings_match_the_rust_types() {
    let rendered = render();
    assert!(
        !rendered.contains("bigint"),
        "a u64 or i64 reaches the page as a bigint, which JSON does not carry; give its field #[ts(type = \"number\")]"
    );
    if env::var_os("UPDATE_BINDINGS").is_some() {
        fs::write(path(), &rendered).expect("bindings.ts can be written");
        return;
    }
    let saved = fs::read_to_string(path()).unwrap_or_default();
    let differs = saved
        .lines()
        .zip(rendered.lines())
        .position(|(saved, rendered)| saved != rendered)
        .map_or_else(
            || "its length".to_owned(),
            |line| {
                format!(
                    "line {}",
                    line.checked_add(1).expect("fewer lines than usize holds")
                )
            },
        );
    assert!(
        saved == rendered,
        "apps/setup/src/lib/bindings.ts differs from the Rust types at {differs}. Run `{REGENERATE}` and commit it."
    );
}
