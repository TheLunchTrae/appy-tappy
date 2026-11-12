//! The page's TypeScript bindings, `apps/setup/src/lib/bindings.ts`, and its
//! command functions, `apps/setup/src/lib/api.ts`.
//!
//! ts-rs writes the TypeScript type of every command's arguments and answer
//! from its Rust definition, reading the serde attributes, so the page sends
//! and reads the JSON the Rust side does. The types it reaches from those
//! roots come along. `api.ts` holds one function for each command in the
//! command table, `commands::table`, which calls it by its name with its
//! arguments under the keys Tauri reads; the test checks those keys against
//! the command's own parameters.
//!
//! `bindings.ts` also holds what the page checks a draft against before it
//! saves: `appytappy-config`'s limits, the keys a key press and the
//! screenshot shortcut can name, what each kind of action does, the trace
//! categories, and the default settings. The save still checks everything
//! again. It holds the overlay windows' labels and how a picture's bytes
//! start, which the page reads too.
//!
//! The page also reads a typed path, link, key press, shortcut or app name
//! itself, so the user sees what to fix as they type, and places a click's
//! spot. The file holds what the Rust side makes of a fixed list of samples,
//! one for each way a parser accepts or refuses, and a sample picture, and
//! page tests check that the page reads every sample the same.
//!
//! The test renders both files and fails when a checked-in one differs, so
//! the gate's `cargo test` catches drift. With `UPDATE_BINDINGS=1` set, it
//! writes them instead.

use std::any::TypeId;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::future::Future;
use std::path::PathBuf;

use appytappy_config::action::{
    AppName, KeyChord, MAX_APP_NAME_LEN, MAX_FUNCTION_KEY, MAX_TARGET_LEN, Modifier, NAMED_KEYS,
    OpenTarget, SoundFile,
};
use appytappy_config::app_file::{
    AntiCheatDef, MAX_STATES, OverlayModeDef, ReferenceDef, WindowDef,
};
use appytappy_config::automation_file::{
    ActionDef, AutomationDef, ClickAtDef, ClickButtonDef, CornerDef, MAX_COOLDOWN_MS,
    MediaCommandDef, WhenDef,
};
use appytappy_config::click::{MAX_SEARCH_PIXELS, Spot};
use appytappy_config::settings_file::{
    FASTEST_READ_MS, MAX_TRACE_FILE_MB, MAX_TRACE_FILES, SHORTCUT_ALONE_KEYS, SHORTCUT_MODIFIERS,
    SHORTCUT_NAMED_KEYS, SLOWEST_READ_MS, SettingsFile, TraceCategory,
};
use appytappy_config::signal::NO_STATE;
use appytappy_sources::screen::{Image, Size};
use serde::Serialize;
use serde_json::json;
use tauri::AppHandle;
use tauri::ipc::Response;
use ts_rs::{Config, TS, TypeVisitor};

use crate::applications::AppSpec;
use crate::automations::AutomationSummary;
use crate::check_spec::{CheckSpec, ColorSpec, RegionSpec};
use crate::debug::bundle::BundleSpec;
use crate::debug::{DebugOverview, LogName};
use crate::overlay::OverlayView;
use crate::overlay::window::label;
use crate::owner::{
    AppSummary, ClickPictureView, FixSpec, FoundClick, Live, Reading, ReferenceSaveSpec, SaveSpec,
    SavedReferenceId, SavedValue,
};
use crate::pixels::{self, HEADER_BYTES};
use crate::recording::RecordingView;
use crate::selection::{SelectionSpec, SelectionView};
use crate::settings::SettingsView;
use crate::updates::UpdateOffer;
use crate::window_list::{WindowFit, WindowList};
use crate::{commands, updates};

/// The command to write the files again.
const REGENERATE: &str = "UPDATE_BINDINGS=1 cargo test -p appytappy-setup bindings";

/// The setup app's source folder.
fn source() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// A checked-in file of the page's, by its name in `src/lib`.
fn page_file(name: &str) -> PathBuf {
    source().join("../src/lib").join(name)
}

/// How a command's argument or answer crosses to the page: the Rust type
/// the command takes or answers, and the TypeScript type the page sends or
/// reads.
trait Wire {
    type Rust;
    fn ts(cfg: &Config) -> String;
    fn visit(found: &mut Declarations<'_>);
}

impl<T: TS + 'static> Wire for T {
    type Rust = T;
    fn ts(cfg: &Config) -> String {
        T::name(cfg).replace("Array<", "ReadonlyArray<")
    }
    fn visit(found: &mut Declarations<'_>) {
        found.visit::<T>();
    }
}

/// A picture, which a command answers as its raw bytes rather than JSON;
/// the page decodes it.
struct Pixels;

impl Wire for Pixels {
    type Rust = Response;
    fn ts(_cfg: &Config) -> String {
        "ArrayBuffer".to_owned()
    }
    fn visit(_found: &mut Declarations<'_>) {}
}

/// A file, which a command answers as its raw bytes rather than JSON.
struct Bytes;

impl Wire for Bytes {
    type Rust = Response;
    fn ts(_cfg: &Config) -> String {
        "Blob".to_owned()
    }
    fn visit(_found: &mut Declarations<'_>) {}
}

/// A `u64` that the page sends and reads as a number, such as a window's
/// handle or a watch's number, which stay far below 2^53.
struct U64AsNumber;

impl Wire for U64AsNumber {
    type Rust = u64;
    fn ts(_cfg: &Config) -> String {
        "number".to_owned()
    }
    fn visit(_found: &mut Declarations<'_>) {}
}

/// A [`U64AsNumber`] that can be missing, which the page reads as null.
struct OptionalU64AsNumber;

impl Wire for OptionalU64AsNumber {
    type Rust = Option<u64>;
    fn ts(cfg: &Config) -> String {
        format!("{} | null", U64AsNumber::ts(cfg))
    }
    fn visit(_found: &mut Declarations<'_>) {}
}

/// One row of the command table.
struct Command {
    /// The module that defines it, `commands` or `updates`.
    module: &'static str,
    name: &'static str,
    /// The page's doc comment, a line at a time.
    docs: Vec<&'static str>,
    /// Each argument's name and TypeScript type.
    args: Vec<(&'static str, String)>,
    /// The answer's TypeScript type.
    answer: String,
    /// Visits the types of its arguments and answer.
    visit: fn(&mut Declarations<'_>),
}

/// Returns `command`, a future answering `Result<T, String>`, so the
/// compiler checks a command's answer against its table row.
fn answers<T, F: Future<Output = Result<T, String>>>(command: F) -> F {
    command
}

/// Takes a closure that calls a command with its table row's argument
/// types; the compiler checks the call, so nothing runs.
fn signature<F>(_calls: F) {}

/// The command table's rows, in its order.
fn commands(cfg: &Config) -> Vec<Command> {
    macro_rules! rows {
        ($($(#[doc = $doc:literal])* $module:ident::$name:ident($($arg:ident: $ty:ty),* $(,)?) -> $answer:ty;)*) => {
            vec![$({
                signature(|app: AppHandle, $($arg: <$ty as Wire>::Rust),*| {
                    answers::<<$answer as Wire>::Rust, _>($module::$name(app, $($arg),*))
                });
                Command {
                    module: stringify!($module),
                    name: stringify!($name),
                    docs: vec![$($doc),*],
                    args: vec![$((stringify!($arg), <$ty as Wire>::ts(cfg))),*],
                    answer: <$answer as Wire>::ts(cfg),
                    visit: |found: &mut Declarations<'_>| {
                        $(<$ty as Wire>::visit(found);)*
                        <$answer as Wire>::visit(found);
                    },
                }
            }),*]
        };
    }
    commands::table::with_commands!(rows)
}

/// The names of the parameters `module`'s command `name` takes after its
/// app handle, as its source spells them, which Tauri reads its arguments
/// by. The command must be a plain `#[tauri::command]`, since an attribute
/// such as `rename_all` changes the keys Tauri reads.
fn parameters(module: &str, name: &str) -> Vec<String> {
    let file = source().join("src").join(format!("{module}.rs"));
    let text = fs::read_to_string(&file).expect("the command's module can be read");
    let opening = format!(" async fn {name}(");
    assert_eq!(
        text.matches(&opening).count(),
        1,
        "{module}.rs should define the command {name} once"
    );
    let (before, rest) = text.split_once(&opening).expect("counted once");
    let (attributes, _visibility) = before
        .rsplit_once('\n')
        .expect("the command starts a line of its own");
    let attribute = attributes
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| *line != "#[mutants::skip]");
    assert_eq!(
        attribute,
        Some("#[tauri::command]"),
        "the command {name} should be a plain #[tauri::command], so Tauri reads its arguments under camelCase keys"
    );
    let mut depth = 0_usize;
    let mut parts = vec![String::new()];
    for character in rest.chars() {
        match character {
            '(' | '<' | '[' => depth = depth.checked_add(1).expect("shallow"),
            ')' if depth == 0 => break,
            ')' | '>' | ']' => depth = depth.checked_sub(1).expect("balanced"),
            ',' if depth == 0 => {
                parts.push(String::new());
                continue;
            },
            _ => {},
        }
        parts.last_mut().expect("one part at least").push(character);
    }
    let mut names = parts
        .iter()
        .filter_map(|part| part.split_once(':'))
        .map(|(name, _type)| name.trim().to_owned());
    assert_eq!(
        names.next().as_deref(),
        Some("app"),
        "the command {name} takes the app's handle first"
    );
    let names: Vec<String> = names.collect();
    let body = rest.split_once("\n}\n").map_or(rest, |(body, _after)| body);
    for call in body.split("ask(&app, ").skip(1) {
        assert!(
            call.starts_with(&format!("\"{name}\"")),
            "the command {name} should name itself where it asks the owner, as setup.log records it"
        );
    }
    assert!(
        names.iter().all(|name| !name.starts_with('_')),
        "Tauri drops the leading underscore of {name}'s argument key, which the page's key keeps; rename the parameter"
    );
    names
}

/// `snake` in camelCase, as Tauri reads an argument's key and the page
/// names a function.
fn camel(snake: &str) -> String {
    let mut words = snake.split('_');
    let mut text = words.next().unwrap_or_default().to_owned();
    for word in words {
        let mut characters = word.chars();
        if let Some(first) = characters.next() {
            text.extend(first.to_uppercase());
            text.push_str(characters.as_str());
        }
    }
    text
}

/// A doc comment of `lines`, each with the space a `///` comment leaves.
fn doc_comment(lines: &[&str]) -> String {
    match lines {
        [] => String::new(),
        [line] => format!("/**{line} */\n"),
        _ => format!(
            "/**\n{}\n */\n",
            lines
                .iter()
                .map(|line| format!(" *{line}"))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    }
}

/// `api.ts`: a function for each command, which sends its arguments under
/// the keys Tauri reads and types its answer. A picture comes back decoded,
/// a file as its bytes, and an answer of nothing as `void`.
fn api(commands: &[Command], declared: &BTreeSet<String>) -> String {
    let mut imported = BTreeSet::new();
    let mut functions = Vec::new();
    for command in commands {
        let types = command
            .args
            .iter()
            .map(|(_name, ts)| ts)
            .chain([&command.answer]);
        for ts in types {
            imported.extend(
                ts.split(|character: char| !character.is_ascii_alphanumeric())
                    .filter(|word| declared.contains(*word)),
            );
        }
        let parameters = command
            .args
            .iter()
            .map(|(name, ts)| format!("{}: {ts}", camel(name)))
            .collect::<Vec<_>>()
            .join(", ");
        let keys = command
            .args
            .iter()
            .map(|(name, _ts)| camel(name))
            .collect::<Vec<_>>();
        let call = if keys.is_empty() {
            format!("'{}'", command.name)
        } else {
            format!("'{}', {{ {} }}", command.name, keys.join(", "))
        };
        let (answer, body) = match command.answer.as_str() {
            "null" => ("void".to_owned(), format!("await invoke({call});")),
            "ArrayBuffer" => (
                "Picture | null".to_owned(),
                format!("return decodePicture(await invoke<ArrayBuffer>({call}));"),
            ),
            "Blob" => (
                "Blob".to_owned(),
                format!("return new Blob([await invoke<ArrayBuffer>({call})]);"),
            ),
            answer => (
                answer.to_owned(),
                format!("return invoke<{answer}>({call});"),
            ),
        };
        functions.push(format!(
            "{}export async function {}({parameters}): Promise<{answer}> {{\n  {body}\n}}\n",
            doc_comment(&command.docs),
            camel(command.name)
        ));
    }
    format!(
        "// Generated from the setup app's command table by its bindings test. Do not edit it;\n\
         // change the table in `commands/table.rs` and run `{REGENERATE}`.\n\
         \n\
         import {{ invoke }} from '@tauri-apps/api/core';\n\
         \n\
         import type {{\n{}\n}} from './bindings.ts';\n\
         import {{ type Picture, decodePicture }} from './pixels.ts';\n\
         \n\
         /** The message of a failed command, which the Rust side writes for the user. */\n\
         export function errorText(error: unknown): string {{\n  \
         return typeof error === 'string' ? error : 'Something went wrong in AppyTappy Setup.';\n\
         }}\n\
         \n\
         {}",
        imported
            .iter()
            .map(|name| format!("  {name},"))
            .collect::<Vec<_>>()
            .join("\n"),
        functions.join("\n")
    )
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
fn declarations(cfg: &Config, commands: &[Command]) -> BTreeMap<String, String> {
    let mut found = Declarations {
        cfg,
        seen: BTreeSet::new(),
        declared: BTreeMap::new(),
    };
    for command in commands {
        (command.visit)(&mut found);
    }
    found.declared
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
        ActionDef::Click {
            app: "app".to_owned(),
            button: ClickButtonDef::Left,
            return_focus: true,
            at: ClickAtDef::Spot {
                corner: CornerDef::TopLeft,
                x: 0,
                y: 0,
            },
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
    let shortcut = strings(&[
        "F9",
        "F1",
        "F24",
        "PrintScreen",
        "Pause",
        "Control+Shift+S",
        "Control+Alt+Shift+Super+PrintScreen",
        "Alt+7",
        "Super+Left",
        "Control+Up",
        "Shift+Down",
        "Alt+Right",
        "Control+Space",
        "Control+Insert",
        "Control+Delete",
        "Control+Home",
        "Control+End",
        "Control+PageUp",
        "Shift+PageDown",
        "Control+F9",
        "Super+Pause",
        "",
        " F9 ",
        "F0",
        "F25",
        "F09",
        "S",
        "7",
        "Space",
        "Up",
        "Shift+Control+S",
        "Super+Alt+S",
        "Control+Control+S",
        "Control+",
        "Control",
        "Control+s",
        "Ctrl+S",
        "Win+S",
        "Control+Enter",
        "Control+Bogus",
        "Bogus",
    ]);
    json!({
        "openTarget": cases(&open, |text| OpenTarget::parse(text).ok().map(|target| target.as_str().to_owned())),
        "soundFile": cases(&sound, |text| {
            SoundFile::parse(text)
                .ok()
                .map(|file| file.path().to_str().expect("read from text").to_owned())
        }),
        "keys": cases(&keys, |text| KeyChord::parse(text).ok().map(|keys| keys.to_string())),
        "appName": cases(&app, |text| AppName::new(text).ok().map(|name| name.as_str().to_owned())),
        "screenshotShortcut": cases(&shortcut, |text| {
            SettingsFile {
                screenshot_shortcut: text.to_owned(),
                ..SettingsFile::default()
            }
            .check()
            .ok()
            .map(|()| text.to_owned())
        }),
        "spotPlacement": spot_cases(),
    })
}

/// Where sample spots land in sample windows: the point from the window's
/// top-left corner, or null when the spot lies outside it.
fn spot_cases() -> serde_json::Value {
    let corners = [
        CornerDef::TopLeft,
        CornerDef::TopRight,
        CornerDef::BottomLeft,
        CornerDef::BottomRight,
    ];
    let mut cases = Vec::new();
    for corner in corners {
        for (x, y, width, height) in [
            (0, 0, 1280, 720),
            (9, 15, 1280, 720),
            (1279, 719, 1280, 720),
            (1280, 0, 1280, 720),
            (0, 720, 1280, 720),
            (0, 0, 1, 1),
            (0, 0, 0, 0),
        ] {
            let spot = Spot::new(corner.into(), x, y).expect("a sample spot is near its corner");
            let place = spot
                .place(width, height)
                .map(|(x, y)| json!({ "x": x, "y": y }));
            cases.push(json!({
                "at": { "corner": corner, "x": x, "y": y },
                "window": { "width": width, "height": height },
                "place": place,
            }));
        }
    }
    serde_json::Value::Array(cases)
}

/// A 2 by 1 picture as a command answers it, with the size and RGBA bytes
/// the page should decode from it.
fn picture_sample() -> serde_json::Value {
    let size = Size {
        width: 2,
        height: 1,
    };
    let image = Image::new(size, vec![10, 20, 30, 0, 40, 50, 60, 7]).expect("two pixels");
    json!({
        "encoded": pixels::rgba(&image, u32::MAX),
        "width": size.width,
        "height": size.height,
        "rgba": [30, 20, 10, 255, 60, 50, 40, 255],
    })
}

/// A whole number type, by its highest value.
trait Highest {
    fn highest() -> u64;
}

impl Highest for u8 {
    fn highest() -> u64 {
        u8::MAX.into()
    }
}

/// The highest value of the field `read` reads, from the field's own type,
/// so the limit follows a change of that type.
fn highest_of<S, T: Highest>(_read: fn(&S) -> Option<&T>) -> u64 {
    T::highest()
}

/// The highest tolerance the page offers: a color's, a reference's ink's,
/// a selection's color match, a state check's, and a click picture's.
fn max_tolerance() -> u64 {
    let each = [
        highest_of(|check: &CheckSpec| match check {
            CheckSpec::Color { tolerance, .. }
            | CheckSpec::State { tolerance, .. }
            | CheckSpec::Characters { tolerance, .. } => Some(tolerance),
            CheckSpec::Number { .. } | CheckSpec::BarFill { .. } => None,
        }),
        highest_of(|reference: &ReferenceDef| Some(&reference.tolerance)),
        highest_of(|selection: &SelectionSpec| Some(&selection.tolerance)),
        highest_of(|at: &ClickAtDef| match at {
            ClickAtDef::Picture { tolerance, .. } => Some(tolerance),
            ClickAtDef::Spot { .. } => None,
        }),
    ];
    let highest = each.iter().copied().max().expect("four fields");
    assert!(
        each.iter().all(|&one| one == highest),
        "the tolerances differ in range; give the page a limit for each"
    );
    highest
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
        "maxTolerance": max_tolerance(),
        "maxSearchPixels": MAX_SEARCH_PIXELS,
    });
    let shortcut = json!({
        "modifiers": SHORTCUT_MODIFIERS,
        "named": SHORTCUT_NAMED_KEYS,
        "alone": SHORTCUT_ALONE_KEYS,
        "maxFunctionKey": MAX_FUNCTION_KEY,
    });
    let overlay_labels: BTreeMap<String, String> =
        [OverlayModeDef::Overlay, OverlayModeDef::PictureInPicture]
            .into_iter()
            .map(|mode| (spelled(&mode), label("", mode)))
            .collect();
    let pictures = json!({
        "headerBytes": HEADER_BYTES,
        "sample": picture_sample(),
    });
    let keys = json!({
        "modifiers": Modifier::ALL.map(|modifier| modifier.to_string()),
        "named": NAMED_KEYS.map(|(name, _)| name),
        "maxFunctionKey": MAX_FUNCTION_KEY,
    });
    [
        format!(
            "/**\n * The longest app name a media or volume action names and the longest path or link, the longest cooldown, the\n * most states, the settings' ranges, the highest tolerance, and the most pixels a click's search area holds, as the\n * saved files take them.\n */\nexport const LIMITS = {} as const;\n",
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
            "/**\n * The keys a screenshot shortcut can name: modifiers in the order they are written, named keys, the named keys\n * that need no modifier, and function keys from F1, which need none either.\n */\nexport const SHORTCUT_KEYS = {} as const;\n",
            literal(&shortcut)
        ),
        format!(
            "/** How an overlay window's label starts, before its application's ID, by its mode. */\nexport const OVERLAY_LABELS = {} as const satisfies Record<OverlayModeDef, string>;\n",
            literal(&overlay_labels)
        ),
        format!(
            "/**\n * How a picture a command answers starts: the bytes of its little-endian width and height before the RGBA rows.\n * The sample is a picture as the Rust side encodes it, and what it holds.\n */\nexport const PICTURES = {} as const;\n",
            literal(&pictures)
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
            "/**\n * What the Rust side makes of sample paths, links, key presses, screenshot shortcuts and app names: the text it\n * keeps, or null when it refuses one; and where sample spots land in sample windows. The page's own checks must read\n * each the same.\n */\nexport const PARSE_CASES = {} as const;\n",
            literal(&parse_cases())
        ),
        format!(
            "/** The settings a new install starts with. */\nexport const DEFAULT_SETTINGS: SettingsFile = {};\n",
            literal(&SettingsFile::default())
        ),
    ]
    .join("\n")
}

/// `text` without the spaces ts-rs leaves at the ends of lines.
fn trimmed(text: &str) -> String {
    let mut file = text
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    file.push('\n');
    file
}

/// Each file by its name in `src/lib`, as the Rust side renders it.
fn render() -> [(&'static str, String); 2] {
    let cfg = Config::new();
    let commands = commands(&cfg);
    let declared = declarations(&cfg, &commands);
    let bindings = format!(
        "// Generated from the setup app's Rust types by its bindings test. Do not edit it;\n\
         // change the Rust types and run `{REGENERATE}`.\n\
         \n\
         /** `T` with every property and array readonly, all the way down. */\n\
         type DeepReadonly<T> = T extends object ? {{ readonly [K in keyof T]: DeepReadonly<T[K]> }} : T;\n\
         \n\
         {}\n\
         {}",
        declared.values().cloned().collect::<Vec<_>>().join("\n"),
        constants()
    );
    let names = declared.into_keys().collect();
    [
        ("bindings.ts", trimmed(&bindings)),
        ("api.ts", trimmed(&api(&commands, &names))),
    ]
}

#[test]
fn bindings_match_the_rust_types() {
    for (name, rendered) in render() {
        assert!(
            !rendered.contains("bigint"),
            "a u64 or i64 reaches the page as a bigint, which JSON does not carry; give its field #[cfg_attr(test, ts(type = \"number\"))], or #[cfg_attr(feature = \"ts\", ts(type = \"number\"))] in appytappy-config, or make a command's argument or answer U64AsNumber"
        );
        if env::var_os("UPDATE_BINDINGS").is_some() {
            fs::write(page_file(name), &rendered).expect("the page's file can be written");
            continue;
        }
        let saved = fs::read_to_string(page_file(name)).unwrap_or_default();
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
            "apps/setup/src/lib/{name} differs from the Rust side at {differs}. Run `{REGENERATE}` and commit it."
        );
    }
}

#[test]
fn the_command_table_names_each_argument_as_its_command_does() {
    for command in commands(&Config::new()) {
        let table: Vec<&str> = command.args.iter().map(|(name, _ts)| *name).collect();
        assert_eq!(
            parameters(command.module, command.name),
            table,
            "the table's arguments of {} differ from its parameters",
            command.name
        );
    }
}

#[test]
fn camel_case_joins_words_as_tauri_reads_keys() {
    assert_eq!(camel("max_width"), "maxWidth");
    assert_eq!(camel("set_starts_with_windows"), "setStartsWithWindows");
    assert_eq!(camel("at"), "at");
}
