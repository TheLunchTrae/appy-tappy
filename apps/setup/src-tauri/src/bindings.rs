//! The page's TypeScript bindings, `apps/setup/src/lib/bindings.ts`.
//!
//! ts-rs writes the TypeScript type of every command's arguments and answer
//! from its Rust definition, reading the serde attributes, so the page sends
//! and reads the JSON the Rust side does. The types it reaches from those
//! roots come along. This module adds what the page checks a draft against
//! before it saves: `appytappy-config`'s limits, the keys a key press can
//! name, what each kind of action does, the trace categories, and the default
//! settings. The save still checks everything again.
//!
//! The page also reads a typed path, link, key press or app name itself, so
//! the user sees what to fix as they type. The file holds what the Rust
//! parsers make of a fixed list of samples, one for each way a parser accepts
//! or refuses, and a page test checks that the page reads every sample the
//! same.
//!
//! The test renders the file and fails when the checked-in one differs, so
//! the gate's `cargo test` catches drift. With `UPDATE_BINDINGS=1` set, it
//! writes the file instead.

use std::any::TypeId;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use appytappy_config::action::{
    AppName, KeyChord, MAX_APP_NAME_LEN, MAX_FUNCTION_KEY, MAX_TARGET_LEN, Modifier, NAMED_KEYS,
    OpenTarget, SoundFile,
};
use appytappy_config::app_file::{AntiCheatDef, MAX_STATES, OverlayModeDef, WindowDef};
use appytappy_config::automation_file::{
    ActionDef, AutomationDef, MAX_COOLDOWN_MS, MediaCommandDef, WhenDef,
};
use appytappy_config::settings_file::{
    FASTEST_READ_MS, MAX_TRACE_FILE_MB, MAX_TRACE_FILES, SLOWEST_READ_MS, SettingsFile,
    TraceCategory,
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

/// `value` as its JSON text, which TypeScript reads as a literal. Every
/// character beyond ASCII is escaped, so a sample's invisible or look-alike
/// character shows in the file as its code.
fn literal(value: &impl Serialize) -> String {
    let text = serde_json::to_string_pretty(value).expect("plain data serializes");
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_ascii() {
            escaped.push(character);
        } else {
            for unit in character.encode_utf16(&mut [0; 2]) {
                write!(escaped, "\\u{unit:04x}").expect("a String takes any text");
            }
        }
    }
    escaped
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

/// Every trace category, in the order the page lists them. The list is
/// checked against the variants the `TS` derive reads from
/// [`TraceCategory`] itself, so a category added there and left out here
/// fails the bindings test.
fn trace_categories() -> [TraceCategory; 5] {
    let all = [
        TraceCategory::Readings,
        TraceCategory::Changes,
        TraceCategory::Rules,
        TraceCategory::Actions,
        TraceCategory::Lifecycle,
    ];
    let listed: BTreeSet<String> = all.iter().map(spelled).collect();
    let declared: BTreeSet<String> = TraceCategory::inline(&Config::new())
        .split(" | ")
        .map(|variant| variant.trim_matches('"').to_owned())
        .collect();
    assert_eq!(
        listed, declared,
        "the trace categories in bindings.rs differ from TraceCategory's variants"
    );
    assert_eq!(listed.len(), all.len(), "a trace category is listed twice");
    all
}

/// A path of exactly `bytes` bytes, ending in `end`, with two two-byte
/// characters, so a page that counts characters instead of bytes reads it
/// as shorter than it is.
fn path_of(bytes: usize, end: &str) -> String {
    let fill = r"C:\éé"
        .len()
        .checked_add(end.len())
        .and_then(|ends| bytes.checked_sub(ends))
        .expect("long enough for its start and end");
    format!(r"C:\éé{}{end}", "a".repeat(fill))
}

/// What `parse` makes of each sample: the text it keeps, or `None` when it
/// refuses the sample.
fn cases(samples: &[String], parse: impl Fn(&str) -> Option<String>) -> serde_json::Value {
    samples
        .iter()
        .map(|text| json!({ "text": text, "read": parse(text) }))
        .collect()
}

/// The samples the parsers read, one for each way they accept or refuse,
/// with what each makes of them. They include the characters a page's own
/// reading could treat differently: a byte order mark, which is not white
/// space, and the Kelvin sign and long s, which only Unicode case folding
/// reads as `k` and `s`.
fn parse_cases() -> serde_json::Value {
    let strings = |samples: &[&str]| {
        samples
            .iter()
            .map(|&text| text.to_owned())
            .collect::<Vec<_>>()
    };
    let longest_link = format!(
        "https://éé{}",
        "a".repeat(
            MAX_TARGET_LEN
                .checked_sub("https://éé".len())
                .expect("long enough")
        )
    );
    let mut open = strings(&[
        r"C:\Tools\Timer.exe",
        "  d:/notes/today.txt ",
        "\u{a0}C:\\Timer.exe\u{3000}",
        "https://example.com/a?b=c",
        "HTTP://example.com",
        "http://x",
        r"C:\",
        "https://a\u{feff}b",
        "",
        "   ",
        "\u{feff}C:\\Timer.exe",
        "Timer.exe",
        r"Tools\Timer.exe",
        "C:Timer.exe",
        "1:/x",
        "\u{212a}:\\Timer.exe",
        "https://",
        "https://example.com/a b",
        "https://a\u{3000}b",
        "http\u{17f}://example.com",
        "ftp://example.com",
        "file:///C:/Tools/Timer.exe",
        "search-ms:query=x",
        r"\\server\share\run.bat",
        r"\\?\UNC\server\share\run.bat",
        "C:\\Tools\nTimer.exe",
        "C:\\Tools\u{85}Timer.exe",
    ]);
    open.extend([
        path_of(MAX_TARGET_LEN, ".exe"),
        path_of(MAX_TARGET_LEN.checked_add(1).expect("small"), ".exe"),
        format!("{longest_link}a"),
        longest_link,
    ]);
    let mut sound = strings(&[
        r"C:\Sounds\Ding.wav",
        " c:/sounds/ding.WAV ",
        r"C:\.wav",
        "",
        "   ",
        ".wav",
        "Ding.wav",
        r"C:\Sounds\Ding.mp3",
        r"C:\Sounds\Ding.wave",
        "https://example.com/ding.wav",
        r"\\server\sounds\ding.wav",
        "\u{212a}:\\Ding.wav",
        "\u{feff}C:\\Ding.wav",
        "C:\\Sounds\u{7}.wav",
        "C:\\\u{e9}.wa",
    ]);
    sound.extend([
        path_of(MAX_TARGET_LEN, ".wav"),
        path_of(MAX_TARGET_LEN.checked_add(1).expect("small"), ".wav"),
    ]);
    let keys = strings(&[
        "Ctrl+Shift+M",
        "shift + ctrl + m",
        "Win+Alt+5",
        "Ctrl+Shift+Alt+Win+M",
        "F9",
        "f1",
        "F24",
        "ctrl+pagedown",
        "Space",
        "a",
        "Ctrl+\u{a0}M\u{3000}",
        "",
        "Ctrl",
        "Ctrl+",
        "Ctrl+Ctrl+M",
        "M+Ctrl",
        "M+N",
        "F0",
        "F25",
        "F01",
        "F+1",
        "F256",
        "Ctrl+Shift+Alt+Win+Meta+M",
        "\u{c4}",
        "PrintScreen",
        "Bac\u{212a}space",
        "\u{17f}hift+M",
        "\u{feff}F9",
        "M+",
        "+M",
        "Ctrl++M",
        "F",
        "F9x",
        "F1+F2",
        "Up",
        "ArrowUp",
        "Control+M",
    ]);
    let mut app = strings(&[
        "Player.exe",
        " Player.exe ",
        "\u{feff}Player.exe",
        "",
        "Player\n.exe",
        "\u{7}",
        "Player\u{85}.exe",
    ]);
    let longest_app = format!(
        "éé{}",
        "a".repeat(
            MAX_APP_NAME_LEN
                .checked_sub("éé".len())
                .expect("long enough")
        )
    );
    app.extend([format!("{longest_app}a"), longest_app]);
    json!({
        "openTarget": cases(&open, |text| OpenTarget::parse(text).ok().map(|target| target.as_str().to_owned())),
        "soundFile": cases(&sound, |text| {
            SoundFile::parse(text)
                .ok()
                .map(|file| file.path().to_str().expect("read from text").to_owned())
        }),
        "keys": cases(&keys, |text| KeyChord::parse(text).ok().map(|keys| keys.to_string())),
        "appName": cases(&app, |text| AppName::new(text).ok().map(|name| name.as_str().to_owned())),
    })
}

/// The limits and facts the page checks a draft against.
fn constants() -> String {
    let limits = json!({
        "maxActionAppBytes": MAX_APP_NAME_LEN,
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
            "/**\n * The longest app name a media or volume action names and the longest path or link, the longest cooldown, the\n * most states, and the settings' ranges, as the saved files take them.\n */\nexport const LIMITS = {} as const;\n",
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
            "/** Every trace category, in the order the page lists them. */\nexport const TRACE_CATEGORIES = {} as const satisfies readonly TraceCategory[];\n",
            literal(&trace_categories())
        ),
        format!(
            "/**\n * What the Rust side makes of sample paths, links, key presses and app names: the text it keeps, or null when it\n * refuses one. The page's own checks must read each the same.\n */\nexport const PARSE_CASES = {} as const;\n",
            literal(&parse_cases())
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
        "a u64 or i64 reaches the page as a bigint, which JSON does not carry; give its field #[cfg_attr(test, ts(type = \"number\"))], or #[cfg_attr(feature = \"ts\", ts(type = \"number\"))] in appytappy-config"
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
