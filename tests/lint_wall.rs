//! Executable contract that the configured lint wall fires: Clippy, with
//! the workspace's `[workspace.lints]` and `clippy.toml`, must report exactly
//! the diagnostics that trailing `//~ ERROR <code>` comments mark in
//! `policy-fixtures/lint-wall.rs.in`.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;
use toml::{Table, Value};

const PROBE_INPUT: &str = "policy-fixtures/lint-wall.rs.in";
const PROBE_SOURCE: &str = "src/lib.rs";
const ERROR_MARKER: &str = "//~ ERROR ";
/// The gate's Clippy run on the one-package probe, with JSON output, and
/// with `--keep-going` so a library unit that fails on the probe's marked
/// errors cannot stop Cargo before it lints the test unit.
const LINT_COMMAND: [&str; 8] = [
    "cargo",
    "clippy",
    "--all-targets",
    "--offline",
    "--keep-going",
    "--message-format=json",
    "--",
    "-Dwarnings",
];

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

#[test]
#[ignore = "run by rust:policy"]
fn gate_clippy_reports_exactly_the_marked_probe_diagnostics() {
    let root = project_root();
    let read =
        |path: &str| read_project_file(&root, path).unwrap_or_else(|error| panic!("{error}"));
    let manifest: Table = read("Cargo.toml").parse().expect("Cargo.toml is TOML");
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
        "{PROBE_INPUT} must mark a diagnostic inside its #[cfg(test)] module, so the proof fails if the wall stops covering test code"
    );

    // Inside target/, source scans and mutation never see the
    // probe, while rustup and Cargo still resolve this project's toolchain and
    // configuration. Its own target directory keeps it off the shared build
    // lock. Clippy reads the gate's clippy.toml at the project root.
    let workspace = root.join("target/lint-wall-probe");
    fs::create_dir_all(workspace.join("src")).expect("create the probe workspace");
    fs::write(
        workspace.join("Cargo.toml"),
        probe_manifest(&manifest).unwrap_or_else(|error| panic!("{error}")),
    )
    .expect("write the probe manifest");
    fs::write(workspace.join(PROBE_SOURCE), &probe).expect("write the probe source");
    let cargo = |arguments: &[&str]| {
        let (program, arguments) = arguments.split_first().expect("a command has a program");
        Command::new(program)
            .args(arguments)
            .current_dir(&workspace)
            .env("CARGO_TARGET_DIR", workspace.join("target"))
            .env("CLIPPY_CONF_DIR", &root)
            .output()
            .unwrap_or_else(|error| panic!("cannot run {program}: {error}"))
    };

    let lockfile = cargo(&["cargo", "generate-lockfile", "--offline"]);
    assert!(
        lockfile.status.success(),
        "cannot generate the probe lockfile: {}",
        String::from_utf8_lossy(&lockfile.stderr)
    );
    let lint = cargo(&LINT_COMMAND);
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
        "Clippy no longer reports exactly the diagnostics marked in {PROBE_INPUT}\n\
         missing: {missing:#?}\nunexpected: {unexpected:#?}\n{}",
        String::from_utf8_lossy(&lint.stderr)
    );
}
