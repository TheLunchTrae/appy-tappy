//! The Debug page: whether the runtime runs, the newest trace file, the
//! metrics and rows of a chosen one, both logs, and the debug bundle.

use std::time::Duration;

use appytappy_sources::screen::Size;

use crate::automations;
use crate::debug::bundle::{BundleJob, BundleSpec};
use crate::debug::outline::{self, CaptureOutline};
use crate::debug::trace_file::{self, TraceFile, TraceTail};
use crate::debug::trace_view::{RowFilter, TraceMetrics, TraceRange, TraceRows, TraceView};
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
        // A trace folder that cannot be listed still leaves the runtime's state
        // to show.
        let newest = trace_file::newest(&traces);
        let Ok(Some(path)) = newest else {
            self.trace_tail = None;
            return Ok(DebugOverview {
                runtime,
                trace: None,
                trace_problem: newest
                    .err()
                    .map(|error| format!("AppyTappy cannot list the trace files: {error}.")),
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

    /// The trace files, newest first.
    pub(crate) fn trace_files(&self) -> Result<Vec<TraceFile>, String> {
        trace_file::list(&self.debug_stores()?.traces)
            .map_err(|error| format!("AppyTappy cannot list the trace files: {error}."))
    }

    /// The metrics of the trace file `file` over `range`, read on from where
    /// the last call on it stopped.
    pub(crate) fn trace_metrics(
        &mut self,
        file: &str,
        range: TraceRange,
    ) -> Result<TraceMetrics, String> {
        self.trace_view(file).map(|view| view.metrics(range))
    }

    /// The rows of the trace file `file` that `filter` picks, read on from
    /// where the last call on it stopped.
    pub(crate) fn trace_rows(
        &mut self,
        file: &str,
        filter: &RowFilter,
    ) -> Result<TraceRows, String> {
        self.trace_view(file).map(|view| view.rows(filter))
    }

    /// Lets go of the trace file the metrics and rows read, as the Debug
    /// page leaves its trace tabs.
    pub(crate) fn close_trace(&mut self) {
        self.trace_view = None;
    }

    /// The view of the trace file `file`, read on. A view of another file is
    /// let go, as the page reads one at a time.
    fn trace_view(&mut self, file: &str) -> Result<&TraceView, String> {
        if !trace_file::is_trace_name(file) {
            return Err(format!("{file} is not a trace file."));
        }
        let path = self.debug_stores()?.traces.join(file);
        let view = match self.trace_view.take() {
            Some(view) if view.path() == path => view,
            _ => TraceView::new(path),
        };
        let view = self.trace_view.insert(view);
        view.update()
            .map_err(|error| format!("AppyTappy cannot read {file}: {error}."))?;
        Ok(view)
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

    /// The debug bundle `spec` asks for, to make away from this thread.
    pub(crate) fn debug_bundle(&self, spec: BundleSpec) -> Result<BundleJob, String> {
        Ok(BundleJob::new(self.debug_stores()?.clone(), spec))
    }

    /// What the runtime copies out of each frame of the saved application
    /// `app`'s window at `width` by `height` pixels, and where each of its
    /// saved values reads there, for the value editor's preview.
    pub(crate) fn capture_outline(
        &self,
        app: &str,
        width: u32,
        height: u32,
    ) -> Result<CaptureOutline, String> {
        let stores = self.stores()?;
        let found = stores.app(app)?;
        let watched = automations::watched_values(stores, app)?;
        Ok(outline::outline(
            &found.file,
            &watched,
            Size { width, height },
            |name| found.picture(name),
        ))
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

    use appytappy_config::app_file::RegionDef;
    use appytappy_config::automation_file::{
        ActionDef, AutomationDef, ComparisonDef, MediaCommandDef, TargetDef, WhenDef,
    };
    use zip::ZipArchive;

    use crate::debug::trace_file::{HEADER, Spread};
    use crate::owner::test_support::{harness_with, scratch_store, with_saved_bar};

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
    fn the_overview_keeps_what_it_read_and_moves_to_a_newer_trace() {
        let (stores, root) = scratch_store("owner-debug-keep");
        let traces = stores.traces.clone();
        fs::create_dir_all(&traces).expect("folder");
        let mut harness = harness_with(false, Ok(stores));
        // A file larger than one read, with a dropped row just inside the
        // part the first read takes.
        let filler = "1000,owner,handled,Example,,,300,,changed\n";
        let mib = 1024 * 1024 / filler.len();
        let first = traces.join("trace-2026-11-12-100000.csv");
        fs::write(
            &first,
            format!(
                "{HEADER}\n{}500,trace,dropped,,,5,,,\n{}",
                filler.repeat(4 * mib),
                filler.repeat(2 * mib)
            ),
        )
        .expect("write");
        let dropped = |harness: &mut crate::owner::test_support::Harness| {
            harness
                .ask(Owner::debug_overview)
                .expect("overview")
                .trace
                .map(|trace| (trace.file, trace.dropped))
        };
        assert_eq!(
            dropped(&mut harness),
            Some(("trace-2026-11-12-100000.csv".to_owned(), 5))
        );
        // It grows past the dropped row's reach of one read from the end; what
        // was read already is kept.
        let mut grown = fs::OpenOptions::new()
            .append(true)
            .open(&first)
            .expect("open");
        std::io::Write::write_all(&mut grown, filler.repeat(3 * mib).as_bytes()).expect("append");
        drop(grown);
        assert_eq!(
            dropped(&mut harness),
            Some(("trace-2026-11-12-100000.csv".to_owned(), 5))
        );
        let second = traces.join("trace-2026-11-12-110000.csv");
        fs::write(&second, format!("{HEADER}\n{filler}")).expect("write");
        fs::File::options()
            .write(true)
            .open(&first)
            .expect("open")
            .set_modified(std::time::SystemTime::UNIX_EPOCH)
            .expect("age it");
        assert_eq!(
            dropped(&mut harness),
            Some(("trace-2026-11-12-110000.csv".to_owned(), 0)),
            "a newer trace is read from its start"
        );
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_picked_trace_is_read_on_one_file_at_a_time_and_let_go() {
        let (stores, root) = scratch_store("owner-debug-view");
        let traces = stores.traces.clone();
        fs::create_dir_all(&traces).expect("folder");
        let mut harness = harness_with(false, Ok(stores));
        // Larger than one update reads, so each call reads on from the last.
        let row = "1000,reader,read,Example,health,3,1000,,\n";
        let older = "trace-2026-11-12-100000.csv";
        fs::write(
            traces.join(older),
            format!("{HEADER}\n{}", row.repeat(12 * 1024 * 1024 / row.len())),
        )
        .expect("write");
        fs::File::options()
            .write(true)
            .open(traces.join(older))
            .expect("open")
            .set_modified(std::time::SystemTime::UNIX_EPOCH)
            .expect("age it");
        let newer = "trace-2026-11-12-110000.csv";
        fs::write(traces.join(newer), format!("{HEADER}\n{row}{row}")).expect("write");

        let files = harness.ask(|owner| owner.trace_files()).expect("listed");
        let names: Vec<&str> = files.iter().map(|file| file.name.as_str()).collect();
        assert_eq!(names, [newer, older], "newest first");
        assert_eq!(
            harness.ask(|owner| owner.trace_metrics("../trace-a.csv", TraceRange::Whole)),
            Err("../trace-a.csv is not a trace file.".to_owned())
        );

        let read = |harness: &mut crate::owner::test_support::Harness, file: &str| {
            harness
                .ask(|owner| owner.trace_metrics(file, TraceRange::Whole))
                .expect("metrics")
                .progress
                .read
        };
        let first = read(&mut harness, older);
        assert!(read(&mut harness, older) > first, "the same file reads on");
        assert_eq!(
            harness
                .ask(|owner| {
                    owner.trace_rows(
                        newer,
                        &RowFilter {
                            range: TraceRange::Whole,
                            categories: vec![
                                appytappy_config::settings_file::TraceCategory::Readings,
                            ],
                            app: None,
                            took_over_ns: None,
                            search: String::new(),
                        },
                    )
                })
                .expect("rows")
                .matching,
            2
        );
        assert_eq!(read(&mut harness, older), first, "another file let it go");
        assert!(read(&mut harness, older) > first);
        harness.ask(Owner::close_trace);
        assert_eq!(read(&mut harness, older), first, "closing let it go");
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_trace_folder_that_cannot_be_listed_still_shows_the_runtime() {
        let (stores, root) = scratch_store("owner-debug-unlisted");
        fs::create_dir_all(stores.traces.parent().expect("a parent")).expect("folder");
        fs::write(&stores.traces, "not a folder").expect("write");
        let mut harness = harness_with(false, Ok(stores));
        harness.owner.host.runtime = Ok(true);
        let overview = harness.ask(Owner::debug_overview).expect("overview");
        assert_eq!(overview.runtime, Some(true));
        assert!(
            overview
                .trace_problem
                .is_some_and(|problem| problem.starts_with("AppyTappy cannot list the trace files"))
        );
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
        fs::write(app.join("notes.txt"), "mine").expect("another file");
        let pictures = stores.automations.pictures();
        fs::create_dir_all(&pictures).expect("pictures folder");
        fs::write(pictures.join("click-1.png"), [3]).expect("picture");
        fs::write(pictures.join("click-1.png.tmp"), [3]).expect("half-written picture");
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
            .and_then(BundleJob::make)
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
            .and_then(BundleJob::make)
            .expect("bundle");
        let (names, log) = unzip(full, "logs/setup.log");
        assert_eq!(log, "pick_window took 1.0 ms\n");
        assert!(
            names.contains(&"apps/example/shown-1.png".to_owned()),
            "{names:?}"
        );
        assert!(
            names.contains(&"automation pictures/click-1.png".to_owned()),
            "{names:?}"
        );
        assert!(
            names
                .iter()
                .all(|name| !name.contains("notes") && !name.contains("png.tmp")),
            "only pictures: {names:?}"
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
            .and_then(BundleJob::make)
            .expect("bundle");
        assert_eq!(
            unzip(outside, "about.txt").0.len(),
            4,
            "only a trace file in the trace folder goes in"
        );
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn the_outline_shows_what_an_enabled_automation_watches() {
        let (mut harness, root) = with_saved_bar("owner-debug-outline");
        let unwatched = harness
            .ask(|owner| owner.capture_outline("example", 4, 2))
            .expect("outline");
        assert_eq!(unwatched.copies, []);
        assert_eq!(unwatched.values.len(), 1);
        let value = unwatched.values[0].id.clone();
        harness
            .ask(|owner| {
                owner.save_automation(
                    None,
                    AutomationDef {
                        id: String::new(),
                        name: "Pause when full".to_owned(),
                        enabled: true,
                        when: WhenDef {
                            app: "example".to_owned(),
                            value,
                            is: ComparisonDef::AtLeast,
                            target: TargetDef::Number(100.0),
                        },
                        actions: vec![ActionDef::Media {
                            command: MediaCommandDef::Pause,
                            app: None,
                        }],
                        undo: false,
                        cooldown_ms: 0,
                    },
                )
            })
            .expect("saves");
        let watched = harness
            .ask(|owner| owner.capture_outline("example", 4, 2))
            .expect("outline");
        let whole = RegionDef {
            x: 0,
            y: 0,
            width: 4,
            height: 2,
        };
        assert_eq!(watched.copies, [whole]);
        assert!(watched.values[0].watched);
        assert_eq!(watched.values[0].region, Some(whole));
        assert!(
            harness
                .ask(|owner| owner.capture_outline("missing", 4, 2))
                .is_err(),
            "an application that is not saved has no outline"
        );
        fs::remove_dir_all(root).expect("cleans up");
    }
}
