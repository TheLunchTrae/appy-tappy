//! A trace record as a CSV row.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use appytappy_config::signal::Value;
use appytappy_platform::LocalTime;

use super::{Detail, Record, What};
use crate::model::SessionId;
use crate::plan::AppPlan;

/// The row that starts a trace: the local time and the runtime's version.
pub(super) fn start_row(local: Option<LocalTime>) -> String {
    let time = local.map_or_else(|| "unknown time".to_owned(), |time| time.to_string());
    format!(
        "0,trace,start,,,,,,{}",
        field(&format!("{time} {}", env!("CARGO_PKG_VERSION")))
    )
}

/// The row that starts the file numbered `part`, `since` the trace started.
pub(super) fn part_row(since: Duration, part: u32) -> String {
    format!("{},trace,part,,,{part},,,", nanos(since))
}

/// The row that says how many records were dropped before `record`.
pub(super) fn dropped_row(record: &Record, started: Instant) -> String {
    format!(
        "{},trace,dropped,,,{},,,",
        nanos(record.at.saturating_duration_since(started)),
        record.dropped
    )
}

/// `record` as a CSV row, with times in nanoseconds since the trace
/// started.
pub(super) fn row(
    record: &Record,
    started: Instant,
    names: &BTreeMap<SessionId, Arc<AppPlan>>,
) -> String {
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
    // A frame from before the trace started, as one whose value a reader
    // sent just before it got the new trace, has no cause in this trace.
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
            plan.runs
                .rules
                .get(item)
                .map(|rule| rule.automation.as_str())
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
    use appytappy_config::signal::Number;

    use super::*;

    #[test]
    fn a_part_row_says_when_the_part_starts_and_its_number() {
        assert_eq!(part_row(Duration::from_nanos(7), 3), "7,trace,part,,,3,,,");
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
}
