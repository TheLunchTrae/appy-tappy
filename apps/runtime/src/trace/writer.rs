//! The trace writer thread: owns the trace files and turns records into CSV
//! rows.
//!
//! It runs at the lowest thread priority, so it takes time only that nothing
//! else wants. It drains the buffer that every [`super::Trace`] sends to,
//! writes through a buffer that it flushes about once a second, and is the
//! only thread that touches the trace files. A file that reaches the size
//! limit ends and the trace continues in a new one, and only the newest
//! files are kept. When a file cannot be written, it tells the owner and
//! stops, and every trace handle turns itself off at its next record.
//!
//! The owner hands it names and tells it to stop over a second channel that
//! has no limit, so the owner never waits for the writer.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender, TryRecvError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use appytappy_config::signal::Value;
use appytappy_platform::LocalTime;

use super::{Categories, Detail, Record, Trace, Tracer, What};
use crate::plan::AppPlan;
use crate::workers::reader::SessionId;

/// How many records the buffer holds. It is allocated once, when the trace
/// starts, at about 150 bytes a record, so it is the trace's largest cost.
/// The footprint run's three values make about 160 rows a second with every
/// category on, so this holds close to a minute of them while the writer
/// waits for time; more values, or a faster read interval, fill it sooner,
/// and the file counts what is dropped.
pub(crate) const BUFFER: usize = 8_192;

/// How often the writer flushes what it wrote.
const FLUSH_EVERY: Duration = Duration::from_secs(1);

/// The header row of every trace file.
pub(crate) const HEADER: &str = "time_ns,thread,event,app,item,value,took_ns,cause,detail";

/// What reaches the writer thread through the buffer.
#[derive(Debug)]
pub(crate) enum Entry {
    /// A row to write.
    Record(Record),
    /// Look at the orders: the owner sent one.
    Wake,
}

/// What the owner tells the writer thread, over a channel with no limit.
#[derive(Debug)]
enum Order {
    /// The plan of a session, which names its application, values, and
    /// automations in the rows that follow.
    Names(SessionId, Arc<AppPlan>),
    /// Write what the buffer holds and stop.
    Stop,
}

/// What the writer tells the owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TraceReport {
    /// Trace `id` stopped, because a file cannot be written; the message
    /// says why, for the log.
    Failed {
        /// The trace's number.
        id: u64,
        /// Why, as a line for the log.
        message: String,
    },
}

/// How a trace writes its files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Options {
    /// The folder the files go in.
    pub(crate) folder: PathBuf,
    /// What to record.
    pub(crate) categories: Categories,
    /// The size at which a file ends and the trace continues in a new one,
    /// in bytes.
    pub(crate) file_limit: u64,
    /// How many trace files to keep in the folder.
    pub(crate) keep_files: usize,
}

/// A running trace writer.
#[derive(Debug)]
pub(crate) struct Writer {
    entries: SyncSender<Entry>,
    orders: Sender<Order>,
    thread: JoinHandle<()>,
    /// The first file's name, for the log.
    file: String,
}

/// A writer told to stop, which may still be writing what its buffer held.
#[derive(Debug)]
pub(crate) struct Stopping(JoinHandle<()>);

impl Stopping {
    /// Whether the writer is done, so [`Self::join`] does not wait.
    pub(crate) fn is_finished(&self) -> bool {
        self.0.is_finished()
    }

    /// Waits for the writer to finish.
    ///
    /// # Errors
    ///
    /// Returns an error when the writer thread panicked.
    pub(crate) fn join(self) -> io::Result<()> {
        crate::workers::join(self.0)
    }
}

#[cfg(test)]
impl Stopping {
    /// A stopping writer whose thread is `thread`.
    pub(super) const fn of(thread: JoinHandle<()>) -> Self {
        Self(thread)
    }
}

impl Writer {
    /// Opens the trace's first file and starts the writer thread, which
    /// reports to `reports`. Returns the writer and the trace handle the
    /// other threads record with. `id` tells this trace from the one before.
    ///
    /// # Errors
    ///
    /// Returns the error from making the folder, opening the file, or
    /// starting the thread.
    pub(crate) fn start<E>(
        id: u64,
        options: Options,
        reports: Sender<E>,
    ) -> io::Result<(Self, Trace)>
    where
        E: From<TraceReport> + Send + 'static,
    {
        let started = Instant::now();
        let mut files = Files::open(
            &options,
            &stem(LocalTime::now(), SystemTime::now()),
            started,
        )?;
        let file = files.name();
        files.row(&start_row(LocalTime::now()))?;
        let (entries, received) = mpsc::sync_channel(BUFFER);
        let (orders, ordered) = mpsc::channel();
        let categories = options.categories;
        let thread = crate::workers::spawn("trace", move || {
            // A thread that keeps its priority still writes the trace.
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the trace is written either way, only sooner"
            )]
            let _ = appytappy_platform::lower_thread_priority();
            // Old files go here rather than on the thread that starts the
            // trace, which is the owner.
            files.prune();
            if let Err(error) = write(&received, &ordered, &mut files) {
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "a failed send means the owner already stopped"
                )]
                let _ = reports.send(E::from(TraceReport::Failed {
                    id,
                    message: format!("the trace cannot be written, so it stops: {error}"),
                }));
            }
        })?;
        let trace = Trace(Some(Tracer {
            id,
            categories,
            entries: entries.clone(),
            dropped: 0,
        }));
        Ok((
            Self {
                entries,
                orders,
                thread,
                file,
            },
            trace,
        ))
    }

    /// The first file's name.
    pub(crate) fn file(&self) -> &str {
        &self.file
    }

    /// Hands the writer `plan`, which names `session`'s rows. It never
    /// waits: the writer takes the names before the next record it writes,
    /// so every row the session records after this is named.
    pub(crate) fn name(&self, session: SessionId, plan: Arc<AppPlan>) {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a writer that stopped has reported why, and needs no names"
        )]
        let _ = self.orders.send(Order::Names(session, plan));
    }

    /// Tells the writer to write what its buffer holds, close the file, and
    /// stop. It never waits for the writer.
    pub(crate) fn retire(self) -> Stopping {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a writer that stopped has reported why, and its join says how it ended"
        )]
        let _ = self.orders.send(Order::Stop);
        // A full buffer means the writer is busy and looks at its orders
        // soon anyway.
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a writer that is busy or gone needs no wake"
        )]
        let _ = self.entries.try_send(Entry::Wake);
        Stopping(self.thread)
    }

    /// Writes what is left, closes the file, and waits for the thread,
    /// failing the test when it does not stop within a few seconds.
    ///
    /// # Errors
    ///
    /// Returns an error when the writer thread panicked.
    #[cfg(test)]
    pub(crate) fn stop(self) -> io::Result<()> {
        super::within(move || self.retire().join())
    }
}

/// The writer loop: writes each record until it is told to stop or every
/// handle is gone, flushing about once a second. Told to stop, it writes at
/// most a buffer's worth more, which is what the buffer held, and ends.
fn write(
    received: &Receiver<Entry>,
    orders: &Receiver<Order>,
    files: &mut Files,
) -> io::Result<()> {
    let mut names: BTreeMap<SessionId, Arc<AppPlan>> = BTreeMap::new();
    let mut flushed = Instant::now();
    loop {
        let entry = received.recv_timeout(FLUSH_EVERY);
        // The names a record needs were sent before it, so they are here.
        let going = take_orders(orders, &mut names);
        match entry {
            Ok(Entry::Record(record)) => files.record(&record, &names)?,
            Ok(Entry::Wake) | Err(RecvTimeoutError::Timeout) => {},
            Err(RecvTimeoutError::Disconnected) => return files.flush(),
        }
        if !going {
            break;
        }
        if flush_due(flushed, Instant::now()) {
            files.flush()?;
            flushed = Instant::now();
        }
    }
    for entry in received.try_iter().take(BUFFER) {
        if let Entry::Record(record) = entry {
            take_orders(orders, &mut names);
            files.record(&record, &names)?;
        }
    }
    files.flush()
}

/// Whether it is time at `now` to flush again, after a flush at `flushed`.
fn flush_due(flushed: Instant, now: Instant) -> bool {
    now.saturating_duration_since(flushed) >= FLUSH_EVERY
}

/// Takes the orders that wait into `names`. Returns false when the writer
/// should stop: it was told to, or the owner is gone.
fn take_orders(orders: &Receiver<Order>, names: &mut BTreeMap<SessionId, Arc<AppPlan>>) -> bool {
    loop {
        match orders.try_recv() {
            Ok(Order::Names(session, plan)) => {
                names.insert(session, plan);
            },
            Ok(Order::Stop) | Err(TryRecvError::Disconnected) => return false,
            Err(TryRecvError::Empty) => return true,
        }
    }
}

/// The open trace file, and the parts before it.
#[derive(Debug)]
struct Files {
    folder: PathBuf,
    /// The first file's name without `.csv`; later parts add `-partN`.
    stem: String,
    /// Which part is open, from 1.
    part: u32,
    file: BufWriter<File>,
    /// Bytes written to the open part.
    written: u64,
    limit: u64,
    keep: usize,
    /// When the trace started, which row times count from.
    started: Instant,
}

impl Files {
    /// Makes `options.folder` and opens the trace's first file, named from
    /// `stem`, or from `stem` and a number when a file of that name exists.
    fn open(options: &Options, stem: &str, started: Instant) -> io::Result<Self> {
        fs::create_dir_all(&options.folder)?;
        let mut taken = 1_u32;
        let (stem, file) = loop {
            let candidate = if taken == 1 {
                stem.to_owned()
            } else {
                format!("{stem}-{taken}")
            };
            match create(&options.folder.join(format!("{candidate}.csv"))) {
                Ok(file) => break (candidate, file),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists && taken < 100 => {
                    taken = taken.saturating_add(1);
                },
                Err(error) => return Err(error),
            }
        };
        let mut files = Self {
            folder: options.folder.clone(),
            stem,
            part: 1,
            file: BufWriter::new(file),
            written: 0,
            limit: options.file_limit,
            keep: options.keep_files,
            started,
        };
        files.header()?;
        Ok(files)
    }

    /// The open file's name.
    fn name(&self) -> String {
        if self.part == 1 {
            format!("{}.csv", self.stem)
        } else {
            format!("{}-part{}.csv", self.stem, self.part)
        }
    }

    /// Writes `line` and a line break, first moving on to a new part when
    /// the open one is full.
    fn row(&mut self, line: &str) -> io::Result<()> {
        if self.written >= self.limit {
            self.next_part()?;
        }
        self.line(line)
    }

    fn line(&mut self, line: &str) -> io::Result<()> {
        self.file.write_all(line.as_bytes())?;
        self.file.write_all(b"\n")?;
        let length = u64::try_from(line.len()).unwrap_or(u64::MAX);
        self.written = self.written.saturating_add(length).saturating_add(1);
        Ok(())
    }

    /// Writes `record`'s row, after a row that counts the records dropped
    /// before it, if any.
    fn record(
        &mut self,
        record: &Record,
        names: &BTreeMap<SessionId, Arc<AppPlan>>,
    ) -> io::Result<()> {
        if record.dropped > 0 {
            self.row(&dropped_row(record, self.started))?;
        }
        self.row(&row(record, self.started, names))
    }

    fn header(&mut self) -> io::Result<()> {
        self.line(HEADER)
    }

    /// Closes the open part, opens the next, and deletes the oldest files
    /// past the number to keep.
    fn next_part(&mut self) -> io::Result<()> {
        self.file.flush()?;
        self.part = self.part.saturating_add(1);
        self.file = BufWriter::new(create(&self.folder.join(self.name()))?);
        self.written = 0;
        self.header()?;
        let at = nanos(self.started.elapsed());
        self.line(&format!("{at},trace,part,,,{},,,", self.part))?;
        self.prune();
        Ok(())
    }

    /// Deletes the oldest trace files past the number to keep.
    fn prune(&self) {
        prune(&self.folder, self.keep);
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

/// Creates a file that does not exist yet.
fn create(path: &Path) -> io::Result<File> {
    OpenOptions::new().write(true).create_new(true).open(path)
}

/// Deletes the oldest trace files in `folder` past the newest `keep`. It
/// does what it can: a file it cannot delete, as one another program has
/// open, or cannot read the time of, stays, and the next part or trace
/// tries again.
fn prune(folder: &Path, keep: usize) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    let mut traces: Vec<_> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let csv = Path::new(&name)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("csv"));
            let modified = entry.metadata().and_then(|meta| meta.modified()).ok()?;
            (name.starts_with("trace-") && csv).then(|| (modified, name, entry.path()))
        })
        .collect();
    traces.sort();
    let extra = traces.len().saturating_sub(keep);
    for (_, _, path) in traces.into_iter().take(extra) {
        // A file that stays is deleted at a later try.
        drop(fs::remove_file(path));
    }
}

/// The first file's name without `.csv`, from the local time the trace
/// starts, such as `trace-2026-11-08-143000`, or from the Unix time where
/// the local time is unknown.
fn stem(local: Option<LocalTime>, now: SystemTime) -> String {
    match local {
        Some(time) => format!(
            "trace-{:04}-{:02}-{:02}-{:02}{:02}{:02}",
            time.year, time.month, time.day, time.hour, time.minute, time.second
        ),
        None => format!(
            "trace-{}",
            now.duration_since(UNIX_EPOCH)
                .map_or(0, |since| since.as_secs())
        ),
    }
}

/// The row that starts a trace: the local time and the runtime's version.
fn start_row(local: Option<LocalTime>) -> String {
    let time = local.map_or_else(|| "unknown time".to_owned(), |time| time.to_string());
    format!(
        "0,trace,start,,,,,,{}",
        field(&format!("{time} {}", env!("CARGO_PKG_VERSION")))
    )
}

/// The row that says how many records were dropped before `record`.
fn dropped_row(record: &Record, started: Instant) -> String {
    format!(
        "{},trace,dropped,,,{},,,",
        nanos(record.at.saturating_duration_since(started)),
        record.dropped
    )
}

/// `record` as a CSV row, with times in nanoseconds since the trace
/// started.
fn row(record: &Record, started: Instant, names: &BTreeMap<SessionId, Arc<AppPlan>>) -> String {
    let plan = record.session.and_then(|session| names.get(&session));
    let app = plan.map_or("", |plan| plan.labels.name.as_str());
    let item = plan
        .zip(record.item)
        .and_then(|(plan, item)| item_name(plan, record.what, item))
        .unwrap_or("");
    let value = record.value.as_ref().map(value_text).unwrap_or_default();
    // A time the record leaves out is an empty field, not 0.
    let took = record
        .took
        .map(|took| nanos(took).to_string())
        .unwrap_or_default();
    // A frame from before the trace started, as a late sample from a reader
    // the new trace restarted, has no cause in this trace.
    let cause = record
        .cause
        .and_then(|cause| cause.checked_duration_since(started))
        .map(|cause| nanos(cause).to_string())
        .unwrap_or_default();
    let detail = match record.detail {
        Detail::None => String::new(),
        Detail::Note(note) => note.to_owned(),
        Detail::Count(count) => count.to_string(),
    };
    format!(
        "{},{},{},{},{},{},{},{},{}",
        nanos(record.at.saturating_duration_since(started)),
        thread(record.what),
        event(record.what),
        field(app),
        field(item),
        field(&value),
        took,
        cause,
        field(&detail),
    )
}

/// The name of the value or automation at `item` in `plan`.
fn item_name(plan: &AppPlan, what: What, item: usize) -> Option<&str> {
    match what {
        What::Read | What::Changed | What::Received => {
            plan.labels.values.get(item).map(String::as_str)
        },
        What::Fired | What::Held | What::Released | What::Queued | What::Started | What::Done => {
            plan.labels.automations.get(item).map(String::as_str)
        },
        What::Handled | What::Snapshot | What::Reading | What::Stopped | What::Reloaded => None,
    }
}

/// The thread a row comes from.
const fn thread(what: What) -> &'static str {
    match what {
        What::Read | What::Changed => "reader",
        What::Started | What::Done => "action",
        What::Snapshot => "lifecycle",
        What::Received
        | What::Handled
        | What::Fired
        | What::Held
        | What::Released
        | What::Queued
        | What::Reading
        | What::Stopped
        | What::Reloaded => "owner",
    }
}

/// The event column's word for `what`.
const fn event(what: What) -> &'static str {
    match what {
        What::Read => "read",
        What::Changed => "changed",
        What::Received => "received",
        What::Handled => "handled",
        What::Fired => "fired",
        What::Held => "held",
        What::Released => "released",
        What::Queued => "queued",
        What::Started => "started",
        What::Done => "done",
        What::Snapshot => "snapshot",
        What::Reading => "reading",
        What::Stopped => "stopped",
        What::Reloaded => "reloaded",
    }
}

fn value_text(value: &Value) -> String {
    match value {
        Value::Number(number) => number.raw().to_string(),
        Value::Flag(flag) => flag.to_string(),
        Value::State(state) => state.to_string(),
    }
}

/// Whole nanoseconds, which fit a `u64` for over 500 years.
fn nanos(time: Duration) -> u64 {
    u64::try_from(time.as_nanos()).unwrap_or(u64::MAX)
}

/// `text` as a CSV field: quoted, with quotes doubled, when it holds a
/// comma, a quote, or a line break.
fn field(text: &str) -> String {
    if text.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::process;

    use appytappy_config::settings_file::TraceCategory;
    use appytappy_config::signal::Number;

    use super::*;
    use crate::trace::Record;

    fn folder(name: &str) -> PathBuf {
        let folder = env::temp_dir().join(format!("appytappy-trace-{name}-{}", process::id()));
        if folder.exists() {
            fs::remove_dir_all(&folder).expect("remove leftovers");
        }
        folder
    }

    fn options(folder: &Path, file_limit: u64, keep_files: usize) -> Options {
        Options {
            folder: folder.to_owned(),
            categories: [TraceCategory::Rules, TraceCategory::Actions]
                .into_iter()
                .collect(),
            file_limit,
            keep_files,
        }
    }

    fn files(folder: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(folder)
            .expect("list")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_trace_writes_a_header_a_start_row_and_named_rows() {
        let folder = folder("rows");
        let (reports, _received) = mpsc::channel::<TraceReport>();
        let (writer, mut trace) =
            Writer::start(7, options(&folder, 1 << 20, 5), reports).expect("start");
        let plan = crate::plan::examples::plan("Skyward, \"Quest\"", 90.0);
        writer.name(SessionId(3), plan);
        let frame = Instant::now();
        trace.record(What::Fired, |record| Record {
            session: Some(SessionId(3)),
            item: Some(0),
            cause: Some(frame),
            ..record
        });
        trace.record(What::Read, |_record| panic!("readings are not recorded"));
        trace.record(What::Done, |record| Record {
            session: Some(SessionId(3)),
            item: Some(0),
            took: Some(Duration::from_micros(2)),
            detail: Detail::Note("ok"),
            ..record
        });
        let name = writer.file().to_owned();
        writer.stop().expect("stop");
        let text = fs::read_to_string(folder.join(&name)).expect("read");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], HEADER);
        assert!(lines[1].starts_with("0,trace,start,,,,,,"), "{}", lines[1]);
        assert!(
            lines[1].ends_with(env!("CARGO_PKG_VERSION")),
            "{}",
            lines[1]
        );
        let quoted = "\"Skyward, \"\"Quest\"\"\",\"Pause Skyward, \"\"Quest\"\"\"";
        let fired = lines[2].split_once(',').expect("a time").1;
        let fired_end = fired
            .strip_prefix(&format!("owner,fired,{quoted},,,"))
            .unwrap_or_else(|| panic!("the fired row's start: {fired}"));
        // The cause is when the frame arrived, a moment after the trace began.
        let cause = fired_end.trim_end_matches(',');
        assert!(cause.parse::<u64>().is_ok(), "{fired}");
        assert!(
            lines[3].contains(&format!(",action,done,{quoted},,2000,,ok")),
            "{}",
            lines[3]
        );
        assert_eq!(lines.len(), 4);
        fs::remove_dir_all(folder).expect("clean up");
    }

    #[test]
    fn a_full_file_continues_in_a_new_part_and_old_files_are_deleted() {
        let folder = folder("parts");
        let (reports, _received) = mpsc::channel::<TraceReport>();
        let (writer, mut trace) =
            Writer::start(1, options(&folder, 200, 2), reports).expect("start");
        for _ in 0..20 {
            trace.record(What::Queued, |record| record);
        }
        writer.stop().expect("stop");
        let names = files(&folder);
        assert_eq!(names.len(), 2, "{names:?}");
        let newest = names.iter().max_by_key(|name| {
            name.rsplit_once("-part")
                .and_then(|(_, part)| part.trim_end_matches(".csv").parse::<u32>().ok())
                .unwrap_or(1)
        });
        let text = fs::read_to_string(folder.join(newest.expect("a file"))).expect("read");
        let mut lines = text.lines();
        assert_eq!(lines.next(), Some(HEADER));
        let part = lines.next().expect("part row");
        assert!(part.contains(",trace,part,,,"), "{part}");
        fs::remove_dir_all(folder).expect("clean up");
    }

    #[test]
    fn a_file_that_cannot_be_deleted_stays_and_the_trace_goes_on() {
        let folder = folder("stuck");
        fs::create_dir_all(&folder).expect("make");
        // A folder named like a trace file cannot be deleted as a file, as a
        // file another program has open cannot.
        fs::create_dir(folder.join("trace-0-stuck.csv")).expect("make the stuck one");
        fs::write(folder.join("trace-1-old.csv"), HEADER).expect("write an old one");
        let (reports, received) = mpsc::channel::<TraceReport>();
        let (writer, mut trace) =
            Writer::start(1, options(&folder, 200, 1), reports).expect("start");
        for _ in 0..20 {
            trace.record(What::Queued, |record| record);
        }
        writer.stop().expect("stop");
        assert_eq!(received.try_iter().count(), 0, "the trace did not fail");
        let names = files(&folder);
        assert!(names.contains(&"trace-0-stuck.csv".to_owned()), "{names:?}");
        assert!(!names.contains(&"trace-1-old.csv".to_owned()), "{names:?}");
        assert!(names.iter().any(|name| name.contains("-part")), "{names:?}");
        fs::remove_dir_all(folder).expect("clean up");
    }

    #[test]
    fn a_writer_told_to_stop_writes_what_it_was_sent() {
        let folder = folder("retire");
        let (reports, _received) = mpsc::channel::<TraceReport>();
        let (writer, mut trace) =
            Writer::start(1, options(&folder, 1 << 20, 5), reports).expect("start");
        for _ in 0..100 {
            trace.record(What::Queued, |record| record);
        }
        let name = writer.file().to_owned();
        writer.stop().expect("stop");
        let text = fs::read_to_string(folder.join(name)).expect("read");
        assert_eq!(
            text.lines().count(),
            102,
            "the header, the start, and each record"
        );
        fs::remove_dir_all(folder).expect("clean up");
    }

    #[test]
    fn a_frame_from_before_the_trace_has_no_cause() {
        let record = Record::now(What::Fired);
        // Two clock reads can be equal on Windows, so the trace starts a
        // moment after the frame on purpose.
        let started = record.at + Duration::from_millis(1);
        let record = Record {
            cause: Some(record.at),
            ..record
        };
        let line = row(&record, started, &BTreeMap::new());
        assert!(line.ends_with(",owner,fired,,,,,,"), "{line}");
    }

    #[test]
    fn records_dropped_for_a_full_buffer_are_counted_in_the_file() {
        let record = Record {
            dropped: 12,
            ..Record::now(What::Fired)
        };
        let started = record.at;
        assert_eq!(dropped_row(&record, started), "0,trace,dropped,,,12,,,");
        let folder = folder("dropped");
        let mut files =
            Files::open(&options(&folder, 1 << 20, 5), "trace-x", started).expect("open");
        files.record(&record, &BTreeMap::new()).expect("write");
        files
            .record(&Record::now(What::Held), &BTreeMap::new())
            .expect("write");
        files.flush().expect("flush");
        let text = fs::read_to_string(folder.join("trace-x.csv")).expect("read");
        let events: Vec<&str> = text
            .lines()
            .skip(1)
            .filter_map(|line| line.split(',').nth(2))
            .collect();
        assert_eq!(events, ["dropped", "fired", "held"], "{text}");
        fs::remove_dir_all(folder).expect("clean up");
    }

    #[test]
    fn the_writer_flushes_while_it_runs() {
        let folder = folder("flush");
        let (reports, _received) = mpsc::channel::<TraceReport>();
        let (writer, mut trace) =
            Writer::start(1, options(&folder, 1 << 20, 5), reports).expect("start");
        trace.record(What::Queued, |record| record);
        let path = folder.join(writer.file());
        let on_disk = || fs::read_to_string(&path).expect("read");
        crate::trace::eventually("the row reached the file", || {
            on_disk().lines().count() >= 3
        });
        writer.stop().expect("stop");
        fs::remove_dir_all(folder).expect("clean up");
    }

    #[test]
    fn a_flush_is_due_once_a_second_has_passed() {
        let flushed = Instant::now();
        let at = |millis| {
            flushed
                .checked_add(Duration::from_millis(millis))
                .expect("in range")
        };
        assert!(!flush_due(flushed, at(999)), "not before a second");
        assert!(flush_due(flushed, at(1_000)), "right on time");
        assert!(flush_due(flushed, at(5_000)), "late");
    }

    #[test]
    fn a_trace_looks_no_further_than_a_hundred_names() {
        let folder = folder("hundred");
        fs::create_dir_all(&folder).expect("make");
        fs::write(folder.join("trace-x.csv"), "").expect("write");
        for taken in 2..=100 {
            fs::write(folder.join(format!("trace-x-{taken}.csv")), "").expect("write");
        }
        let options = options(&folder, 1 << 20, 5);
        let error = Files::open(&options, "trace-x", Instant::now()).expect_err("all taken");
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert!(!folder.join("trace-x-101.csv").exists());
        fs::remove_dir_all(folder).expect("clean up");
    }

    #[test]
    fn only_trace_files_are_deleted_to_keep_the_newest() {
        let folder = folder("prune");
        fs::create_dir_all(&folder).expect("make");
        for name in ["notes.csv", "trace-a.txt", "trace-b.csv"] {
            fs::write(folder.join(name), "").expect("write");
        }
        fs::write(folder.join("trace-c.csv"), "").expect("write");
        prune(&folder, 1);
        assert_eq!(files(&folder), ["notes.csv", "trace-a.txt", "trace-c.csv"]);
        fs::remove_dir_all(folder).expect("clean up");
    }

    #[test]
    fn a_second_trace_in_the_same_second_gets_its_own_file() {
        let folder = folder("same-second");
        let options = options(&folder, 1 << 20, 5);
        let first = Files::open(&options, "trace-x", Instant::now()).expect("first");
        let second = Files::open(&options, "trace-x", Instant::now()).expect("second");
        assert_eq!(
            (first.name(), second.name()),
            ("trace-x.csv".to_owned(), "trace-x-2.csv".to_owned())
        );
        fs::remove_dir_all(folder).expect("clean up");
    }

    #[test]
    fn files_are_named_by_the_local_start_time() {
        let time = LocalTime {
            year: 2026,
            month: 11,
            day: 8,
            hour: 14,
            minute: 3,
            second: 0,
        };
        assert_eq!(stem(Some(time), UNIX_EPOCH), "trace-2026-11-08-140300");
        let later = UNIX_EPOCH + Duration::from_secs(90);
        assert_eq!(stem(None, later), "trace-90");
    }

    #[test]
    fn values_and_fields_read_plainly() {
        assert_eq!(
            value_text(&Value::Number(Number::new(1250.0).expect("finite"))),
            "1250"
        );
        assert_eq!(
            value_text(&Value::Number(Number::new(0.5).expect("finite"))),
            "0.5"
        );
        assert_eq!(value_text(&Value::Flag(true)), "true");
        assert_eq!(field("plain"), "plain");
        assert_eq!(field("a,b"), "\"a,b\"");
        assert_eq!(field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(field("two\nlines"), "\"two\nlines\"");
    }

    #[test]
    fn a_trace_that_cannot_open_its_folder_does_not_start() {
        let folder = folder("unwritable");
        fs::create_dir_all(&folder).expect("make");
        let blocked = folder.join("blocked");
        fs::write(&blocked, b"a file, not a folder").expect("write");
        let (reports, _received) = mpsc::channel::<TraceReport>();
        let error =
            Writer::start(1, options(&blocked, 1 << 20, 5), reports).expect_err("not a folder");
        assert_ne!(error.kind(), io::ErrorKind::Other);
        fs::remove_dir_all(folder).expect("clean up");
    }
}
