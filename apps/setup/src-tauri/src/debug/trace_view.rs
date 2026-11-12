//! The Debug page's metrics and trace rows: one trace file, read from its
//! first row, a window at a time, as the page asks every second.
//!
//! A [`TraceView`] keeps the file's newest [`ROWS`] rows for the rows
//! table, with each name held once, and counts and times for the metrics.
//! Counts are kept for the whole file and by the second for its last
//! [`RECENT_S`] seconds, and times as the newest [`SAMPLES`] of each kind,
//! so a long trace holds about the same amount, more only for each value and
//! automation it names, and each range reads only those.
//! Each row's time is its time since the trace started, as the file has it.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::io;
use std::mem;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use appytappy_config::settings_file::TraceCategory;
use serde::{Deserialize, Serialize};

use crate::debug::trace_file::{Behind, RowReader, Spread, TraceRow};

/// How many of the newest rows the rows table can filter.
pub(crate) const ROWS: usize = 100_000;

/// How many of the newest times of each kind are kept.
pub(crate) const SAMPLES: usize = 20_000;

/// How many rows a query of the rows answers at most, the newest.
const SHOWN: usize = 500;

/// How many seconds before a trace's newest row its counts are kept by the
/// second, as long as the longest range but the whole trace.
const RECENT_S: u64 = 600;

/// How many windows an update reads at most, so a large file is read over a
/// few calls rather than holding the owner thread.
const WINDOWS: usize = 2;

/// Nanoseconds in a second.
const NS_PER_S: u64 = 1_000_000_000;

/// How many bars the frame to action chart aims for.
const BARS: u64 = 24;

/// Which of a trace's rows the metrics and the rows count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) enum TraceRange {
    /// Those in the minute up to the newest row.
    LastMinute,
    /// Those in the 10 minutes up to the newest row.
    LastTenMinutes,
    /// All of them.
    Whole,
}

impl TraceRange {
    /// How long the range lasts, or `None` for the whole trace.
    const fn span_ns(self) -> Option<u64> {
        match self {
            Self::LastMinute => Some(60 * NS_PER_S),
            Self::LastTenMinutes => Some(RECENT_S * NS_PER_S),
            Self::Whole => None,
        }
    }
}

/// The metrics of one trace file over one range. Counts go by whole
/// seconds, so a range's counts can take in up to a second before it, while
/// its times and rows start where it does.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct TraceMetrics {
    /// How much of the file is read.
    pub(crate) progress: ReadProgress,
    /// The categories the file holds rows of. A number of another category
    /// was not recorded.
    pub(crate) recorded: Vec<TraceCategory>,
    /// How long the range covers up to the newest row: from where it starts,
    /// or from the first row when that is later.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) span_ns: u64,
    /// From a frame arriving to the action it led to starting.
    pub(crate) frame_to_action: Option<Spread>,
    /// How those times spread.
    pub(crate) chart: Option<Chart>,
    /// How long the runtime took to handle each event.
    pub(crate) rule_handling: Option<Spread>,
    /// How long each automation's actions took to run.
    pub(crate) action_run: Option<Spread>,
    /// How many values were read.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) reads: u64,
    /// How many values changed.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) changes: u64,
    /// How many automations' actions started.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) actions: u64,
    /// How many automations' actions failed.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) failed: u64,
    /// How many records the trace dropped.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) dropped: u64,
    /// Each value read, by application and then name.
    pub(crate) values: Vec<ValueMetrics>,
    /// Each automation that fired or ran, by application and then name.
    pub(crate) automations: Vec<AutomationMetrics>,
    /// Whether a time comes from only the newest [`SAMPLES`] of its kind,
    /// as the range holds more.
    pub(crate) sampled: bool,
}

/// How far a [`TraceView`] has read its file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReadProgress {
    /// The file's name.
    pub(crate) file: String,
    /// The file's size.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) bytes: u64,
    /// How much of it is read. Less than `bytes` while it is read.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) read: u64,
    /// Why its rows cannot be read, such as a trace from another version.
    pub(crate) problem: Option<String>,
    /// Whether some rows could not be read, as when a quoted field went
    /// wrong, so the numbers leave them out.
    pub(crate) skipped: bool,
}

/// One value's numbers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct ValueMetrics {
    pub(crate) app: String,
    pub(crate) name: String,
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) reads: u64,
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) changes: u64,
    /// When it last changed, in the whole trace.
    #[cfg_attr(test, ts(type = "number | null"))]
    pub(crate) last_change_ns: Option<u64>,
    /// How long its check took.
    pub(crate) check: Option<Spread>,
}

/// One automation's numbers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct AutomationMetrics {
    pub(crate) app: String,
    pub(crate) name: String,
    /// How many times its actions started.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) ran: u64,
    /// How many times its cooldown held it back.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) held: u64,
    /// How many times its actions failed.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) failed: u64,
    /// From a frame arriving to its actions starting.
    pub(crate) frame_to_action: Option<Spread>,
}

/// How many times fall in each bar of a chart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct Chart {
    /// How long each bar spans; bar `n` counts the times from `n` bars to
    /// `n + 1`.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) bar_ns: u64,
    /// How many times each bar counts.
    #[cfg_attr(test, ts(type = "number[]"))]
    pub(crate) counts: Vec<u64>,
    /// How many times come after the last bar, as a few far slower times
    /// would leave the rest too thin to read.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) over: u64,
    /// How many times in all.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) samples: u64,
}

/// Which rows the rows table shows.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct RowFilter {
    pub(crate) range: TraceRange,
    /// The categories of row to show. A trace's own rows count as
    /// lifecycle rows.
    pub(crate) categories: Vec<TraceCategory>,
    /// The application, or `None` for every one.
    pub(crate) app: Option<String>,
    /// Only rows that took longer than this, in nanoseconds.
    #[cfg_attr(test, ts(type = "number | null"))]
    pub(crate) took_over_ns: Option<u64>,
    /// Text the row's thread, event, application, item, value or detail
    /// holds, in any case; empty for any row.
    pub(crate) search: String,
}

/// The rows a [`RowFilter`] picks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct TraceRows {
    /// How much of the file is read.
    pub(crate) progress: ReadProgress,
    /// The newest rows it picks, at most [`SHOWN`], oldest first.
    pub(crate) rows: Vec<TraceRow>,
    /// How many rows it picks.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) matching: u64,
    /// How many rows are held to pick from, the newest [`ROWS`].
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) held: u64,
    /// How many rows were read.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) total: u64,
    /// The applications the held rows name, in order.
    pub(crate) apps: Vec<String>,
}

/// A name the rows share, held once.
type Name = Arc<str>;

/// An application and a value's or automation's name.
type Key = (Name, Name);

/// Each text the held rows use, held once and named by its place, with how
/// many of them use it. A text no held row uses any more is let go and its
/// place used again, so names of dropped rows, such as values read once, never
/// pile up.
#[derive(Debug, Default)]
struct Names {
    places: HashMap<Name, u32>,
    /// Each place's text and how many held rows use it.
    entries: Vec<(Name, u32)>,
    /// The places let go, to use again.
    free: Vec<u32>,
}

impl Names {
    /// The place of `text` for one more held row, added if it is new.
    fn hold(&mut self, text: &str) -> u32 {
        if let Some(&place) = self.places.get(text) {
            if let Some(entry) = self.entries.get_mut(place as usize) {
                entry.1 = entry.1.saturating_add(1);
            }
            return place;
        }
        let name: Name = Arc::from(text);
        let entry = (Arc::clone(&name), 1);
        let place = if let Some(place) = self.free.pop() {
            if let Some(free) = self.entries.get_mut(place as usize) {
                *free = entry;
            }
            place
        } else {
            self.entries.push(entry);
            u32::try_from(self.entries.len().saturating_sub(1)).unwrap_or(u32::MAX)
        };
        self.places.insert(name, place);
        place
    }

    /// Lets go of one held row's use of `place`, and of its text once no
    /// held row uses it.
    fn release(&mut self, place: u32) {
        let Some(entry) = self.entries.get_mut(place as usize) else {
            return;
        };
        entry.1 = entry.1.saturating_sub(1);
        if entry.1 == 0 {
            self.places.remove(&*entry.0);
            entry.0 = Name::default();
            self.free.push(place);
        }
    }

    /// `text` as held, or a new copy when no held row uses it.
    fn name(&self, text: &str) -> Name {
        self.places
            .get_key_value(text)
            .map_or_else(|| Arc::from(text), |(name, _)| Arc::clone(name))
    }

    /// The text at `place`. Every place a row holds came from
    /// [`Self::hold`], so it is always there.
    fn get(&self, place: u32) -> &str {
        self.entries
            .get(place as usize)
            .map_or("", |(name, _)| name)
    }
}

/// One held row, its texts named by their place in [`Names`].
#[derive(Debug, Clone, Copy)]
struct Row {
    time_ns: u64,
    took_ns: Option<u64>,
    cause: Option<u64>,
    category: TraceCategory,
    /// The thread, event, application, item, value, and detail.
    texts: [u32; 6],
}

/// A count over the whole trace and by the second for its last
/// [`RECENT_S`] seconds.
#[derive(Debug, Default)]
struct Tally {
    total: u64,
    /// Each second since the trace started that has a count, and its count,
    /// oldest first.
    seconds: VecDeque<(u64, u64)>,
}

impl Tally {
    /// Adds `count` at `time_ns`.
    fn add(&mut self, time_ns: u64, count: u64) {
        self.total = self.total.saturating_add(count);
        let second = time_ns / NS_PER_S;
        // Most rows land in the newest second.
        if let Some(newest) = self.seconds.back_mut().filter(|newest| newest.0 == second) {
            newest.1 = newest.1.saturating_add(count);
            return;
        }
        // Rows from different threads can arrive a little out of order, so
        // a second can come after a later one.
        let at = self.seconds.iter().rposition(|(each, _)| *each <= second);
        match at.and_then(|at| self.seconds.get_mut(at).map(|entry| (at, entry))) {
            Some((_, entry)) if entry.0 == second => entry.1 = entry.1.saturating_add(count),
            Some((at, _)) => self.seconds.insert(at.saturating_add(1), (second, count)),
            None => self.seconds.push_front((second, count)),
        }
    }

    /// Drops the seconds from before [`RECENT_S`] seconds before `newest_ns`.
    fn trim(&mut self, newest_ns: u64) {
        let keep = (newest_ns / NS_PER_S).saturating_sub(RECENT_S);
        while self
            .seconds
            .front()
            .is_some_and(|(second, _)| *second < keep)
        {
            self.seconds.pop_front();
        }
    }

    /// The count from `from_ns` on, by whole seconds, or the whole count.
    fn since(&self, from_ns: Option<u64>) -> u64 {
        from_ns.map_or(self.total, |from| {
            let first = from / NS_PER_S;
            self.seconds
                .iter()
                .filter(|(second, _)| *second >= first)
                .map(|(_, count)| *count)
                .sum()
        })
    }
}

/// The newest [`SAMPLES`] times of one kind, each with when it happened.
#[derive(Debug, Default)]
struct Samples {
    kept: VecDeque<(u64, u64)>,
    /// Whether older times were dropped.
    dropped: bool,
}

impl Samples {
    /// Adds `sample`, from `time_ns`.
    fn add(&mut self, time_ns: u64, sample: u64) {
        if self.kept.len() == SAMPLES {
            self.kept.pop_front();
            self.dropped = true;
        }
        self.kept.push_back((time_ns, sample));
    }

    /// The times from `from_ns` on, sorted, and whether some in the range
    /// were dropped.
    fn since(&self, from_ns: Option<u64>) -> (Vec<u64>, bool) {
        let mut times: Vec<u64> = self
            .kept
            .iter()
            .filter(|(time, _)| from_ns.is_none_or(|from| *time >= from))
            .map(|(_, sample)| *sample)
            .collect();
        times.sort_unstable();
        let short = self.dropped
            && self
                .kept
                .front()
                .is_some_and(|(oldest, _)| from_ns.is_none_or(|from| *oldest > from));
        (times, short)
    }
}

/// One value's counts and times.
#[derive(Debug, Default)]
struct ValueStats {
    reads: Tally,
    changes: Tally,
    last_change_ns: Option<u64>,
    checks: Samples,
}

/// One automation's counts and times.
#[derive(Debug, Default)]
struct AutomationStats {
    ran: Tally,
    held: Tally,
    failed: Tally,
    frame_to_action: Samples,
}

/// One trace file read so far, for the Debug page's metrics and rows.
#[derive(Debug)]
pub(crate) struct TraceView {
    reader: RowReader,
    bytes: u64,
    names: Names,
    rows: VecDeque<Row>,
    total: u64,
    first_ns: Option<u64>,
    newest_ns: u64,
    recorded: BTreeSet<TraceCategory>,
    frame_to_action: Samples,
    rule_handling: Samples,
    action_run: Samples,
    reads: Tally,
    changes: Tally,
    actions: Tally,
    failed: Tally,
    dropped: Tally,
    values: BTreeMap<Key, ValueStats>,
    automations: BTreeMap<Key, AutomationStats>,
}

impl TraceView {
    /// A view of `path` that has read nothing yet.
    pub(crate) fn new(path: PathBuf) -> Self {
        Self::reading(RowReader::new(path))
    }

    /// A view that holds nothing, reading on with `reader`.
    fn reading(reader: RowReader) -> Self {
        Self {
            reader,
            bytes: 0,
            names: Names::default(),
            rows: VecDeque::new(),
            total: 0,
            first_ns: None,
            newest_ns: 0,
            recorded: BTreeSet::new(),
            frame_to_action: Samples::default(),
            rule_handling: Samples::default(),
            action_run: Samples::default(),
            reads: Tally::default(),
            changes: Tally::default(),
            actions: Tally::default(),
            failed: Tally::default(),
            dropped: Tally::default(),
            values: BTreeMap::new(),
            automations: BTreeMap::new(),
        }
    }

    /// The file this view reads.
    pub(crate) fn path(&self) -> &Path {
        self.reader.path()
    }

    /// Reads on from where the last update stopped, at most [`WINDOWS`]
    /// windows of rows. A file shorter than what was read was replaced, so
    /// it is read again from its start.
    ///
    /// # Errors
    ///
    /// Returns the error from reading the file.
    pub(crate) fn update(&mut self) -> io::Result<()> {
        for _ in 0..WINDOWS {
            let mut rows = Vec::new();
            let read = self.reader.read(Behind::CatchUp, |row| rows.push(row))?;
            if read.restarted {
                self.restart();
            }
            self.bytes = read.bytes;
            for row in rows {
                self.add(&row);
            }
            if !read.full {
                break;
            }
        }
        self.trim();
        Ok(())
    }

    /// Lets go of everything read, keeping the reader, which started its
    /// file again.
    fn restart(&mut self) {
        let reader = mem::replace(&mut self.reader, RowReader::new(PathBuf::new()));
        *self = Self::reading(reader);
    }

    /// Adds `row` to the counts, the times and the held rows.
    fn add(&mut self, row: &TraceRow) {
        let category = category(&row.event);
        let time = row.time_ns;
        self.total = self.total.saturating_add(1);
        self.first_ns = Some(self.first_ns.map_or(time, |first| first.min(time)));
        self.newest_ns = self.newest_ns.max(time);
        self.recorded.insert(category);
        self.hold(row, category);
        // The held row holds these names, so a value or automation seen before
        // costs no new text.
        let key: Key = (self.names.name(&row.app), self.names.name(&row.item));
        let from_frame = row.cause.map(|cause| time.saturating_sub(cause));
        match row.event.as_str() {
            "read" => {
                self.reads.add(time, 1);
                let value = self.values.entry(key).or_default();
                value.reads.add(time, 1);
                if let Some(took) = row.took_ns {
                    value.checks.add(time, took);
                }
            },
            "changed" => {
                self.changes.add(time, 1);
                let value = self.values.entry(key).or_default();
                value.changes.add(time, 1);
                value.last_change_ns =
                    Some(value.last_change_ns.map_or(time, |last| last.max(time)));
            },
            "handled" => {
                if let Some(took) = row.took_ns {
                    self.rule_handling.add(time, took);
                }
            },
            "held" => self.automations.entry(key).or_default().held.add(time, 1),
            "started" => {
                self.actions.add(time, 1);
                let automation = self.automations.entry(key).or_default();
                automation.ran.add(time, 1);
                if let Some(sample) = from_frame {
                    automation.frame_to_action.add(time, sample);
                    self.frame_to_action.add(time, sample);
                }
            },
            "done" => {
                if let Some(took) = row.took_ns {
                    self.action_run.add(time, took);
                }
                if row.detail == "failed" {
                    self.failed.add(time, 1);
                    self.automations.entry(key).or_default().failed.add(time, 1);
                }
            },
            "dropped" => self.dropped.add(time, row.value.parse().unwrap_or(0)),
            _ => {},
        }
    }

    /// Holds `row` for the rows table, dropping the oldest past [`ROWS`].
    fn hold(&mut self, row: &TraceRow, category: TraceCategory) {
        if self.rows.len() == ROWS
            && let Some(dropped) = self.rows.pop_front()
        {
            for text in dropped.texts {
                self.names.release(text);
            }
        }
        let texts = [
            &row.thread,
            &row.event,
            &row.app,
            &row.item,
            &row.value,
            &row.detail,
        ]
        .map(|text| self.names.hold(text));
        self.rows.push_back(Row {
            time_ns: row.time_ns,
            took_ns: row.took_ns,
            cause: row.cause,
            category,
            texts,
        });
    }

    /// Drops the counts by the second that no range reads any more.
    fn trim(&mut self) {
        let newest = self.newest_ns;
        let tallies = [
            &mut self.reads,
            &mut self.changes,
            &mut self.actions,
            &mut self.failed,
            &mut self.dropped,
        ];
        for tally in tallies {
            tally.trim(newest);
        }
        for value in self.values.values_mut() {
            value.reads.trim(newest);
            value.changes.trim(newest);
        }
        for automation in self.automations.values_mut() {
            automation.ran.trim(newest);
            automation.held.trim(newest);
            automation.failed.trim(newest);
        }
    }

    /// How much of the file is read.
    fn progress(&self) -> ReadProgress {
        ReadProgress {
            file: self.reader.file_name(),
            bytes: self.bytes,
            read: self.reader.position(),
            problem: self.reader.problem(),
            skipped: self.reader.skipped(),
        }
    }

    /// Where `range` starts, or `None` for the whole trace.
    fn from(&self, range: TraceRange) -> Option<u64> {
        range
            .span_ns()
            .map(|span| self.newest_ns.saturating_sub(span))
    }

    /// The metrics over `range`.
    pub(crate) fn metrics(&self, range: TraceRange) -> TraceMetrics {
        let from = self.from(range);
        let mut sampled = false;
        let mut spread = |samples: &Samples| {
            let (times, short) = samples.since(from);
            sampled |= short;
            (Spread::of_sorted(&times), times)
        };
        let (frame_to_action, frame_times) = spread(&self.frame_to_action);
        let (rule_handling, _) = spread(&self.rule_handling);
        let (action_run, _) = spread(&self.action_run);
        let values = self
            .values
            .iter()
            .map(|((app, name), stats)| ValueMetrics {
                app: app.to_string(),
                name: name.to_string(),
                reads: stats.reads.since(from),
                changes: stats.changes.since(from),
                last_change_ns: stats.last_change_ns,
                check: spread(&stats.checks).0,
            })
            .collect();
        let automations = self
            .automations
            .iter()
            .map(|((app, name), stats)| AutomationMetrics {
                app: app.to_string(),
                name: name.to_string(),
                ran: stats.ran.since(from),
                held: stats.held.since(from),
                failed: stats.failed.since(from),
                frame_to_action: spread(&stats.frame_to_action).0,
            })
            .collect();
        let start = self.first_ns.map_or(self.newest_ns, |first| {
            from.map_or(first, |from| from.max(first))
        });
        TraceMetrics {
            progress: self.progress(),
            recorded: self.recorded.iter().copied().collect(),
            span_ns: self.newest_ns.saturating_sub(start),
            chart: chart(&frame_times, frame_to_action),
            frame_to_action,
            rule_handling,
            action_run,
            reads: self.reads.since(from),
            changes: self.changes.since(from),
            actions: self.actions.since(from),
            failed: self.failed.since(from),
            dropped: self.dropped.since(from),
            values,
            automations,
            sampled,
        }
    }

    /// The held rows `filter` picks.
    pub(crate) fn rows(&self, filter: &RowFilter) -> TraceRows {
        let from = self.from(filter.range);
        let needle = filter.search.trim().to_lowercase();
        // Whether each name holds the text, worked out once per name.
        let found: Vec<bool> = if needle.is_empty() {
            Vec::new()
        } else {
            self.names
                .entries
                .iter()
                .map(|(name, _)| name.to_lowercase().contains(&needle))
                .collect()
        };
        let app = filter
            .app
            .as_deref()
            .map(|app| self.names.places.get(app).copied());
        let picks = |row: &&Row| {
            from.is_none_or(|from| row.time_ns >= from)
                && filter.categories.contains(&row.category)
                && app.is_none_or(|app| app == Some(row.texts[2]))
                && filter
                    .took_over_ns
                    .is_none_or(|over| row.took_ns.is_some_and(|took| took > over))
                && (found.is_empty()
                    || row
                        .texts
                        .iter()
                        .any(|text| found.get(*text as usize).copied().unwrap_or(false)))
        };
        let mut matching = 0_u64;
        let mut rows = Vec::new();
        for row in self.rows.iter().rev().filter(picks) {
            matching = matching.saturating_add(1);
            if rows.len() < SHOWN {
                rows.push(self.row(row));
            }
        }
        rows.reverse();
        let apps: BTreeSet<u32> = self.rows.iter().map(|row| row.texts[2]).collect();
        let mut apps: Vec<String> = apps
            .into_iter()
            .map(|app| self.names.get(app).to_string())
            .filter(|app| !app.is_empty())
            .collect();
        apps.sort_unstable();
        TraceRows {
            progress: self.progress(),
            rows,
            matching,
            held: u64::try_from(self.rows.len()).unwrap_or(u64::MAX),
            total: self.total,
            apps,
        }
    }

    /// `row` as the page reads it.
    fn row(&self, row: &Row) -> TraceRow {
        let [thread, event, app, item, value, detail] =
            row.texts.map(|text| self.names.get(text).to_string());
        TraceRow {
            time_ns: row.time_ns,
            thread,
            event,
            app,
            item,
            value,
            took_ns: row.took_ns,
            cause: row.cause,
            detail,
        }
    }
}

/// The category a row of `event` belongs to, as the trace's settings name
/// them. The trace's own rows count as lifecycle rows.
fn category(event: &str) -> TraceCategory {
    match event {
        "read" => TraceCategory::Readings,
        "changed" | "received" => TraceCategory::Changes,
        "handled" | "fired" | "held" | "released" => TraceCategory::Rules,
        "queued" | "started" | "done" => TraceCategory::Actions,
        _ => TraceCategory::Lifecycle,
    }
}

/// How `times`, sorted, spread over about [`BARS`] bars of a round width.
/// When the longest is more than 4 times the 95th percentile, a few far
/// slower times would leave the rest too thin to read, so the bars stop
/// just past twice the 95th percentile and the times after the last bar are
/// counted apart.
fn chart(times: &[u64], spread: Option<Spread>) -> Option<Chart> {
    let (spread, longest) = (spread?, *times.last()?);
    let top = if longest > spread.p95_ns.saturating_mul(4) {
        spread.p95_ns.saturating_mul(2)
    } else {
        longest
    };
    let bar = round_up(top.div_ceil(BARS).max(1));
    let bars = top.checked_div(bar).unwrap_or(0).saturating_add(1);
    let mut counts = vec![0_u64; usize::try_from(bars).unwrap_or(0)];
    let mut over = 0_u64;
    for time in times {
        let at = time
            .checked_div(bar)
            .and_then(|at| usize::try_from(at).ok());
        match at.and_then(|at| counts.get_mut(at)) {
            Some(count) => *count = count.saturating_add(1),
            None => over = over.saturating_add(1),
        }
    }
    Some(Chart {
        bar_ns: bar,
        counts,
        over,
        samples: u64::try_from(times.len()).unwrap_or(u64::MAX),
    })
}

/// The smallest of 1, 2 and 5 times a power of ten that is at least
/// `width`.
fn round_up(width: u64) -> u64 {
    let mut power = 1_u64;
    loop {
        for step in [1, 2, 5] {
            let round = power.saturating_mul(step);
            if round >= width || round == u64::MAX {
                return round;
            }
        }
        power = power.saturating_mul(10);
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{self, OpenOptions};
    use std::io::Write as _;
    use std::{env, process};

    use crate::debug::trace_file::{HEADER, WINDOW};

    use super::*;

    const MS: u64 = 1_000_000;

    /// A fresh trace file named `name`, holding `rows` under the header.
    fn trace(name: &str, rows: &[String]) -> PathBuf {
        let folder = env::temp_dir().join(format!("appytappy-view-{name}-{}", process::id()));
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a folder left by an earlier run may or may not exist"
        )]
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).expect("folder");
        let path = folder.join("trace-2026-11-12-100000.csv");
        fs::write(&path, format!("{HEADER}\n{}", lines(rows))).expect("write");
        path
    }

    fn lines(rows: &[String]) -> String {
        rows.iter().fold(String::new(), |mut text, row| {
            text.push_str(row);
            text.push('\n');
            text
        })
    }

    /// A row at `ms` milliseconds since the trace started.
    fn row(
        ms: u64,
        event: &str,
        item: &str,
        value: &str,
        took: &str,
        cause: &str,
        detail: &str,
    ) -> String {
        format!(
            "{},reader,{event},Example App,{item},{value},{took},{cause},{detail}",
            ms.saturating_mul(MS)
        )
    }

    /// A value read at `ms` whose check took `took_ms`.
    fn read(ms: u64, item: &str, took_ms: u64) -> String {
        row(
            ms,
            "read",
            item,
            "3",
            &took_ms.saturating_mul(MS).to_string(),
            "",
            "",
        )
    }

    /// An automation's actions that started at `ms` for a frame from
    /// `from_ms`.
    fn started(ms: u64, item: &str, from_ms: u64) -> String {
        row(
            ms,
            "started",
            item,
            "",
            "",
            &from_ms.saturating_mul(MS).to_string(),
            "",
        )
    }

    fn view(path: PathBuf) -> TraceView {
        let mut view = TraceView::new(path);
        view.update().expect("read");
        view
    }

    fn every_row(range: TraceRange) -> RowFilter {
        RowFilter {
            range,
            categories: vec![
                TraceCategory::Readings,
                TraceCategory::Changes,
                TraceCategory::Rules,
                TraceCategory::Actions,
                TraceCategory::Lifecycle,
            ],
            app: None,
            took_over_ns: None,
            search: String::new(),
        }
    }

    /// The items of `rows`, oldest first.
    fn items(rows: &TraceRows) -> Vec<&str> {
        rows.rows.iter().map(|row| row.item.as_str()).collect()
    }

    #[test]
    fn the_metrics_count_and_time_what_the_trace_holds() {
        let path = trace(
            "metrics",
            &[
                "0,main,start,,,,,,2026-11-12 10:00:00 0.1.0".to_owned(),
                read(10, "health", 2),
                read(20, "health", 4),
                row(20, "changed", "health", "2", "", &(15 * MS).to_string(), ""),
                row(21, "handled", "", "", &(MS / 2).to_string(), "", ""),
                row(22, "held", "heal", "", "", "", ""),
                started(30, "heal", 15),
                row(40, "done", "heal", "", &(5 * MS).to_string(), "", "ok"),
                started(50, "mute", 40),
                row(60, "done", "mute", "", &(7 * MS).to_string(), "", "failed"),
                row(70, "dropped", "", "4", "", "", ""),
            ],
        );
        let metrics = view(path).metrics(TraceRange::Whole);
        assert_eq!(metrics.progress.problem, None);
        assert_eq!(metrics.progress.read, metrics.progress.bytes);
        assert_eq!(metrics.progress.file, "trace-2026-11-12-100000.csv");
        assert_eq!(
            metrics.recorded,
            [
                TraceCategory::Readings,
                TraceCategory::Changes,
                TraceCategory::Rules,
                TraceCategory::Actions,
                TraceCategory::Lifecycle,
            ]
        );
        assert_eq!(metrics.span_ns, 70 * MS);
        assert_eq!(
            metrics.frame_to_action,
            Some(Spread {
                median_ns: 10 * MS,
                p95_ns: 15 * MS
            })
        );
        assert_eq!(
            metrics.rule_handling.map(|spread| spread.median_ns),
            Some(MS / 2)
        );
        assert_eq!(metrics.action_run.map(|spread| spread.p95_ns), Some(7 * MS));
        assert_eq!(
            (
                metrics.reads,
                metrics.changes,
                metrics.actions,
                metrics.failed,
                metrics.dropped
            ),
            (2, 1, 2, 1, 4)
        );
        assert!(!metrics.sampled);
        assert_eq!(
            metrics.values,
            [ValueMetrics {
                app: "Example App".to_owned(),
                name: "health".to_owned(),
                reads: 2,
                changes: 1,
                last_change_ns: Some(20 * MS),
                check: Some(Spread {
                    median_ns: 2 * MS,
                    p95_ns: 4 * MS
                }),
            }]
        );
        let ran: Vec<(&str, u64, u64, u64)> = metrics
            .automations
            .iter()
            .map(|each| (each.name.as_str(), each.ran, each.held, each.failed))
            .collect();
        assert_eq!(ran, [("heal", 1, 1, 0), ("mute", 1, 0, 1)]);
        assert_eq!(
            metrics.automations[0]
                .frame_to_action
                .map(|spread| spread.median_ns),
            Some(15 * MS)
        );
        let chart = metrics.chart.expect("a chart");
        assert_eq!((chart.bar_ns, chart.over, chart.samples), (MS, 0, 2));
        assert_eq!(chart.counts.len(), 16);
        assert_eq!((chart.counts[10], chart.counts[15]), (1, 1));
    }

    #[test]
    fn a_range_counts_only_its_last_minutes() {
        let path = trace(
            "range",
            &[
                read(0, "health", 1),
                started(1_000, "heal", 990),
                // Just where the last minute starts.
                "70000000000,main,snapshot,,,,,,".to_owned(),
                read(100_000, "health", 1),
                started(100_000, "heal", 99_995),
                read(130_000, "mana", 1),
            ],
        );
        let view = view(path);
        let minute = view.metrics(TraceRange::LastMinute);
        assert_eq!((minute.reads, minute.actions), (2, 1));
        assert_eq!(minute.span_ns, 60_000 * MS);
        assert_eq!(
            minute.frame_to_action.map(|spread| spread.p95_ns),
            Some(5 * MS)
        );
        let names: Vec<(&str, u64)> = minute
            .values
            .iter()
            .map(|each| (each.name.as_str(), each.reads))
            .collect();
        assert_eq!(names, [("health", 1), ("mana", 1)]);
        let whole = view.metrics(TraceRange::Whole);
        assert_eq!((whole.reads, whole.actions), (3, 2));
        assert_eq!(whole.span_ns, 130_000 * MS);
        assert_eq!(view.metrics(TraceRange::LastTenMinutes).reads, 3);
        assert_eq!(view.rows(&every_row(TraceRange::LastMinute)).matching, 4);
    }

    #[test]
    fn an_empty_trace_has_no_times_and_records_nothing() {
        let metrics = view(trace("empty", &[])).metrics(TraceRange::LastMinute);
        assert_eq!(metrics.recorded, []);
        assert_eq!((metrics.span_ns, metrics.reads), (0, 0));
        assert_eq!((metrics.frame_to_action, metrics.chart), (None, None));
    }

    #[test]
    fn the_rows_are_filtered_by_category_app_time_and_text() {
        let path = trace(
            "rows",
            &[
                read(1, "health", 5),
                read(2, "Mana", 9),
                "3,reader,read,Other App,ammo,1,1000000,,".to_owned(),
                started(4, "heal", 1),
                row(5, "done", "heal", "", &(20 * MS).to_string(), "", "ok"),
            ],
        );
        let view = view(path);
        let all = view.rows(&every_row(TraceRange::Whole));
        assert_eq!(items(&all), ["health", "Mana", "ammo", "heal", "heal"]);
        assert_eq!((all.matching, all.held, all.total), (5, 5, 5));
        assert_eq!(all.apps, ["Example App", "Other App"]);
        assert_eq!(all.rows[1].took_ns, Some(9 * MS));
        let actions = RowFilter {
            categories: vec![TraceCategory::Actions],
            ..every_row(TraceRange::Whole)
        };
        assert_eq!(items(&view.rows(&actions)), ["heal", "heal"]);
        let other = RowFilter {
            app: Some("Other App".to_owned()),
            ..every_row(TraceRange::Whole)
        };
        assert_eq!(items(&view.rows(&other)), ["ammo"]);
        let unknown = RowFilter {
            app: Some("Missing App".to_owned()),
            ..every_row(TraceRange::Whole)
        };
        assert_eq!(view.rows(&unknown).matching, 0);
        // A row that took just as long as the limit is not over it.
        let slow = RowFilter {
            took_over_ns: Some(5 * MS),
            ..every_row(TraceRange::Whole)
        };
        assert_eq!(items(&view.rows(&slow)), ["Mana", "heal"]);
        let found = RowFilter {
            search: " mAnA ".to_owned(),
            ..every_row(TraceRange::Whole)
        };
        assert_eq!(items(&view.rows(&found)), ["Mana"]);
        let by_detail = RowFilter {
            search: "OK".to_owned(),
            ..every_row(TraceRange::Whole)
        };
        assert_eq!(view.rows(&by_detail).matching, 1);
    }

    #[test]
    fn only_the_newest_rows_are_shown_oldest_first() {
        let rows: Vec<String> = (0..SHOWN as u64 + 3)
            .map(|ms| read(ms, &format!("v{ms}"), 1))
            .collect();
        let shown = view(trace("shown", &rows)).rows(&every_row(TraceRange::Whole));
        assert_eq!(shown.matching, SHOWN as u64 + 3);
        assert_eq!(shown.rows.len(), SHOWN);
        assert_eq!(shown.rows.first().map(|row| row.item.as_str()), Some("v3"));
        assert_eq!(
            shown.rows.last().map(|row| row.item.clone()),
            Some(format!("v{}", SHOWN + 2))
        );
    }

    #[test]
    fn a_trace_s_own_rows_count_as_lifecycle_rows() {
        for event in [
            "start", "part", "dropped", "snapshot", "reading", "stopped", "reloaded",
        ] {
            assert_eq!(category(event), TraceCategory::Lifecycle, "{event}");
        }
        assert_eq!(category("read"), TraceCategory::Readings);
        for event in ["changed", "received"] {
            assert_eq!(category(event), TraceCategory::Changes, "{event}");
        }
        for event in ["handled", "fired", "held", "released"] {
            assert_eq!(category(event), TraceCategory::Rules, "{event}");
        }
        for event in ["queued", "started", "done"] {
            assert_eq!(category(event), TraceCategory::Actions, "{event}");
        }
    }

    #[test]
    fn a_large_trace_is_read_a_few_windows_at_a_time() {
        // Two values, each read more often than the times kept of a kind.
        let row = format!("{}\n{}\n", read(1, "health", 1), read(1, "mana", 1));
        let count = usize::try_from(WINDOW * 5 / 2).expect("fits") / row.len() + 1;
        let path = trace("large", &[]);
        let mut file = OpenOptions::new().append(true).open(&path).expect("open");
        file.write_all(row.repeat(count).as_bytes()).expect("write");
        drop(file);
        let mut view = TraceView::new(path);
        view.update().expect("read");
        let first = view.metrics(TraceRange::Whole);
        assert!(first.progress.read < first.progress.bytes);
        assert!(first.progress.read >= WINDOW * 2 - row.len() as u64);
        view.update().expect("read");
        let caught_up = view.metrics(TraceRange::Whole);
        assert_eq!(caught_up.progress.read, caught_up.progress.bytes);
        assert_eq!(caught_up.reads, 2 * count as u64);
        assert!(
            caught_up
                .values
                .iter()
                .all(|value| value.reads > SAMPLES as u64)
        );
        assert!(caught_up.sampled);
        assert_eq!(view.rows(&every_row(TraceRange::Whole)).held, ROWS as u64);
    }

    #[test]
    fn a_replaced_trace_is_read_again_from_its_start() {
        let path = trace("replaced", &[read(1, "health", 1), read(2, "health", 1)]);
        let mut view = view(path.clone());
        fs::write(&path, format!("{HEADER}\n{}", lines(&[read(5, "mana", 1)]))).expect("write");
        view.update().expect("read");
        let metrics = view.metrics(TraceRange::Whole);
        assert_eq!(metrics.reads, 1);
        assert_eq!(metrics.values[0].name, "mana");
        assert_eq!(view.path(), path);
    }

    #[test]
    fn a_trace_from_another_version_says_so() {
        let path = trace("other", &[]);
        fs::write(&path, "time,something\n1,2\n").expect("write");
        let progress = view(path).metrics(TraceRange::Whole).progress;
        assert!(
            progress
                .problem
                .is_some_and(|problem| problem.contains("another version"))
        );
    }

    #[test]
    fn rows_out_of_order_are_counted_in_their_own_second() {
        let mut tally = Tally::default();
        for (ms, count) in [(5_000, 1), (3_000, 2), (5_500, 4), (1_000, 8), (4_000, 16)] {
            tally.add(ms * MS, count);
        }
        assert_eq!(
            Vec::from(tally.seconds.clone()),
            [(1, 8), (3, 2), (4, 16), (5, 5)]
        );
        assert_eq!(tally.total, 31);
        assert_eq!(tally.since(Some(3_900 * MS)), 23);
        assert_eq!(tally.since(None), 31);
        tally.trim((RECENT_S + 4) * NS_PER_S);
        assert_eq!(Vec::from(tally.seconds.clone()), [(4, 16), (5, 5)]);
        assert_eq!(tally.total, 31);
    }

    #[test]
    fn only_the_newest_times_are_kept_and_a_range_says_when_it_lost_some() {
        let mut samples = Samples::default();
        for time in 0..SAMPLES as u64 {
            samples.add(time, SAMPLES as u64 - time);
        }
        assert!(!samples.since(None).1);
        samples.add(SAMPLES as u64, 0);
        let (times, short) = samples.since(None);
        assert!(short);
        assert_eq!(times.len(), SAMPLES);
        assert_eq!(times.first(), Some(&0));
        // A range that starts after the oldest kept time lost nothing.
        let (recent, short) = samples.since(Some(5));
        assert!(!short);
        assert_eq!(recent.len(), SAMPLES - 4);
        assert!(!samples.since(Some(1)).1);
        assert!(samples.since(Some(0)).1);
    }

    #[test]
    fn names_of_dropped_rows_are_let_go() {
        let mut view = TraceView::new(PathBuf::from("trace-test.csv"));
        let each = |index: usize| TraceRow {
            time_ns: index as u64,
            thread: "reader".to_owned(),
            event: "read".to_owned(),
            app: "Example App".to_owned(),
            item: format!("value {index}"),
            value: "3".to_owned(),
            took_ns: None,
            cause: None,
            detail: String::new(),
        };
        let count = ROWS + ROWS * 3 / 2;
        for index in 0..count {
            view.add(&each(index));
        }
        assert_eq!(view.rows.len(), ROWS);
        // Each row names its own value, so without letting names go there would be
        // a name for every row read.
        // The held rows' own values, and the five texts every row shares.
        assert_eq!(view.names.places.len(), ROWS + 5);
        assert!(
            view.names.entries.len() <= ROWS + 6,
            "{}",
            view.names.entries.len()
        );
        let newest = view.rows(&RowFilter {
            search: format!("value {}", count - 1),
            ..every_row(TraceRange::Whole)
        });
        assert_eq!(newest.matching, 1);
        assert_eq!(newest.rows[0].time_ns, (count - 1) as u64);
        assert_eq!(
            (newest.rows[0].app.as_str(), newest.rows[0].value.as_str()),
            ("Example App", "3")
        );
    }

    #[test]
    fn a_chart_leaves_out_a_few_far_slower_times() {
        let mut times: Vec<u64> = (1..=100_u64).map(|ms| ms.saturating_mul(MS)).collect();
        times.push(10_000 * MS);
        let drawn = chart(&times, Spread::of_sorted(&times)).expect("a chart");
        // The 95th percentile is 96 ms, so the bars stop at 192 ms.
        assert_eq!((drawn.bar_ns, drawn.counts.len()), (10 * MS, 20));
        assert_eq!((drawn.over, drawn.samples), (1, 101));
        assert_eq!(drawn.counts.iter().sum::<u64>(), 100);
        assert_eq!(chart(&[], None), None);
    }

    #[test]
    fn a_chart_keeps_a_longest_time_just_4_times_the_95th_percentile() {
        let mut times = vec![10 * MS; 99];
        times.push(40 * MS);
        let spread = Spread::of_sorted(&times);
        assert_eq!(spread.map(|spread| spread.p95_ns), Some(10 * MS));
        let drawn = chart(&times, spread).expect("a chart");
        assert_eq!(
            (drawn.bar_ns, drawn.counts.len(), drawn.over),
            (2 * MS, 21, 0)
        );
        assert_eq!(drawn.counts.last(), Some(&1));
    }

    #[test]
    fn counts_by_the_second_are_kept_only_as_long_as_a_range_reads_them() {
        let late = (RECENT_S + 100) * 1_000;
        let view = view(trace(
            "trim",
            &[
                read(0, "health", 1),
                started(0, "heal", 0),
                read(late, "health", 1),
                started(late, "heal", late),
            ],
        ));
        assert_eq!(view.reads.seconds.len(), 1);
        assert_eq!(view.actions.seconds.len(), 1);
        assert!(
            view.values
                .values()
                .all(|value| value.reads.seconds.len() == 1)
        );
        assert!(
            view.automations
                .values()
                .all(|automation| automation.ran.seconds.len() == 1)
        );
        assert_eq!(
            view.metrics(TraceRange::Whole).reads,
            2,
            "the whole count stays"
        );
    }

    #[test]
    fn bars_are_a_round_width() {
        for (width, round) in [
            (0, 1),
            (1, 1),
            (2, 2),
            (3, 5),
            (6, 10),
            (11, 20),
            (21, 50),
            (499, 500),
        ] {
            assert_eq!(round_up(width), round, "{width}");
        }
        assert_eq!(round_up(u64::MAX), u64::MAX);
    }
}
