//! Reads the runtime's newest trace file for the Debug page: its newest
//! rows, and the timings the overview shows.
//!
//! The page asks again every few seconds while the trace grows, so a
//! [`TraceTail`] keeps its place in the file and reads only the rows added
//! since. It reads at most [`WINDOW`] bytes at a time: a larger file, or a
//! larger growth between two reads, is read from that far before its end,
//! so a trace of any size costs the owner thread the same.
//! `wiki/Trace.md` describes the file.

use std::collections::VecDeque;
use std::fs::{self, File};
use std::io::{self, Read as _, Seek as _, SeekFrom};
use std::mem;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;

/// The first line of every trace file this version writes.
pub(crate) const HEADER: &str = "time_ns,thread,event,app,item,value,took_ns,cause,detail";

/// How many of the newest rows the overview shows.
const NEWEST: usize = 8;

/// How many of the newest timings of each kind a spread is taken from, so
/// a long trace keeps a fixed amount in memory and its numbers follow what
/// happens now.
const SAMPLES: usize = 10_000;

/// The most a tail reads at a time, from the end of the file.
const WINDOW: u64 = 4 * 1024 * 1024;

/// One row of a trace file, as the page shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct TraceRow {
    /// When it happened, in nanoseconds since the trace started.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) time_ns: u64,
    pub(crate) thread: String,
    pub(crate) event: String,
    pub(crate) app: String,
    pub(crate) item: String,
    pub(crate) value: String,
    /// How long it took, in nanoseconds, where the event says.
    #[cfg_attr(test, ts(type = "number | null"))]
    pub(crate) took_ns: Option<u64>,
    /// When the frame arrived whose reading led to this row.
    #[cfg_attr(test, ts(type = "number | null"))]
    pub(crate) cause: Option<u64>,
    pub(crate) detail: String,
}

/// A middle and a slow time, in nanoseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct Spread {
    /// The median.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) median_ns: u64,
    /// The 95th percentile: 1 in 20 took longer.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) p95_ns: u64,
}

impl Spread {
    /// The spread of `samples`, or `None` without any.
    fn of(samples: &VecDeque<u64>) -> Option<Self> {
        let mut samples: Vec<u64> = samples.iter().copied().collect();
        samples.sort_unstable();
        // The nearest rank: the smallest sample that at least `part`
        // percent of the samples are no larger than.
        let count = samples.len();
        let at = |part: usize| {
            let rank = count.saturating_mul(part).div_ceil(100);
            samples.get(rank.saturating_sub(1)).copied()
        };
        Some(Self {
            median_ns: at(50)?,
            p95_ns: at(95)?,
        })
    }
}

/// What the overview shows of the newest trace file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct TraceSummary {
    /// The file's name.
    pub(crate) file: String,
    /// Its size.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) bytes: u64,
    /// Why its rows cannot be read, such as a trace from another version.
    pub(crate) problem: Option<String>,
    /// From a frame arriving to the action it led to starting, or `None`
    /// when no action started from a frame, as with `actions` or `changes`
    /// not recorded.
    pub(crate) frame_to_action: Option<Spread>,
    /// How long the runtime took to handle each event, or `None` when no
    /// event was recorded, as with `rules` not recorded.
    pub(crate) rule_handling: Option<Spread>,
    /// How long the runtime took to run each automation's actions, or
    /// `None` when none ran, as with `actions` not recorded.
    pub(crate) action_run: Option<Spread>,
    /// How many records the trace dropped, because its buffer was full, in
    /// the part of the file read.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) dropped: u64,
    /// Whether the part read starts after the file's first row, because
    /// the file, or its growth since the last read, was larger than a read
    /// takes. The numbers then cover only the newer rows.
    pub(crate) partial: bool,
    /// The newest rows, oldest first.
    pub(crate) newest: Vec<TraceRow>,
}

/// The trace file in `folder` that changed last, or `None` without one.
///
/// # Errors
///
/// Returns the error from listing the folder, except a folder that is not
/// there yet, which holds no trace.
pub(crate) fn newest(folder: &Path) -> io::Result<Option<PathBuf>> {
    let entries = match fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut newest: Option<(SystemTime, PathBuf)> = None;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let is_trace = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(is_trace_name);
        if !is_trace {
            continue;
        }
        let modified = entry.metadata()?.modified()?;
        if newest.as_ref().is_none_or(|(time, _path)| modified > *time) {
            newest = Some((modified, path));
        }
    }
    Ok(newest.map(|(_time, path)| path))
}

/// A trace file read so far: where the next read starts, and what the rows
/// up to there came to.
#[derive(Debug)]
pub(crate) struct TraceTail {
    path: PathBuf,
    /// How far the file has been read.
    read: u64,
    /// The start of a row whose end was not written yet.
    partial: Vec<u8>,
    /// Whether the first line was this version's header, once read.
    header: Option<bool>,
    /// Whether the bytes up to the next line break are the end of a row
    /// whose start was skipped.
    mid_row: bool,
    /// Whether rows were skipped.
    skipped: bool,
    frame_to_action: VecDeque<u64>,
    rule_handling: VecDeque<u64>,
    action_run: VecDeque<u64>,
    dropped: u64,
    newest: VecDeque<TraceRow>,
}

impl TraceTail {
    /// A tail of `path` that has read nothing yet.
    pub(crate) fn new(path: PathBuf) -> Self {
        Self {
            path,
            read: 0,
            partial: Vec::new(),
            header: None,
            mid_row: false,
            skipped: false,
            frame_to_action: VecDeque::new(),
            rule_handling: VecDeque::new(),
            action_run: VecDeque::new(),
            dropped: 0,
            newest: VecDeque::new(),
        }
    }

    /// The file this tail reads.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Reads the rows added since the last read, at most [`WINDOW`] bytes
    /// of them, and sums up what it read.
    ///
    /// # Errors
    ///
    /// Returns the error from reading the file.
    pub(crate) fn update(&mut self) -> io::Result<TraceSummary> {
        let mut file = File::open(&self.path)?;
        let bytes = file.metadata()?.len();
        if bytes < self.read {
            // The file was replaced by a shorter one: read it again.
            *self = Self::new(self.path.clone());
        }
        if self.header.is_none() {
            match first_line(&mut file)? {
                Some(line) => self.header = Some(line == HEADER),
                // The header is not all written yet.
                None => return Ok(self.summary(bytes)),
            }
        }
        let skip_to = bytes.saturating_sub(WINDOW);
        if skip_to > self.read {
            // Too much to read: start a window before the end, after the next
            // line break, and drop the start of a row cut in two.
            self.read = skip_to;
            self.partial.clear();
            self.mid_row = true;
            self.skipped = true;
        }
        file.seek(SeekFrom::Start(self.read))?;
        let mut added = Vec::new();
        let count = (&mut file).take(WINDOW).read_to_end(&mut added)?;
        self.read = self
            .read
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
        self.take(&added);
        Ok(self.summary(bytes))
    }

    /// Takes `added`, the bytes the file gained, a whole row at a time. A
    /// row ends at a line break outside quotes, since a quoted field can
    /// hold one.
    fn take(&mut self, added: &[u8]) {
        self.partial.extend_from_slice(added);
        let mut start = 0_usize;
        let mut quoted = false;
        let mut rows = Vec::new();
        for (at, byte) in self.partial.iter().enumerate() {
            match byte {
                b'"' if !self.mid_row => quoted = !quoted,
                b'\n' if self.mid_row => {
                    self.mid_row = false;
                    start = at.saturating_add(1);
                },
                b'\n' if !quoted => {
                    rows.push(start..at);
                    start = at.saturating_add(1);
                },
                _ => {},
            }
        }
        for range in rows {
            let line = String::from_utf8_lossy(self.partial.get(range).unwrap_or_default());
            let line = line.strip_suffix('\r').unwrap_or(&line);
            if line == HEADER || self.header != Some(true) {
                continue;
            }
            if let Some(row) = parse_row(line) {
                self.count(row);
            }
        }
        self.partial.drain(..start.min(self.partial.len()));
    }

    /// Adds `row` to the timings and the newest rows.
    fn count(&mut self, row: TraceRow) {
        match row.event.as_str() {
            "started" => {
                if let Some(cause) = row.cause {
                    keep(&mut self.frame_to_action, row.time_ns.saturating_sub(cause));
                }
            },
            "handled" => {
                if let Some(took) = row.took_ns {
                    keep(&mut self.rule_handling, took);
                }
            },
            "done" => {
                if let Some(took) = row.took_ns {
                    keep(&mut self.action_run, took);
                }
            },
            "dropped" => {
                let count = row.value.parse::<u64>().unwrap_or(0);
                self.dropped = self.dropped.saturating_add(count);
            },
            _ => {},
        }
        if self.newest.len() == NEWEST {
            self.newest.pop_front();
        }
        self.newest.push_back(row);
    }

    /// What the rows read so far come to, for a file of `bytes`.
    fn summary(&self, bytes: u64) -> TraceSummary {
        let file = self
            .path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        let problem = (self.header == Some(false)).then(|| {
            "This trace is from another version of AppyTappy, so its rows cannot be read here."
                .to_owned()
        });
        TraceSummary {
            file,
            bytes,
            problem,
            frame_to_action: Spread::of(&self.frame_to_action),
            rule_handling: Spread::of(&self.rule_handling),
            action_run: Spread::of(&self.action_run),
            dropped: self.dropped,
            partial: self.skipped,
            newest: self.newest.iter().cloned().collect(),
        }
    }
}

/// Whether `name` names a trace file: a bare file name, with no folder,
/// that starts `trace-` and ends `.csv`.
pub(crate) fn is_trace_name(name: &str) -> bool {
    let path = Path::new(name);
    path.file_name().is_some_and(|file| file == name)
        && name.starts_with("trace-")
        && path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("csv"))
}

/// The first line of `file`, without its line break, or `None` before a
/// whole line is written. A first line longer than a header is never one,
/// so it reads as an empty line.
fn first_line(file: &mut File) -> io::Result<Option<String>> {
    file.seek(SeekFrom::Start(0))?;
    let mut start = Vec::new();
    let longest = u64::try_from(HEADER.len())
        .unwrap_or(u64::MAX)
        .saturating_add(2);
    file.take(longest).read_to_end(&mut start)?;
    let Some(end) = start.iter().position(|byte| *byte == b'\n') else {
        return Ok((start.len() >= HEADER.len().saturating_add(2)).then(String::new));
    };
    start.truncate(end);
    let line = String::from_utf8_lossy(&start);
    Ok(Some(line.strip_suffix('\r').unwrap_or(&line).to_owned()))
}

/// Adds `sample` to `samples`, dropping the oldest once there are
/// [`SAMPLES`].
fn keep(samples: &mut VecDeque<u64>, sample: u64) {
    if samples.len() == SAMPLES {
        samples.pop_front();
    }
    samples.push_back(sample);
}

/// The row `line` holds, or `None` for a line that is not a row.
pub(crate) fn parse_row(line: &str) -> Option<TraceRow> {
    let fields = split(line);
    let [time, thread, event, app, item, value, took, cause, detail] =
        <[String; 9]>::try_from(fields).ok()?;
    let optional = |field: &str| -> Option<Option<u64>> {
        if field.is_empty() {
            Some(None)
        } else {
            field.parse().ok().map(Some)
        }
    };
    Some(TraceRow {
        time_ns: time.parse().ok()?,
        thread,
        event,
        app,
        item,
        value,
        took_ns: optional(&took)?,
        cause: optional(&cause)?,
        detail,
    })
}

/// The fields of the CSV line `line`. A field in quotes holds its commas,
/// and two quotes in it are one.
fn split(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut characters = line.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '"' if quoted && characters.next_if_eq(&'"').is_some() => field.push('"'),
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(mem::take(&mut field)),
            _ => field.push(character),
        }
    }
    fields.push(field);
    fields
}

#[cfg(test)]
mod tests {
    use std::time::Duration;
    use std::{env, process};

    use super::*;

    #[test]
    fn a_spread_is_the_median_and_95th_percentile() {
        assert_eq!(Spread::of(&VecDeque::new()), None);
        let samples: VecDeque<u64> = (1..=100).rev().collect();
        assert_eq!(
            Spread::of(&samples),
            Some(Spread {
                median_ns: 50,
                p95_ns: 95
            })
        );
        assert_eq!(
            Spread::of(&VecDeque::from([7])),
            Some(Spread {
                median_ns: 7,
                p95_ns: 7
            })
        );
    }

    #[test]
    fn a_small_sample_does_not_hide_its_slowest() {
        assert_eq!(
            Spread::of(&VecDeque::from([100, 1])),
            Some(Spread {
                median_ns: 1,
                p95_ns: 100
            })
        );
        assert_eq!(
            Spread::of(&VecDeque::from([1, 2, 3, 100])).map(|spread| spread.p95_ns),
            Some(100)
        );
    }

    #[test]
    fn only_the_newest_samples_are_kept() {
        let mut samples = VecDeque::new();
        for sample in 0..=u64::try_from(SAMPLES).expect("fits") {
            keep(&mut samples, sample);
        }
        assert_eq!(samples.len(), SAMPLES);
        assert_eq!(samples.front(), Some(&1), "the oldest went");
        assert_eq!(samples.back(), Some(&u64::try_from(SAMPLES).expect("fits")));
    }

    #[test]
    fn a_row_reads_quoted_fields_and_empty_numbers() {
        let row =
            parse_row(r#"12,owner,handled,"Shop, ""main""",Rule,,300,,"a, b""#).expect("a row");
        assert_eq!(row.time_ns, 12);
        assert_eq!(row.app, "Shop, \"main\"");
        assert_eq!(row.took_ns, Some(300));
        assert_eq!(row.cause, None);
        assert_eq!(row.detail, "a, b");
        assert_eq!(parse_row("12,owner,handled"), None, "too few fields");
        assert_eq!(parse_row("x,a,b,c,d,e,,,"), None, "no time");
        assert_eq!(
            parse_row("1,a,b,c,d,e,soon,,"),
            None,
            "a took that is no number"
        );
        assert_eq!(parse_row("1,a,b,c,d,e,,1,,extra"), None, "too many fields");
    }

    #[test]
    fn the_newest_trace_is_the_last_written() {
        let folder = env::temp_dir().join(format!("appytappy-newest-{}", process::id()));
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a folder left by an earlier run may or may not exist"
        )]
        let _ = fs::remove_dir_all(&folder);
        assert_eq!(newest(&folder).expect("missing is fine"), None);
        fs::create_dir_all(&folder).expect("folder");
        fs::write(folder.join("notes.csv"), "").expect("write");
        fs::write(folder.join("trace-old.csv"), "").expect("write");
        fs::write(folder.join("trace-new.CSV"), "").expect("write");
        fs::write(folder.join("trace-mid.csv"), "").expect("write");
        // Each gets its own time, so the order the folder lists them in does
        // not matter.
        for (name, seconds) in [
            ("trace-old.csv", 3_600),
            ("trace-new.CSV", 10_800),
            ("trace-mid.csv", 7_200),
        ] {
            let when = SystemTime::UNIX_EPOCH
                .checked_add(Duration::from_secs(seconds))
                .expect("fits");
            File::options()
                .write(true)
                .open(folder.join(name))
                .expect("open")
                .set_modified(when)
                .expect("set its time");
        }
        fs::write(folder.join("trace-text.txt"), "").expect("write");
        assert_eq!(
            newest(&folder).expect("listed"),
            Some(folder.join("trace-new.CSV"))
        );
        fs::remove_dir_all(&folder).expect("clean up");
    }

    #[test]
    fn a_shorter_file_is_read_again_from_the_start() {
        let folder = env::temp_dir().join(format!("appytappy-tail-{}", process::id()));
        fs::create_dir_all(&folder).expect("folder");
        let path = folder.join("trace-a.csv");
        fs::write(
            &path,
            format!("{HEADER}\n1,a,b,c,d,e,,,\n2,a,b,c,d,e,,,\r\n"),
        )
        .expect("write");
        let mut tail = TraceTail::new(path.clone());
        assert_eq!(
            tail.update().expect("read").newest.len(),
            2,
            "a CR is not part of a row"
        );
        fs::write(&path, format!("{HEADER}\n3,a,b,c,d,e,,,\n")).expect("write");
        let summary = tail.update().expect("read");
        assert_eq!(
            summary
                .newest
                .iter()
                .map(|row| row.time_ns)
                .collect::<Vec<_>>(),
            [3]
        );
        fs::remove_dir_all(&folder).expect("clean up");
    }

    /// A scratch folder for one test, empty.
    fn scratch(name: &str) -> PathBuf {
        let folder = env::temp_dir().join(format!("appytappy-{name}-{}", process::id()));
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a folder left by an earlier run may or may not exist"
        )]
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).expect("folder");
        folder
    }

    #[test]
    fn a_line_break_in_quotes_stays_in_its_row() {
        let folder = scratch("tail-quotes");
        let path = folder.join("trace-a.csv");
        fs::write(
            &path,
            format!("{HEADER}\n1,action,done,\"Two\nlines\",Pause,,40,,ok\n2,a,b,c,d,e,,,\n"),
        )
        .expect("write");
        let summary = TraceTail::new(path).update().expect("read");
        assert_eq!(summary.newest.len(), 2);
        assert_eq!(
            summary.newest.first().map(|row| row.app.as_str()),
            Some("Two\nlines")
        );
        assert_eq!(summary.action_run.map(|spread| spread.median_ns), Some(40));
        assert!(!summary.partial);
        fs::remove_dir_all(&folder).expect("clean up");
    }

    #[test]
    fn a_header_still_being_written_waits() {
        let folder = scratch("tail-header");
        let path = folder.join("trace-a.csv");
        fs::write(&path, "time_ns,thr").expect("write");
        let mut tail = TraceTail::new(path.clone());
        let early = tail.update().expect("read");
        assert_eq!(early.problem, None, "not another version yet");
        fs::write(&path, format!("{HEADER}\n1,a,b,c,d,e,,,\n")).expect("write");
        assert_eq!(tail.update().expect("read").newest.len(), 1);
        fs::remove_dir_all(&folder).expect("clean up");
    }

    #[test]
    fn a_trace_name_is_a_bare_csv_file_that_starts_trace() {
        assert!(is_trace_name("trace-2026-11-12-100000.csv"));
        assert!(is_trace_name("trace-a.CSV"));
        assert!(!is_trace_name("notes.csv"), "another csv");
        assert!(!is_trace_name("trace-a.txt"), "not a csv");
        assert!(!is_trace_name("../trace-a.csv"), "in another folder");
        assert!(!is_trace_name("logs/trace-a.csv"), "in a folder");
    }

    #[test]
    fn a_folder_that_cannot_be_listed_is_an_error() {
        let folder = scratch("newest-unreadable");
        let file = folder.join("file.txt");
        fs::write(&file, "x").expect("write");
        assert!(newest(&file.join("inside")).is_err());
        fs::remove_dir_all(&folder).expect("clean up");
    }

    #[test]
    fn a_trace_with_another_header_shows_no_rows() {
        let folder = scratch("tail-other");
        let path = folder.join("trace-a.csv");
        fs::write(
            &path,
            "time,what
1,a,b,c,d,e,,,
",
        )
        .expect("write");
        let summary = TraceTail::new(path).update().expect("read");
        assert!(summary.problem.is_some());
        assert_eq!(summary.newest.len(), 0);
        fs::remove_dir_all(&folder).expect("clean up");
    }

    #[test]
    fn a_row_cut_in_two_is_dropped_up_to_its_line_break() {
        let times = |added: &[u8]| {
            let mut tail = TraceTail::new(PathBuf::from("trace-a.csv"));
            tail.header = Some(true);
            tail.mid_row = true;
            tail.take(added);
            tail.summary(0)
                .newest
                .iter()
                .map(|row| row.time_ns)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            times(b"1,a,b,c,d,e,,,\n2,a,b,c,d,e,,,\n"),
            [2],
            "even a cut that looks like a whole row"
        );
        assert_eq!(
            times(b"x\",1\n2,a,b,c,d,e,,,\n"),
            [2],
            "a quote in the cut part opens nothing"
        );
    }

    #[test]
    fn a_large_file_is_read_from_near_its_end() {
        let folder = scratch("tail-window");
        let path = folder.join("trace-a.csv");
        let row = "1,owner,handled,App,,,500,,changed\n";
        let rows = usize::try_from(WINDOW).expect("fits") / row.len() + 100;
        let mut text = format!("{HEADER}\n");
        text.push_str(&row.repeat(rows));
        text.push_str("9,action,done,App,,,700,,ok\n");
        fs::write(&path, &text).expect("write");
        let summary = TraceTail::new(path).update().expect("read");
        assert!(summary.partial, "the start was skipped");
        assert_eq!(summary.problem, None, "the header is still checked");
        assert_eq!(summary.newest.last().map(|row| row.time_ns), Some(9));
        assert_eq!(
            summary.rule_handling.map(|spread| spread.median_ns),
            Some(500)
        );
        assert!(
            summary.newest.iter().all(|row| row.event != "ndled"),
            "the row cut in two is dropped"
        );
        fs::remove_dir_all(&folder).expect("clean up");
    }
}
