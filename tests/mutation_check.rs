//! The project's own rules for a cargo-mutants report, which
//! `scripts/check-mutants.sh` checks after each mutation lane.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A report's folder, which this process alone writes and which is removed
/// with the value. cargo-mutants runs two copies of a test binary at once,
/// so a folder shared between processes would let one test delete
/// another's report.
struct Report(PathBuf);

impl Drop for Report {
    fn drop(&mut self) {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "removing a report is best-effort cleanup; a failure leaves only a folder under target/"
        )]
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A report with the outcome lists `lists`, as list name and its lines, and
/// one unviable mutant's log, `log`, in a fresh folder named for `name`.
fn report(name: &str, lists: &[(&str, &str)], log: Option<&str>) -> io::Result<Report> {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("mutation-check")
        .join(format!("{name}-{}", std::process::id()));
    match fs::remove_dir_all(&folder) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
        _ => {},
    }
    fs::create_dir_all(folder.join("log"))?;
    for (list, lines) in lists {
        fs::write(folder.join(format!("{list}.txt")), lines)?;
    }
    fs::write(folder.join("log/baseline.log"), "\n*** baseline\n")?;
    if let Some(log) = log {
        fs::write(folder.join("log/src__lib.rs_line_2_col_22.log"), log)?;
    }
    Ok(Report(folder))
}

/// Whether the check passes `report` from the lane `mode`.
fn passes(mode: &str, report: &Report) -> io::Result<bool> {
    let output = Command::new("sh")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/check-mutants.sh"))
        .arg(mode)
        .arg(&report.0)
        .output()?;
    Ok(output.status.success())
}

const MUTANT: &str = "src/lib.rs:2:22: replace make -> X with Default::default()\n";

/// The log of a mutant the compiler rejected, with the error at `at`.
fn rejected_at(at: &str) -> String {
    format!(
        "\n*** {MUTANT}\n*** mutation diff:\n--- src/lib.rs\n\n\
         error[E0277]: the trait bound `X: Default` is not satisfied\n --> {at}:2:22\n  |\n"
    )
}

#[test]
fn a_full_run_must_test_a_mutant_that_builds() -> io::Result<()> {
    let caught = report(
        "full-caught",
        &[
            ("caught", MUTANT),
            ("missed", ""),
            ("timeout", ""),
            ("unviable", ""),
        ],
        None,
    )?;
    assert!(passes("full", &caught)?);
    let timed_out = report(
        "full-timeout",
        &[
            ("caught", ""),
            ("missed", ""),
            ("timeout", MUTANT),
            ("unviable", ""),
        ],
        None,
    )?;
    assert!(
        passes("full", &timed_out)?,
        "a mutant that timed out was tested"
    );
    let empty = report(
        "full-empty",
        &[
            ("caught", ""),
            ("missed", ""),
            ("timeout", ""),
            ("unviable", ""),
        ],
        None,
    )?;
    assert!(!passes("full", &empty)?, "no mutant at all");
    assert!(passes("diff", &empty)?, "a diff shard can select none");
    let unviable = report(
        "full-unviable",
        &[
            ("caught", ""),
            ("missed", ""),
            ("timeout", ""),
            ("unviable", MUTANT),
        ],
        Some(&rejected_at("src/lib.rs")),
    )?;
    assert!(!passes("full", &unviable)?, "only unviable mutants");
    Ok(())
}

#[test]
fn a_diff_of_only_unviable_mutants_needs_the_compiler_to_reject_each() -> io::Result<()> {
    let lists = [
        ("caught", ""),
        ("missed", ""),
        ("timeout", ""),
        ("unviable", MUTANT),
    ];
    let rejected = report("diff-rejected", &lists, Some(&rejected_at("src/lib.rs")))?;
    assert!(passes("diff", &rejected)?);
    let elsewhere = report("diff-elsewhere", &lists, Some(&rejected_at("src/other.rs")))?;
    assert!(
        !passes("diff", &elsewhere)?,
        "an error in another file says the build directory may be broken"
    );
    let no_error = report(
        "diff-no-error",
        &lists,
        Some(&format!("\n*** {MUTANT}\n*** result: Failure\n")),
    )?;
    assert!(!passes("diff", &no_error)?);
    Ok(())
}

#[test]
fn a_report_without_its_lists_fails() -> io::Result<()> {
    let partial = report("partial", &[("caught", MUTANT)], None)?;
    assert!(!passes("diff", &partial)?);
    assert!(!passes("sweep", &partial)?, "an unknown lane");
    Ok(())
}
