//! Executable contract that the configured lint wall fires: the `rust:lint`
//! command line must report exactly the diagnostics that trailing
//! `//~ ERROR <code>` comments mark in `policy-fixtures/lint-wall.rs.in`.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;
use toml::{Table, Value};

const PROBE_INPUT: &str = "policy-fixtures/lint-wall.rs.in";
const PROBE_SOURCE: &str = "src/lib.rs";
const ERROR_MARKER: &str = "//~ ERROR ";
const TASK_FRAGMENT: &str = ".config/mise/conf.d/20-rust.toml";
const PROOF_COMMAND: &str = "cargo test --locked --all-features --test lint_wall -- --ignored";
/// The `rust:lint` keys a replay of its `run` value reproduces. Any other key,
/// including one a later mise release adds, can change or skip what the gate
/// runs.
const REPLAYED_TASK_KEYS: [&str; 2] = ["description", "run"];
/// The Cargo arguments `rust:lint` passes before `--`, in any order. The probe
/// is one package without features or examples, so a narrower package, target,
/// or feature selection would still pass it while the gate skips code.
const CARGO_ARGUMENTS: [&str; 4] = ["--workspace", "--all-targets", "--all-features", "--locked"];
/// Cargo reads compiler flags from these variables, from every `CARGO_*`
/// variable that ends in `_RUSTFLAGS` or `_RUSTDOCFLAGS`, such as
/// `CARGO_ENCODED_RUSTFLAGS`, and from `rustflags` and `rustdocflags` in its
/// configuration files.
const FLAG_VARIABLES: [&str; 2] = ["RUSTFLAGS", "RUSTDOCFLAGS"];
/// Flag prefixes that can lower a lint below its `[workspace.lints]` or gate
/// level: `-A`, unstable `-Z` options such as `-Zcrate-attr`, and `@file`
/// argument files, whose contents this test cannot see. `-W` cannot lower a
/// level: the gate's `-D warnings` outranks it.
const LOWERING_PREFIXES: [&str; 3] = ["-A", "-Z", "@"];
/// Long options that lower lint levels, alone or with `=value`.
const LOWERING_OPTIONS: [&str; 3] = ["--allow", "--force-warn", "--cap-lints"];

/// The sampled wall. Dropping a case from the probe takes a matching edit
/// here, so a probe stripped of its markers cannot pass.
const REQUIRED_CODES: [&str; 14] = [
    "clippy::disallowed_types",
    "clippy::disallowed_methods",
    "clippy::disallowed_macros",
    "clippy::unwrap_used",
    "clippy::print_stdout",
    "clippy::print_stderr",
    "clippy::clone_on_ref_ptr",
    "clippy::rc_buffer",
    "clippy::arithmetic_side_effects",
    "missing_debug_implementations",
    "elided_lifetimes_in_paths",
    "clippy::allow_attributes",
    "clippy::allow_attributes_without_reason",
    "unfulfilled_lint_expectations",
];

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Diagnostic {
    file: String,
    line: u64,
    level: String,
    code: String,
}

#[derive(Deserialize)]
struct CargoRecord {
    reason: String,
    message: Option<CompilerMessage>,
}

#[derive(Deserialize)]
struct CompilerMessage {
    message: String,
    level: String,
    code: Option<DiagnosticCode>,
    spans: Vec<DiagnosticSpan>,
}

#[derive(Deserialize)]
struct DiagnosticCode {
    code: String,
}

#[derive(Deserialize)]
struct DiagnosticSpan {
    file_name: String,
    line_start: u64,
    is_primary: bool,
}

/// The configuration files mise merged into a task. A same-named task in any
/// other file overrides or extends the fragment's definition.
#[derive(Deserialize)]
struct ResolvedTask {
    config_sources: Vec<PathBuf>,
}

/// Cargo reruns a copied tree's up-to-date test binary, whose compile-time
/// `CARGO_MANIFEST_DIR` still names the original tree, so the value Cargo
/// sets at run time wins.
fn project_root() -> PathBuf {
    env::var_os("CARGO_MANIFEST_DIR")
        .map_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")), PathBuf::from)
}

fn read_project_file(root: &Path, path: &str) -> Result<String, String> {
    fs::read_to_string(root.join(path)).map_err(|error| format!("cannot read {path}: {error}"))
}

fn rust_tasks(root: &Path) -> Result<Table, String> {
    read_project_file(root, TASK_FRAGMENT)?
        .parse()
        .map_err(|error| format!("{TASK_FRAGMENT} is not TOML: {error}"))
}

fn task<'tasks>(tasks: &'tasks Table, name: &str) -> Result<&'tasks Value, String> {
    tasks
        .get("tasks")
        .and_then(|tasks| tasks.get(name))
        .ok_or_else(|| format!("{TASK_FRAGMENT} has no {name} task"))
}

fn task_run<'tasks>(tasks: &'tasks Table, name: &str) -> Result<&'tasks str, String> {
    let run = task(tasks, name)?
        .get("run")
        .ok_or_else(|| format!("{TASK_FRAGMENT} has no {name} run value"))?;
    run.as_str()
        .ok_or_else(|| format!("{name} must keep one string run value for this test: {run}"))
}

/// The `rust:lint` command line with JSON output, and with `--keep-going` so
/// a library unit that fails on the probe's marked errors cannot stop Cargo
/// before it lints the test unit. Neither flag changes which lints fire. A task
/// the replay cannot reproduce, or whose Clippy arguments lower lint levels, is
/// an error.
fn lint_command(tasks: &Table) -> Result<Vec<String>, String> {
    let lint = task(tasks, "rust:lint")?;
    if let Some(key) = lint.as_table().and_then(|table| {
        table
            .keys()
            .find(|key| !REPLAYED_TASK_KEYS.contains(&key.as_str()))
    }) {
        return Err(format!(
            "rust:lint must set only description and run, not {key}; the lint-wall proof replays only its run value"
        ));
    }
    let run = task_run(tasks, "rust:lint")?.trim();
    if !run
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || " -_:=./".contains(character))
    {
        return Err(format!(
            "rust:lint must stay one plain command line for this test to replay: {run:?}"
        ));
    }
    let mut command: Vec<String> = run.split_whitespace().map(str::to_owned).collect();
    if !command.starts_with(&["cargo".to_owned(), "clippy".to_owned()]) {
        return Err(format!("rust:lint must run cargo clippy: {run:?}"));
    }
    let cargo_arguments_end = command
        .iter()
        .position(|argument| argument == "--")
        .unwrap_or(command.len());
    let mut cargo_arguments: Vec<&str> = command
        .iter()
        .take(cargo_arguments_end)
        .skip(2)
        .map(String::as_str)
        .collect();
    cargo_arguments.sort_unstable();
    let mut allowed = CARGO_ARGUMENTS;
    allowed.sort_unstable();
    if cargo_arguments != allowed {
        return Err(format!(
            "rust:lint must pass Cargo exactly {CARGO_ARGUMENTS:?} before --; the one-package probe cannot prove a narrower selection: {run:?}"
        ));
    }
    if let Some(flag) = command
        .iter()
        .skip_while(|argument| *argument != "--")
        .find(|argument| lowers_lints(argument))
    {
        return Err(format!(
            "rust:lint must not lower lint levels; [workspace.lints] sets them: {flag}"
        ));
    }
    for flag in ["--keep-going", "--message-format=json"] {
        command.insert(cargo_arguments_end, flag.to_owned());
    }
    Ok(command)
}

/// Asks mise which files define the `rust:lint` the gate runs.
fn lint_task_sources(root: &Path) -> Result<Vec<PathBuf>, String> {
    let output = Command::new("mise")
        .args(["tasks", "info", "--json", "rust:lint"])
        .current_dir(root)
        .output()
        .map_err(|error| format!("cannot run mise: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "mise cannot resolve rust:lint: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let task: ResolvedTask = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("mise tasks info printed no config_sources: {error}"))?;
    task.config_sources
        .iter()
        .map(|source| {
            fs::canonicalize(source)
                .map_err(|error| format!("cannot resolve {}: {error}", source.display()))
        })
        .collect()
}

fn lowers_lints(flag: &str) -> bool {
    LOWERING_PREFIXES
        .iter()
        .any(|prefix| flag.starts_with(prefix))
        || LOWERING_OPTIONS.iter().any(|option| {
            flag.strip_prefix(option)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('='))
        })
}

/// Reports each Cargo flag variable that lowers a lint level. Names compare
/// case-insensitively, as Windows environment names do.
fn flag_variable_violations(
    variables: impl IntoIterator<Item = (OsString, OsString)>,
) -> Vec<String> {
    variables
        .into_iter()
        .filter_map(|(name, value)| {
            let name = name.into_string().ok()?;
            let upper = name.to_ascii_uppercase();
            let reads_flags = FLAG_VARIABLES.contains(&upper.as_str())
                || upper.strip_prefix("CARGO_").is_some_and(|rest| {
                    rest.ends_with("_RUSTFLAGS") || rest.ends_with("_RUSTDOCFLAGS")
                });
            let value = value.to_string_lossy();
            let flags: Vec<&str> = if upper.starts_with("CARGO_ENCODED_") {
                value.split('\u{1f}').collect()
            } else {
                value.split_whitespace().collect()
            };
            (reads_flags && flags.iter().any(|flag| lowers_lints(flag)))
                .then(|| format!("{name}={}", flags.join(" ")))
        })
        .collect()
}

/// The configuration files Cargo reads when the gate runs in `root`:
/// `.cargo/config` and `.cargo/config.toml` there and in each parent
/// directory, then `config` and `config.toml` in `cargo_home`. Paths stay as
/// Cargo finds them, because includes resolve beside a symlink, not its target.
fn cargo_config_files(root: &Path, cargo_home: Option<&Path>) -> Vec<PathBuf> {
    root.ancestors()
        .map(|directory| directory.join(".cargo"))
        .chain(cargo_home.map(Path::to_path_buf))
        .flat_map(|directory| [directory.join("config"), directory.join("config.toml")])
        .filter(|path| path.is_file())
        .collect()
}

/// Collects the `rustflags` and `rustdocflags` values at any depth, such as
/// under `[build]` or `[target.'cfg(unix)']`.
fn flag_settings<'config>(
    table: &'config Table,
    settings: &mut Vec<(&'config str, &'config Value)>,
) {
    for (key, value) in table {
        if key == "rustflags" || key == "rustdocflags" {
            settings.push((key, value));
        } else if let Value::Table(table) = value {
            flag_settings(table, settings);
        }
    }
}

/// Cargo `[env]` entries that replace part of the gate's Clippy setup.
const CLIPPY_ENV_OVERRIDES: [(&str, &str); 2] = [
    ("CLIPPY_ARGS", "the gate's Clippy arguments when forced"),
    ("CLIPPY_CONF_DIR", "the gate's clippy.toml"),
];

/// Reports lint-lowering flags in a Cargo configuration file and the files it
/// includes, and each `[env]` entry that replaces part of the gate's Clippy
/// setup. A missing optional include is skipped; Cargo fails on any other
/// missing include, and so does this check.
fn config_violations(path: &Path, visited: &mut BTreeSet<PathBuf>, violations: &mut Vec<String>) {
    let Ok(canonical) = fs::canonicalize(path) else {
        violations.push(format!("{} cannot be resolved", path.display()));
        return;
    };
    if !visited.insert(canonical) {
        return;
    }
    let config: Table = match fs::read_to_string(path)
        .map_err(|error| error.to_string())
        .and_then(|text| {
            text.parse()
                .map_err(|error: toml::de::Error| error.to_string())
        }) {
        Ok(config) => config,
        Err(error) => {
            violations.push(format!("{} is not readable TOML: {error}", path.display()));
            return;
        },
    };
    let mut settings = Vec::new();
    flag_settings(&config, &mut settings);
    for (key, value) in settings {
        let flags: Vec<&str> = match value {
            Value::String(flags) => flags.split_whitespace().collect(),
            Value::Array(flags) => flags.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        };
        if flags.iter().any(|flag| lowers_lints(flag)) {
            violations.push(format!("{}: {key} = {}", path.display(), flags.join(" ")));
        }
    }
    if let Some(env) = config.get("env").and_then(Value::as_table) {
        for (name, replaced) in CLIPPY_ENV_OVERRIDES {
            if env.keys().any(|key| key.eq_ignore_ascii_case(name)) {
                violations.push(format!(
                    "{}: [env] sets {name}, which replaces {replaced}",
                    path.display()
                ));
            }
        }
    }
    let includes = config
        .get("include")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    for include in includes {
        let relative = include
            .as_str()
            .or_else(|| include.get("path").and_then(Value::as_str));
        let optional = include.get("optional").and_then(Value::as_bool) == Some(true);
        let (Some(relative), Some(directory)) = (relative, path.parent()) else {
            continue;
        };
        let target = directory.join(relative);
        if target.is_file() {
            config_violations(&target, visited, violations);
        } else if !optional {
            violations.push(format!(
                "{} includes {relative}, which is missing",
                path.display()
            ));
        }
    }
}

/// Clippy reads `clippy.toml` from `CLIPPY_CONF_DIR`, resolved from the
/// project root where Cargo runs the gate, so any other directory replaces the
/// gate's configuration.
fn clippy_conf_dir_violation(root: &Path, directory: Option<&OsStr>) -> Option<String> {
    let directory = directory?;
    (fs::canonicalize(root.join(directory)).ok() != fs::canonicalize(root).ok()).then(|| {
        format!(
            "CLIPPY_CONF_DIR={} replaces the gate's clippy.toml",
            directory.to_string_lossy()
        )
    })
}

/// Reports every inherited setting that lowers a lint level for the gate or
/// replaces its Clippy configuration.
fn lint_level_violations(root: &Path) -> Vec<String> {
    let cargo_home = env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| env::home_dir().map(|home| home.join(".cargo")));
    let mut violations = flag_variable_violations(env::vars_os());
    violations.extend(clippy_conf_dir_violation(
        root,
        env::var_os("CLIPPY_CONF_DIR").as_deref(),
    ));
    let mut visited = BTreeSet::new();
    for path in cargo_config_files(root, cargo_home.as_deref()) {
        config_violations(&path, &mut visited, &mut violations);
    }
    violations
}

/// Reads a `[package]` field, following `field.workspace = true` to
/// `[workspace.package]` as Cargo does, because the probe workspace has no
/// such table to inherit from.
fn package_field(manifest: &Table, key: &str) -> Result<Value, String> {
    let value = manifest
        .get("package")
        .and_then(|package| package.get(key))
        .ok_or_else(|| format!("Cargo.toml [package] has no {key}"))?;
    if value.get("workspace").and_then(Value::as_bool) != Some(true) {
        return Ok(value.clone());
    }
    manifest
        .get("workspace")
        .and_then(|workspace| workspace.get("package"))
        .and_then(|package| package.get(key))
        .cloned()
        .ok_or_else(|| format!("Cargo.toml inherits {key}, but [workspace.package] has none"))
}

fn probe_manifest(manifest: &Table) -> Result<String, String> {
    let lints = manifest
        .get("workspace")
        .and_then(|workspace| workspace.get("lints"))
        .ok_or("Cargo.toml has no [workspace.lints] table")?;

    let mut probe_package = Table::new();
    probe_package.insert("name".to_owned(), Value::from("lint-wall-probe"));
    probe_package.insert("version".to_owned(), Value::from("0.0.0"));
    probe_package.insert("publish".to_owned(), Value::from(false));
    for key in ["edition", "rust-version"] {
        probe_package.insert(key.to_owned(), package_field(manifest, key)?);
    }
    let probe = Table::from_iter([
        ("package".to_owned(), Value::Table(probe_package)),
        (
            "lints".to_owned(),
            Value::Table(Table::from_iter([(
                "workspace".to_owned(),
                Value::from(true),
            )])),
        ),
        (
            "workspace".to_owned(),
            Value::Table(Table::from_iter([("lints".to_owned(), lints.clone())])),
        ),
    ]);
    toml::to_string(&probe).map_err(|error| format!("cannot write the probe manifest: {error}"))
}

fn expected_diagnostics(probe: &str) -> BTreeSet<Diagnostic> {
    probe
        .lines()
        .zip(1..)
        .flat_map(|(text, line)| {
            text.split(ERROR_MARKER)
                .skip(1)
                .map(move |code| Diagnostic {
                    file: PROBE_SOURCE.to_owned(),
                    line,
                    level: "error".to_owned(),
                    code: code.trim().to_owned(),
                })
        })
        .collect()
}

/// Keys each compiler message by its first primary span. A message without a
/// code or span keeps the field empty, so it can only count as unexpected.
fn observed_diagnostics(
    cargo_stdout: &str,
) -> Result<BTreeMap<Diagnostic, String>, serde_json::Error> {
    let mut observed = BTreeMap::new();
    for line in cargo_stdout.lines() {
        let record: CargoRecord = serde_json::from_str(line)?;
        if record.reason != "compiler-message" {
            continue;
        }
        let Some(message) = record.message else {
            continue;
        };
        let primary = message.spans.iter().find(|span| span.is_primary);
        observed.insert(
            Diagnostic {
                file: primary.map_or_else(String::new, |span| span.file_name.clone()),
                line: primary.map_or(0, |span| span.line_start),
                level: message.level,
                code: message.code.map_or_else(String::new, |code| code.code),
            },
            message.message,
        );
    }
    Ok(observed)
}

/// An empty directory under `target/` for one test's files.
fn fixture_directory(name: &str) -> io::Result<PathBuf> {
    let directory = project_root().join("target/lint-wall-fixtures").join(name);
    if directory.exists() {
        fs::remove_dir_all(&directory)?;
    }
    fs::create_dir_all(&directory)?;
    Ok(directory)
}

#[test]
fn recognizes_lint_lowering_flags() {
    for flag in [
        "-A",
        "-Aclippy::unwrap_used",
        "--allow",
        "--allow=unused",
        "--force-warn",
        "--force-warn=clippy::panic",
        "--cap-lints",
        "--cap-lints=warn",
        "-Zcrate-attr=allow(clippy::panic)",
        "@flags.txt",
    ] {
        assert!(lowers_lints(flag), "{flag} lowers lint levels");
    }
    for flag in [
        "-D",
        "-Dwarnings",
        "-W",
        "--warn=unused",
        "--forbid",
        "-Clink-arg=-Aside",
        "--allowed",
    ] {
        assert!(!lowers_lints(flag), "{flag} does not lower lint levels");
    }
}

#[test]
fn reports_cargo_flag_variables_that_lower_lints() {
    let variables = [
        ("RUSTFLAGS", "-C opt-level=1 -A clippy::unwrap_used"),
        ("CARGO_ENCODED_RUSTDOCFLAGS", "--cap-lints\u{1f}warn"),
        (
            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS",
            "--force-warn=clippy::panic",
        ),
        ("CARGO_PROFILE_DEV_RUSTFLAGS", "@flags.txt"),
        ("RustDocFlags", "-Zunstable-options"),
        ("CARGO_ENCODED_RUSTFLAGS", "-D\u{1f}warnings"),
        ("CARGO_BUILD_RUSTFLAGS", "-W clippy::pedantic"),
        ("CARGO_TARGET_DIR", "-A"),
        ("PATH", "-A"),
    ]
    .map(|(name, value)| (OsString::from(name), OsString::from(value)));
    assert_eq!(
        flag_variable_violations(variables),
        [
            "RUSTFLAGS=-C opt-level=1 -A clippy::unwrap_used",
            "CARGO_ENCODED_RUSTDOCFLAGS=--cap-lints warn",
            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS=--force-warn=clippy::panic",
            "CARGO_PROFILE_DEV_RUSTFLAGS=@flags.txt",
            "RustDocFlags=-Zunstable-options",
        ]
    );
}

#[test]
fn reports_cargo_configuration_that_lowers_lints_through_includes() {
    let cargo = fixture_directory("config")
        .expect("create the fixture")
        .join(".cargo");
    fs::create_dir_all(&cargo).expect("create .cargo");
    fs::write(
        cargo.join("config.toml"),
        "include = [\"extra.toml\", { path = \"absent.toml\", optional = true }, \"missing.toml\"]\n\n\
         [build]\nrustflags = [\"-C\", \"debuginfo=0\", \"-W\", \"clippy::pedantic\"]\n",
    )
    .expect("write config.toml");
    fs::write(
        cargo.join("extra.toml"),
        "include = [\"config.toml\"]\n\n\
         [build]\nrustdocflags = \"-Arustdoc::all\"\n\n\
         [env]\nclippy_args = { value = \"\", force = true }\nclippy_conf_dir = \"lints\"\n\n\
         [target.'cfg(unix)']\nrustflags = [\"--cap-lints\", \"warn\"]\n",
    )
    .expect("write extra.toml");

    let mut violations = Vec::new();
    config_violations(
        &cargo.join("config.toml"),
        &mut BTreeSet::new(),
        &mut violations,
    );
    let extra = cargo.join("extra.toml");
    assert_eq!(
        violations,
        [
            format!("{}: rustdocflags = -Arustdoc::all", extra.display()),
            format!("{}: rustflags = --cap-lints warn", extra.display()),
            format!(
                "{}: [env] sets CLIPPY_ARGS, which replaces the gate's Clippy arguments when forced",
                extra.display()
            ),
            format!(
                "{}: [env] sets CLIPPY_CONF_DIR, which replaces the gate's clippy.toml",
                extra.display()
            ),
            format!(
                "{} includes missing.toml, which is missing",
                cargo.join("config.toml").display()
            ),
        ]
    );
}

#[cfg(unix)]
#[test]
fn resolves_includes_beside_a_symlinked_configuration() {
    let root = fixture_directory("symlink").expect("create the fixture");
    let cargo = root.join(".cargo");
    let dotfiles = root.join("dotfiles");
    for directory in [&cargo, &dotfiles] {
        fs::create_dir_all(directory).expect("create the directory");
    }
    fs::write(dotfiles.join("cargo.toml"), "include = [\"extra.toml\"]\n")
        .expect("write the linked configuration");
    std::os::unix::fs::symlink(dotfiles.join("cargo.toml"), cargo.join("config.toml"))
        .expect("link the configuration");
    fs::write(
        cargo.join("extra.toml"),
        "[build]\nrustflags = [\"-Aclippy::expect_used\"]\n",
    )
    .expect("write extra.toml");

    let mut violations = Vec::new();
    for path in cargo_config_files(&root, None) {
        config_violations(&path, &mut BTreeSet::new(), &mut violations);
    }
    assert!(
        violations.contains(&format!(
            "{}: rustflags = -Aclippy::expect_used",
            cargo.join("extra.toml").display()
        )),
        "{violations:?}"
    );
}

#[test]
fn reads_cargo_configuration_from_each_parent_and_cargo_home() {
    let root = fixture_directory("chain").expect("create the fixture");
    let project = root.join("workspace/project");
    let cargo_home = root.join("cargo-home");
    let files = [
        project.join(".cargo/config"),
        root.join("workspace/.cargo/config.toml"),
        cargo_home.join("config.toml"),
    ];
    for file in &files {
        fs::create_dir_all(file.parent().expect("a file has a parent"))
            .expect("create the directory");
        fs::write(file, "").expect("write the configuration");
    }
    let found = cargo_config_files(&project, Some(&cargo_home));
    for file in files {
        assert!(found.contains(&file), "Cargo reads {}", file.display());
    }
}

#[test]
fn rejects_a_clippy_configuration_outside_the_project_root() {
    let root = fixture_directory("clippy-conf").expect("create the fixture");
    fs::create_dir_all(root.join("lints")).expect("create the alternate directory");
    let violation = |directory: &str| clippy_conf_dir_violation(&root, Some(OsStr::new(directory)));
    assert_eq!(clippy_conf_dir_violation(&root, None), None);
    assert_eq!(violation("."), None);
    assert_eq!(violation(&root.to_string_lossy()), None);
    assert_eq!(violation("lints/.."), None);
    for directory in ["lints", "missing", ".."] {
        assert_eq!(
            violation(directory),
            Some(format!(
                "CLIPPY_CONF_DIR={directory} replaces the gate's clippy.toml"
            ))
        );
    }
}

#[test]
fn rejects_lint_lowering_gate_arguments() {
    let gate = |arguments: &str| {
        let tasks: Table = format!(
            "[tasks.\"rust:lint\"]\nrun = \"cargo clippy --workspace --all-targets --all-features --locked -- {arguments}\"\n"
        )
        .parse()
        .expect("valid TOML");
        lint_command(&tasks)
    };
    assert!(gate("-D warnings -F clippy::allow_attributes").is_ok());
    let error = gate("-D warnings -A clippy::expect_used").expect_err("-A lowers a lint");
    assert!(error.contains("must not lower lint levels"), "{error}");
}

#[test]
fn rust_policy_runs_the_lint_wall_proof() {
    let tasks = rust_tasks(&project_root()).unwrap_or_else(|error| panic!("{error}"));
    let policy = task_run(&tasks, "rust:policy").unwrap_or_else(|error| panic!("{error}"));
    assert!(
        policy.lines().any(|line| line.trim() == PROOF_COMMAND),
        "rust:policy must run the ignored lint-wall proof with `{PROOF_COMMAND}`"
    );
}

#[test]
#[ignore = "run by rust:policy"]
fn gate_clippy_reports_exactly_the_marked_probe_diagnostics() {
    let root = project_root();
    let read =
        |path: &str| read_project_file(&root, path).unwrap_or_else(|error| panic!("{error}"));
    let manifest: Table = read("Cargo.toml").parse().expect("Cargo.toml is TOML");
    let tasks = rust_tasks(&root).unwrap_or_else(|error| panic!("{error}"));
    let fragment = fs::canonicalize(root.join(TASK_FRAGMENT)).expect("the task fragment exists");
    let sources = lint_task_sources(&root).unwrap_or_else(|error| panic!("{error}"));
    assert!(
        sources == [fragment],
        "rust:lint must be defined only in {TASK_FRAGMENT}, which this proof replays; mise merges it from {sources:?}"
    );
    let command = lint_command(&tasks).unwrap_or_else(|error| panic!("{error}"));
    let lowered = lint_level_violations(&root);
    assert!(
        lowered.is_empty(),
        "lint levels come only from [workspace.lints] and reasoned expectations, but the gate inherits flags that lower them:\n{}",
        lowered.join("\n")
    );
    let probe = read(PROBE_INPUT);
    let expected = expected_diagnostics(&probe);
    let unmarked: Vec<&str> = REQUIRED_CODES
        .into_iter()
        .filter(|code| !expected.iter().any(|diagnostic| diagnostic.code == *code))
        .collect();
    assert!(
        unmarked.is_empty(),
        "{PROBE_INPUT} no longer marks these sampled diagnostics: {unmarked:?}"
    );
    let test_module = probe
        .lines()
        .zip(1..)
        .find_map(|(text, line)| (text.trim() == "#[cfg(test)]").then_some(line))
        .expect("the probe keeps a #[cfg(test)] module");
    assert!(
        expected
            .iter()
            .any(|diagnostic| diagnostic.line > test_module),
        "{PROBE_INPUT} must mark a diagnostic inside its #[cfg(test)] module, so the proof fails if rust:lint stops linting test targets"
    );

    // Inside target/, source scans, packaging, and mutation never see the
    // probe, while rustup and Cargo still resolve this project's toolchain and
    // configuration. Its own target directory keeps it off the shared build
    // lock. Clippy reads the gate's configuration at the project root; the
    // policy check above rejects a CLIPPY_CONF_DIR that names another one.
    let workspace = root.join("target/lint-wall-probe");
    fs::create_dir_all(workspace.join("src")).expect("create the probe workspace");
    fs::write(
        workspace.join("Cargo.toml"),
        probe_manifest(&manifest).unwrap_or_else(|error| panic!("{error}")),
    )
    .expect("write the probe manifest");
    fs::write(workspace.join(PROBE_SOURCE), &probe).expect("write the probe source");
    let cargo = |arguments: &[String]| {
        let (program, arguments) = arguments.split_first().expect("a command has a program");
        Command::new(program)
            .args(arguments)
            .current_dir(&workspace)
            .env("CARGO_TARGET_DIR", workspace.join("target"))
            .env("CLIPPY_CONF_DIR", &root)
            .output()
            .unwrap_or_else(|error| panic!("cannot run {program}: {error}"))
    };

    let lockfile = cargo(&["cargo", "generate-lockfile", "--offline"].map(str::to_owned));
    assert!(
        lockfile.status.success(),
        "cannot generate the probe lockfile: {}",
        String::from_utf8_lossy(&lockfile.stderr)
    );
    let lint = cargo(&command);
    let observed = observed_diagnostics(&String::from_utf8_lossy(&lint.stdout))
        .expect("cargo writes one JSON record per line");

    let missing: Vec<&Diagnostic> = expected
        .iter()
        .filter(|diagnostic| !observed.contains_key(diagnostic))
        .collect();
    let unexpected: Vec<(&Diagnostic, &String)> = observed
        .iter()
        .filter(|(diagnostic, _)| !expected.contains(diagnostic))
        .collect();
    assert!(
        missing.is_empty() && unexpected.is_empty(),
        "the rust:lint command no longer reports exactly the diagnostics marked in {PROBE_INPUT}\n\
         missing: {missing:#?}\nunexpected: {unexpected:#?}\n{}",
        String::from_utf8_lossy(&lint.stderr)
    );
}
