//! The Debug page: whether the runtime runs, the newest trace file, both
//! logs, and the debug bundle.

use std::time::Duration;

use crate::debug::bundle::{self, BundleSpec};
use crate::debug::trace_file::{self, TraceTail};
use crate::debug::{DebugOverview, LogName, log_tail};
use crate::settings;
use crate::stores::Stores;

use super::Owner;
use super::host::Host;

impl<H: Host> Owner<H> {
    /// What the Debug page's overview shows. The newest trace file is read
    /// from where the last call stopped, so a page that asks every few
    /// seconds reads only the rows added since.
    pub(crate) fn debug_overview(&mut self) -> Result<DebugOverview, String> {
        let runtime = self.host.runtime_runs().ok();
        let traces = self.debug_stores()?.traces.clone();
        let newest = trace_file::newest(&traces)
            .map_err(|error| format!("AppyTappy cannot list the trace files: {error}."))?;
        let Some(path) = newest else {
            self.trace_tail = None;
            return Ok(DebugOverview {
                runtime,
                trace: None,
                trace_problem: None,
            });
        };
        let tail = match self.trace_tail.take() {
            Some(tail) if tail.path() == path => tail,
            _ => TraceTail::new(path),
        };
        let tail = self.trace_tail.insert(tail);
        let (trace, trace_problem) = match tail.update() {
            Ok(summary) => (Some(summary), None),
            Err(error) => (
                None,
                Some(format!("AppyTappy cannot read the newest trace: {error}.")),
            ),
        };
        Ok(DebugOverview {
            runtime,
            trace,
            trace_problem,
        })
    }

    /// The newest lines of `log`, or nothing before anything was logged.
    pub(crate) fn read_log(&self, log: LogName) -> Result<String, String> {
        let path = self.debug_stores()?.logs.join(log.file());
        log_tail(&path).map_err(|error| format!("AppyTappy cannot read {}: {error}.", log.file()))
    }

    /// Opens the logs folder in File Explorer.
    #[mutants::skip] // Opens a window, checked by hand on Windows.
    pub(crate) fn open_logs_folder(&self) -> Result<(), String> {
        let logs = &self.debug_stores()?.logs;
        std::fs::create_dir_all(logs)
            .and_then(|()| settings::open_folder(logs))
            .map_err(|error| format!("AppyTappy cannot open the logs folder: {error}."))
    }

    /// The debug bundle, as the bytes of a zip file.
    pub(crate) fn debug_bundle(&self, spec: BundleSpec) -> Result<Vec<u8>, String> {
        bundle::build(self.debug_stores()?, spec)
            .map_err(|error| format!("AppyTappy cannot make the debug bundle: {error}."))
    }

    /// Records in `setup.log` that `command` took `took`, and why it failed,
    /// if it did. A command's work calls this as it ends on this thread.
    pub(crate) fn record_command(
        &mut self,
        command: &str,
        took: Duration,
        failure: Option<&String>,
    ) {
        self.setup_log.record(command, took, failure);
    }

    /// Where the files are, or why that is unknown, as the user reads it.
    fn debug_stores(&self) -> Result<&Stores, String> {
        self.stores
            .as_ref()
            .map_err(|reason| format!("AppyTappy cannot find its files: {reason}."))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::{Cursor, Read as _};

    use zip::ZipArchive;

    use crate::debug::trace_file::{HEADER, Spread};
    use crate::owner::test_support::{harness_with, scratch_store};

    use super::*;

    /// The names in the zip file `bytes`, and the text of `name` in it.
    fn unzip(bytes: Vec<u8>, name: &str) -> (Vec<String>, String) {
        let mut archive = ZipArchive::new(Cursor::new(bytes)).expect("a zip file");
        let names = archive
            .file_names()
            .map(|name| name.expect("a name").into_owned())
            .collect();
        let mut text = String::new();
        archive
            .by_name(name)
            .expect("in the bundle")
            .read_to_string(&mut text)
            .expect("text");
        (names, text)
    }

    #[test]
    fn the_overview_follows_the_newest_trace_as_it_grows() {
        let (stores, root) = scratch_store("owner-debug-trace");
        let traces = stores.traces.clone();
        let mut harness = harness_with(false, Ok(stores));
        harness.owner.host.runtime = Ok(true);
        let empty = harness.ask(Owner::debug_overview).expect("overview");
        assert_eq!(empty.runtime, Some(true));
        assert_eq!(empty.trace, None, "no trace yet");

        fs::create_dir_all(&traces).expect("folder");
        let file = traces.join("trace-2026-11-12-100000.csv");
        let rows = [
            "0,trace,start,,,,,,2026-11-12 10:00:00 0.1.0",
            "1000,owner,handled,Example,,,300,1000,changed",
            "1200,action,started,Example,Pause,,,1000,",
        ];
        fs::write(
            &file,
            format!("{HEADER}\n{}\n500,trace,dro", rows.join("\n")),
        )
        .expect("write");
        let first = harness
            .ask(Owner::debug_overview)
            .expect("overview")
            .trace
            .expect("a trace");
        assert_eq!(first.file, "trace-2026-11-12-100000.csv");
        assert_eq!(first.newest.len(), 3, "the unfinished row waits");
        assert_eq!(
            first.frame_to_action,
            Some(Spread {
                median_ns: 200,
                p95_ns: 200
            })
        );
        assert_eq!(
            first.rule_handling.map(|spread| spread.median_ns),
            Some(300)
        );

        let mut grown = fs::read_to_string(&file).expect("read");
        grown.push_str("pped,,,4,,,\n");
        fs::write(&file, grown).expect("write");
        harness.owner.host.runtime = Err("no mutexes here".to_owned());
        let second = harness.ask(Owner::debug_overview).expect("overview");
        assert_eq!(second.runtime, None, "a check that fails says nothing");
        let trace = second.trace.expect("a trace");
        assert_eq!(trace.dropped, 4, "the finished row counts");
        assert_eq!(trace.newest.len(), 4);
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_trace_from_another_version_says_so() {
        let (stores, root) = scratch_store("owner-debug-version");
        fs::create_dir_all(&stores.traces).expect("folder");
        fs::write(
            stores.traces.join("trace-2026-11-12-100000.csv"),
            "time,what\n1,read\n",
        )
        .expect("write");
        let mut harness = harness_with(false, Ok(stores));
        let trace = harness
            .ask(Owner::debug_overview)
            .expect("overview")
            .trace
            .expect("a trace");
        assert!(
            trace
                .problem
                .is_some_and(|problem| problem.contains("another version"))
        );
        assert_eq!(trace.newest.len(), 0, "no rows yet");
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_log_reads_its_end_or_nothing() {
        let (stores, root) = scratch_store("owner-debug-log");
        fs::create_dir_all(&stores.logs).expect("folder");
        fs::write(stores.logs.join("runtime.log"), "Started\n").expect("write");
        let mut harness = harness_with(false, Ok(stores));
        assert_eq!(
            harness.ask(|owner| owner.read_log(LogName::Runtime)),
            Ok("Started\n".to_owned())
        );
        assert_eq!(
            harness.ask(|owner| owner.read_log(LogName::Setup)),
            Ok(String::new())
        );
        assert_eq!(
            harness_with(false, Err("APPDATA is not set".to_owned()))
                .ask(|owner| owner.read_log(LogName::Setup)),
            Err("AppyTappy cannot find its files: APPDATA is not set.".to_owned())
        );
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn the_bundle_holds_the_files_and_the_pictures_and_trace_only_when_asked() {
        let (stores, root) = scratch_store("owner-debug-bundle");
        let locations = stores.locations.clone();
        let app = locations.apps().join("example");
        fs::create_dir_all(&app).expect("app folder");
        fs::write(locations.settings(), "{\"format\": 1}").expect("settings");
        fs::write(app.join("app.json"), "{}").expect("app file");
        fs::write(app.join("shown-1.png"), [1, 2]).expect("picture");
        fs::create_dir_all(&stores.logs).expect("logs");
        fs::write(stores.logs.join("setup.log"), "pick_window took 1.0 ms\n").expect("log");
        fs::create_dir_all(&stores.traces).expect("traces");
        fs::write(stores.traces.join("trace-2026-11-12-100000.csv"), HEADER).expect("trace");
        let mut harness = harness_with(false, Ok(stores));

        let plain = harness
            .ask(|owner| {
                owner.debug_bundle(BundleSpec {
                    pictures: false,
                    trace: None,
                })
            })
            .expect("bundle");
        let (names, settings) = unzip(plain, "settings.json");
        assert_eq!(settings, "{\"format\": 1}", "files go in as they are");
        assert_eq!(
            names,
            [
                "about.txt",
                "settings.json",
                "apps/example/app.json",
                "logs/setup.log"
            ]
        );

        let full = harness
            .ask(|owner| {
                owner.debug_bundle(BundleSpec {
                    pictures: true,
                    trace: Some("trace-2026-11-12-100000.csv".to_owned()),
                })
            })
            .expect("bundle");
        let (names, log) = unzip(full, "logs/setup.log");
        assert_eq!(log, "pick_window took 1.0 ms\n");
        assert!(
            names.contains(&"apps/example/shown-1.png".to_owned()),
            "{names:?}"
        );
        assert!(
            names.contains(&"traces/trace-2026-11-12-100000.csv".to_owned()),
            "{names:?}"
        );
        let outside = harness
            .ask(|owner| {
                owner.debug_bundle(BundleSpec {
                    pictures: false,
                    trace: Some("../settings.json".to_owned()),
                })
            })
            .expect("bundle");
        assert_eq!(
            unzip(outside, "about.txt").0.len(),
            4,
            "only a trace file in the trace folder goes in"
        );
        fs::remove_dir_all(root).expect("clean up");
    }
}
