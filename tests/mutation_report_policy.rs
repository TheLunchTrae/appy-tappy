//! Executable contract for cargo-mutants report evidence.

use std::env::{self, VarError};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{Value, json};

const CARGO_MUTANTS_VERSION: &str = "27.1.0";
const REPORT_DIRECTORY_VARIABLE: &str = "MUTATION_REPORT_DIRECTORY";
const REPORT_MODE_VARIABLE: &str = "MUTATION_REPORT_MODE";

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
struct Counts {
    caught: u64,
    missed: u64,
    timeout: u64,
    unviable: u64,
}

impl Counts {
    fn classified(self) -> Result<u64, String> {
        self.caught
            .checked_add(self.missed)
            .and_then(|count| count.checked_add(self.timeout))
            .and_then(|count| count.checked_add(self.unviable))
            .ok_or_else(|| "cargo-mutants outcome counts overflowed".to_owned())
    }

    fn executed(self) -> Result<u64, String> {
        self.caught
            .checked_add(self.missed)
            .and_then(|count| count.checked_add(self.timeout))
            .ok_or_else(|| "cargo-mutants executed counts overflowed".to_owned())
    }

    fn increment(&mut self, summary: &str) -> Result<(), String> {
        let count = match summary {
            "CaughtMutant" => &mut self.caught,
            "MissedMutant" => &mut self.missed,
            "Timeout" => &mut self.timeout,
            "Unviable" => &mut self.unviable,
            other => {
                return Err(format!(
                    "cargo-mutants report contains unknown summary {other:?}"
                ));
            },
        };
        *count = count
            .checked_add(1)
            .ok_or_else(|| "cargo-mutants outcome count overflowed".to_owned())?;
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct Outcome {
    scenario: Value,
    summary: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MutationReport {
    outcomes: Vec<Outcome>,
    total_mutants: u64,
    missed: u64,
    caught: u64,
    timeout: u64,
    unviable: u64,
    success: u64,
    start_time: String,
    end_time: Option<String>,
    cargo_mutants_version: String,
}

#[derive(Clone, Copy)]
enum ReportMode {
    Full,
    Diff,
}

impl ReportMode {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "full" => Ok(Self::Full),
            "diff" => Ok(Self::Diff),
            _ => Err(format!("unknown mutation report mode {value:?}")),
        }
    }
}

fn verify_report(mode: ReportMode, report_directory: &Path) -> Result<(u64, u64), String> {
    let report_path = report_directory.join("outcomes.json");
    let document = fs::read_to_string(&report_path)
        .map_err(|error| format!("cannot read {}: {error}", report_path.display()))?;
    let report: MutationReport = serde_json::from_str(&document).map_err(|error| {
        format!(
            "{} is not valid pinned-schema JSON: {error}",
            report_path.display()
        )
    })?;

    if report.cargo_mutants_version != CARGO_MUTANTS_VERSION {
        return Err(format!(
            "cargo-mutants report version must be {CARGO_MUTANTS_VERSION}, got {:?}",
            report.cargo_mutants_version
        ));
    }
    if report.start_time.trim().is_empty()
        || report
            .end_time
            .as_deref()
            .is_none_or(|end_time| end_time.trim().is_empty())
    {
        return Err(
            "cargo-mutants report is incomplete: start_time and end_time are required".to_owned(),
        );
    }
    if report.success != 0 {
        return Err("cargo-mutants report contains unclassified successful mutants".to_owned());
    }

    let reported = Counts {
        caught: report.caught,
        missed: report.missed,
        timeout: report.timeout,
        unviable: report.unviable,
    };
    if reported.classified()? != report.total_mutants {
        return Err("cargo-mutants outcome counts do not equal total_mutants".to_owned());
    }

    let observed = observed_outcome_counts(&report.outcomes)?;
    if observed != reported {
        return Err("cargo-mutants JSON summary does not match its outcome records".to_owned());
    }
    verify_outcome_lists(report_directory, reported)?;

    let executed = reported.executed()?;
    if matches!(mode, ReportMode::Full) && (report.total_mutants == 0 || executed == 0) {
        return Err("full mutation testing must execute at least one mutant".to_owned());
    }
    if report.total_mutants > 0 && executed == 0 {
        return Err(
            "a nonempty mutation report must contain at least one executed mutant".to_owned(),
        );
    }
    Ok((executed, report.total_mutants))
}

fn observed_outcome_counts(outcomes: &[Outcome]) -> Result<Counts, String> {
    let mut baseline_count = 0_u64;
    let mut counts = Counts::default();
    for outcome in outcomes {
        match &outcome.scenario {
            Value::String(scenario) if scenario == "Baseline" => {
                if outcome.summary != "Success" {
                    return Err("cargo-mutants baseline did not succeed".to_owned());
                }
                baseline_count = baseline_count
                    .checked_add(1)
                    .ok_or_else(|| "cargo-mutants baseline count overflowed".to_owned())?;
            },
            Value::Object(scenario) if scenario.len() == 1 && scenario.contains_key("Mutant") => {
                counts.increment(&outcome.summary)?;
            },
            _ => return Err("cargo-mutants report contains an unknown scenario".to_owned()),
        }
    }
    if baseline_count != 1 {
        return Err("cargo-mutants report must contain exactly one successful baseline".to_owned());
    }
    Ok(counts)
}

fn verify_outcome_lists(report_directory: &Path, counts: Counts) -> Result<(), String> {
    for (name, expected) in [
        ("caught", counts.caught),
        ("missed", counts.missed),
        ("timeout", counts.timeout),
        ("unviable", counts.unviable),
    ] {
        let path = report_directory.join(format!("{name}.txt"));
        let contents = fs::read_to_string(&path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        let actual = u64::try_from(
            contents
                .lines()
                .filter(|line| !line.trim().is_empty())
                .count(),
        )
        .map_err(|error| format!("{} has too many entries: {error}", path.display()))?;
        if actual != expected {
            return Err(format!(
                "cargo-mutants {name} evidence is internally inconsistent: expected {expected}, got {actual}"
            ));
        }
    }
    Ok(())
}

#[test]
#[ignore = "run by rust:policy"]
fn accepts_consistent_full_and_empty_diff_reports() {
    let caught = TestReport::new(
        "caught",
        Counts {
            caught: 1,
            ..Counts::default()
        },
    )
    .expect("create caught report");
    assert_eq!(verify_report(ReportMode::Full, caught.path()), Ok((1, 1)));

    let empty = TestReport::new("empty", Counts::default()).expect("create empty report");
    assert_eq!(verify_report(ReportMode::Diff, empty.path()), Ok((0, 0)));
    assert!(verify_report(ReportMode::Full, empty.path()).is_err());
}

#[test]
#[ignore = "run by rust:policy"]
fn rejects_malformed_trailing_and_duplicate_json() {
    let malformed = TestReport::with_document("malformed", "THIS IS NOT JSON")
        .expect("create malformed report");
    assert!(verify_report(ReportMode::Full, malformed.path()).is_err());

    let trailing = TestReport::new(
        "trailing",
        Counts {
            caught: 1,
            ..Counts::default()
        },
    )
    .expect("create trailing report");
    fs::write(
        trailing.path().join("outcomes.json"),
        format!(
            "{}\nTRAILING GARBAGE\n",
            trailing.document().expect("read trailing report")
        ),
    )
    .expect("write trailing-garbage report");
    assert!(verify_report(ReportMode::Full, trailing.path()).is_err());

    let duplicate = TestReport::new(
        "duplicate",
        Counts {
            caught: 1,
            ..Counts::default()
        },
    )
    .expect("create duplicate report");
    let document = duplicate
        .document()
        .expect("read duplicate report")
        .replacen(
            "\"total_mutants\": 1",
            "\"total_mutants\": 1, \"total_mutants\": 1",
            1,
        );
    fs::write(duplicate.path().join("outcomes.json"), document)
        .expect("write duplicate-field report");
    assert!(verify_report(ReportMode::Full, duplicate.path()).is_err());

    let blank_end_time = TestReport::new(
        "blank-end-time",
        Counts {
            caught: 1,
            ..Counts::default()
        },
    )
    .expect("create blank-end-time report");
    let document = blank_end_time
        .document()
        .expect("read blank-end-time report")
        .replacen(
            "\"end_time\": \"2026-08-20T00:00:01Z\"",
            "\"end_time\": \"   \"",
            1,
        );
    fs::write(blank_end_time.path().join("outcomes.json"), document)
        .expect("write blank-end-time report");
    assert!(verify_report(ReportMode::Full, blank_end_time.path()).is_err());
}

#[test]
#[ignore = "run by rust:policy"]
fn rejects_inconsistent_and_all_unviable_reports() {
    let inconsistent = TestReport::new(
        "inconsistent",
        Counts {
            caught: 1,
            ..Counts::default()
        },
    )
    .expect("create inconsistent report");
    fs::write(inconsistent.path().join("caught.txt"), "").expect("write inconsistent outcome list");
    assert!(verify_report(ReportMode::Full, inconsistent.path()).is_err());

    let unviable = TestReport::new(
        "unviable",
        Counts {
            unviable: 1,
            ..Counts::default()
        },
    )
    .expect("create unviable report");
    assert!(verify_report(ReportMode::Full, unviable.path()).is_err());
    assert!(verify_report(ReportMode::Diff, unviable.path()).is_err());
}

#[test]
#[ignore = "run directly by the post-mutation verifier"]
fn verify_configured_report() {
    let mode = env::var(REPORT_MODE_VARIABLE);
    let report_directory = env::var(REPORT_DIRECTORY_VARIABLE);
    if matches!(
        (&mode, &report_directory),
        (Err(VarError::NotPresent), Err(VarError::NotPresent))
    ) {
        return;
    }
    let mode = ReportMode::parse(&mode.expect("mutation report mode must be present"))
        .expect("mutation report mode must be full or diff");
    let report_directory =
        PathBuf::from(report_directory.expect("mutation report directory must be present"));
    let (executed, total) =
        verify_report(mode, &report_directory).unwrap_or_else(|error| panic!("{error}"));
    println!("Verified {executed} executed mutant outcome(s) out of {total} generated.");
}

struct TestReport {
    root: PathBuf,
}

impl TestReport {
    fn new(label: &str, counts: Counts) -> Result<Self, String> {
        let report = Self::empty(label)?;
        let document = report_document(counts)?;
        fs::write(report.root.join("outcomes.json"), &document)
            .map_err(|error| format!("cannot write synthetic mutation report: {error}"))?;
        write_outcome_lists(&report.root, counts)?;
        Ok(report)
    }

    fn with_document(label: &str, document: &str) -> Result<Self, String> {
        let report = Self::empty(label)?;
        fs::write(report.root.join("outcomes.json"), document)
            .map_err(|error| format!("cannot write synthetic malformed report: {error}"))?;
        write_outcome_lists(&report.root, Counts::default())?;
        Ok(report)
    }

    fn empty(label: &str) -> Result<Self, String> {
        let root = env::temp_dir().join(format!(
            "standards-mutation-report-{}-{label}",
            std::process::id()
        ));
        if let Err(error) = fs::remove_dir_all(&root)
            && error.kind() != io::ErrorKind::NotFound
        {
            return Err(format!(
                "cannot clear synthetic mutation report directory: {error}"
            ));
        }
        fs::create_dir_all(&root).map_err(|error| {
            format!("cannot create synthetic mutation report directory: {error}")
        })?;
        Ok(Self { root })
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn document(&self) -> Result<String, String> {
        fs::read_to_string(self.root.join("outcomes.json"))
            .map_err(|error| format!("cannot read synthetic mutation report: {error}"))
    }
}

impl Drop for TestReport {
    fn drop(&mut self) {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "Drop cannot return the failure; a leftover temporary directory does not change a test's result"
        )]
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn report_document(counts: Counts) -> Result<String, String> {
    let mut outcomes = vec![json!({ "scenario": "Baseline", "summary": "Success" })];
    for (count, summary) in [
        (counts.caught, "CaughtMutant"),
        (counts.missed, "MissedMutant"),
        (counts.timeout, "Timeout"),
        (counts.unviable, "Unviable"),
    ] {
        for index in 0..count {
            outcomes.push(json!({
                "scenario": { "Mutant": { "name": format!("mutant-{summary}-{index}") } },
                "summary": summary,
            }));
        }
    }
    let total_mutants = counts.classified()?;
    serde_json::to_string_pretty(&json!({
        "outcomes": outcomes,
        "total_mutants": total_mutants,
        "missed": counts.missed,
        "caught": counts.caught,
        "timeout": counts.timeout,
        "unviable": counts.unviable,
        "success": 0,
        "start_time": "2026-08-20T00:00:00Z",
        "end_time": "2026-08-20T00:00:01Z",
        "cargo_mutants_version": CARGO_MUTANTS_VERSION,
    }))
    .map_err(|error| format!("cannot serialize synthetic mutation report: {error}"))
}

fn write_outcome_lists(root: &Path, counts: Counts) -> Result<(), String> {
    for (name, count) in [
        ("caught", counts.caught),
        ("missed", counts.missed),
        ("timeout", counts.timeout),
        ("unviable", counts.unviable),
    ] {
        let mut contents = (0..count)
            .map(|index| format!("src/lib.rs:{index}: synthetic {name}"))
            .collect::<Vec<_>>()
            .join("\n");
        if !contents.is_empty() {
            contents.push('\n');
        }
        fs::write(root.join(format!("{name}.txt")), contents)
            .map_err(|error| format!("cannot write synthetic {name} list: {error}"))?;
    }
    Ok(())
}
