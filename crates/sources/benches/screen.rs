//! Times the screen checks on the fixture screenshots, the read schedule, and,
//! on Windows when asked, the capture-to-decision latency of a live window.
//!
//! Run it with `cargo bench -p appytappy-sources --bench screen`, or
//! `mise run rust:bench`. Each line
//! gives the median and the 99th percentile of one operation's time, over
//! 101 samples that each repeat the operation for about a millisecond.
//!
//! The live capture runs only when `APPYTAPPY_BENCH_WINDOW` names part of a
//! visible window's title. `APPYTAPPY_BENCH_REGION` sets the region as
//! `x,y,width,height` (default `0,0,64,64`), and `APPYTAPPY_BENCH_MIN_MS` and
//! `APPYTAPPY_BENCH_MAX_MS` set the read interval (the settings' defaults).

use std::env;
use std::error::Error;
use std::hint::black_box;
use std::io::{self, Write};
use std::time::{Duration, Instant};

use appytappy_config::app_file::{PinDef, SizeChangesDef};
use appytappy_config::settings_file::ReadInterval;
use appytappy_sources::schedule::{Activity, ReadSchedule};
use appytappy_sources::screen::check::{BarFill, Check, ColorMatch, ImageMatch};
use appytappy_sources::screen::digits::DigitReader;
use appytappy_sources::screen::find::PictureSearch;
use appytappy_sources::screen::mask::Mask;
use appytappy_sources::screen::saved::{SavedCheck, SavedValue};
use appytappy_sources::screen::{Image, Placed, Region, Rgb, Size};

/// Samples per operation; odd, so the median is one sample.
const SAMPLES: usize = 101;

/// The time each sample aims to fill, so short operations repeat enough to
/// outlast the clock's resolution.
const SAMPLE_TIME: Duration = Duration::from_millis(1);

const WHITE: Rgb = Rgb {
    red: 255,
    green: 255,
    blue: 255,
};

type BenchResult = Result<(), Box<dyn Error>>;

fn main() -> BenchResult {
    // `cargo test --all-targets` and nextest also run this binary, without
    // `--bench`; they only need it to build, so it lists no tests and exits.
    if !env::args().any(|argument| argument == "--bench") {
        return Ok(());
    }
    let mut out = io::stdout().lock();
    let [first, second] = [
        include_bytes!("../tests/fixtures/progress_37.png").as_slice(),
        include_bytes!("../tests/fixtures/progress_90.png").as_slice(),
    ]
    .map(Image::decode_png);
    let (first, second) = (first?, second?);
    let missing = || io::Error::other("a fixture region lies outside its screenshot");

    let bar_region = Region::new(59, 19, 202, 12).ok_or_else(missing)?;
    let counter_region = Region::new(120, 38, 100, 28).ok_or_else(missing)?;
    let panel_region = Region::new(100, 110, 120, 50).ok_or_else(missing)?;
    let bar = first.crop(bar_region).ok_or_else(missing)?;
    let counter = first.crop(counter_region).ok_or_else(missing)?;
    let panel = second.crop(panel_region).ok_or_else(missing)?;

    let fill = BarFill::new(
        first.pixel(70, 25).ok_or_else(missing)?,
        first.pixel(250, 25).ok_or_else(missing)?,
    )
    .map_err(|_same| io::Error::other("the fixture bar's colors match"))?;
    let mut reader = DigitReader::new(WHITE, 60);
    reader.learn(
        &Image::decode_png(include_bytes!("../tests/fixtures/digits.png"))?,
        "0123456789.%",
    )?;
    let gold = ColorMatch {
        color: second.pixel(105, 115).ok_or_else(missing)?,
        tolerance: 40,
    };
    let picture = ImageMatch::new(first.crop(panel_region).ok_or_else(missing)?, None, 8)
        .map_err(io::Error::other)?;
    // Every other pixel kept: the most runs a mask can break the region into.
    let alternate = Mask::new(
        panel.size(),
        (0..panel.pixels().len())
            .map(|index| index % 2 == 0)
            .collect(),
    )
    .map_err(io::Error::other)?;
    let masked = ImageMatch::new(
        first.crop(panel_region).ok_or_else(missing)?,
        Some(&alternate),
        8,
    )
    .map_err(io::Error::other)?;

    report(&mut out, "crop the bar from a 320x180 frame", || {
        first.crop(bar_region)
    })?;
    // A capture of one value copies only its region, so the frame is the
    // value's pixels already.
    let alone = Size {
        width: 400,
        height: 100,
    };
    let frame = Image::new(alone, vec![0; 400 * 100 * 4])?;
    let placed = Placed::new(
        alone,
        [Ok(SavedValue {
            drawn: SavedCheck {
                window_size: alone,
                region: Region::new(0, 0, alone.width, alone.height).ok_or_else(missing)?,
                check: Check::Color(gold),
            },
            fixes: Vec::new(),
            pin: PinDef::TopLeft,
            size_changes: SizeChangesDef::Rarely,
        }
        .first())],
    );
    report(&mut out, "one value alone, 400x100", || {
        placed.cropped(0, &frame).map(|image| image.pixels().len())
    })?;
    report(&mut out, "bar fill, 202x12", || fill.percent(&bar))?;
    report(&mut out, "number, 100x28", || reader.read(&counter, false))?;
    report(&mut out, "color, 120x50", || gold.matches(&panel))?;
    report(&mut out, "image, 120x50", || picture.matches(&panel))?;
    report(&mut out, "image, 120x50, every other pixel", || {
        masked.matches(&panel)
    })?;
    // A click's default search area is its picture's box widened by 50
    // pixels each way, and the picture sits near the area's far corner, so
    // most places are tried before the match.
    let search_area = first
        .crop(Region::new(70, 40, 180, 120).ok_or_else(missing)?)
        .ok_or_else(missing)?;
    let button = PictureSearch::new(
        first
            .crop(Region::new(195, 135, 40, 20).ok_or_else(missing)?)
            .ok_or_else(missing)?,
        None,
        20,
    )
    .map_err(io::Error::other)?;
    report(&mut out, "find a 40x20 picture in 180x120", || {
        button.find(&search_area)
    })?;
    let mut schedule = ReadSchedule::new(ReadInterval::DEFAULT);
    report(&mut out, "read schedule step", || {
        schedule.after_read(Activity::Steady)
    })?;
    live::run(&mut out, fill)
}

/// Times `operation` and writes its median and 99th percentile.
fn report<T>(out: &mut impl Write, name: &str, mut operation: impl FnMut() -> T) -> BenchResult {
    let repeats = calibrate(&mut operation);
    let mut samples: Vec<Duration> = (0..SAMPLES)
        .map(|_| {
            let start = Instant::now();
            for _ in 0..repeats {
                black_box(operation());
            }
            start.elapsed().checked_div(repeats).unwrap_or_default()
        })
        .collect();
    samples.sort_unstable();
    writeln!(
        out,
        "{name:<36} median {:>10.2?}  p99 {:>10.2?}",
        percentile(&samples, 50),
        percentile(&samples, 99),
    )?;
    Ok(())
}

/// How many repeats of `operation` fill about [`SAMPLE_TIME`].
fn calibrate<T>(operation: &mut impl FnMut() -> T) -> u32 {
    let mut repeats: u32 = 1;
    loop {
        let start = Instant::now();
        for _ in 0..repeats {
            black_box(operation());
        }
        if start.elapsed() >= SAMPLE_TIME || repeats >= 1 << 24 {
            return repeats;
        }
        repeats = repeats.saturating_mul(2);
    }
}

/// The `percent`th percentile of sorted `samples`.
fn percentile(samples: &[Duration], percent: usize) -> Duration {
    let last = samples.len().saturating_sub(1);
    let index = last.saturating_mul(percent).checked_div(100).unwrap_or(0);
    samples.get(index).copied().unwrap_or_default()
}

#[cfg(windows)]
mod live {
    //! Capture-to-decision latency on a live window: the time from the
    //! system composing a frame until a check has read its region.

    use std::env;
    use std::sync::mpsc;

    use appytappy_sources::HostSystem;
    use appytappy_sources::screen::{Capture, CaptureSession, Crop, Layout, NextFrame};
    use appytappy_sources::system::System;

    use super::{
        Activity, BarFill, BenchResult, Duration, Instant, ReadInterval, ReadSchedule, Region,
        SAMPLES, Write, io, percentile,
    };

    pub(super) fn run(out: &mut impl Write, fill: BarFill) -> BenchResult {
        let Ok(title) = env::var("APPYTAPPY_BENCH_WINDOW") else {
            writeln!(
                out,
                "capture: set APPYTAPPY_BENCH_WINDOW to time a live window"
            )?;
            return Ok(());
        };
        let region = parse_region(env::var("APPYTAPPY_BENCH_REGION").ok().as_deref())?;
        let interval = ReadInterval::new(
            millis("APPYTAPPY_BENCH_MIN_MS", ReadInterval::DEFAULT.min())?,
            millis("APPYTAPPY_BENCH_MAX_MS", ReadInterval::DEFAULT.max())?,
        )
        .ok_or_else(|| io::Error::other("the minimum interval exceeds the maximum"))?;
        let system = HostSystem::new();
        let window = system
            .windows()?
            .into_iter()
            .find(|window| window.visible && window.title.contains(&title))
            .ok_or_else(|| io::Error::other(format!("no visible window titled {title:?}")))?;

        let (sender, latencies) = mpsc::channel();
        let mut schedule = ReadSchedule::new(interval);
        let session = system.start(
            window.handle,
            Crop::Layout(Layout::one(region)),
            interval.min(),
            Box::new(move |frame| {
                let sample = frame.map(|frame| {
                    let read = fill.percent(frame.image).is_ok();
                    (frame.latency(), read)
                });
                // A send fails only once the benchmark stopped listening.
                let listening = sender.send(sample).is_ok();
                NextFrame::After(schedule.after_read(if listening {
                    Activity::Changed
                } else {
                    Activity::Steady
                }))
            }),
        )?;
        let started = Instant::now();
        let mut samples = Vec::with_capacity(SAMPLES);
        while samples.len() < SAMPLES {
            let (latency, _read) = latencies.recv_timeout(Duration::from_secs(10))??;
            samples.push(latency);
        }
        let elapsed = started.elapsed();
        session.close()?;
        samples.sort_unstable();
        let name = format!(
            "capture to decision, {}x{}",
            region.size().width,
            region.size().height
        );
        writeln!(
            out,
            "{name:<36} median {:>10.2?}  p99 {:>10.2?}  ({:.1} reads/s)",
            percentile(&samples, 50),
            percentile(&samples, 99),
            f64::from(u32::try_from(SAMPLES)?) / elapsed.as_secs_f64(),
        )?;
        Ok(())
    }

    fn parse_region(text: Option<&str>) -> Result<Region, Box<dyn std::error::Error>> {
        let text = text.unwrap_or("0,0,64,64");
        let parts: Vec<u32> = text
            .split(',')
            .map(|part| part.trim().parse())
            .collect::<Result<_, _>>()?;
        match *parts.as_slice() {
            [x, y, width, height] => Region::new(x, y, width, height),
            _ => None,
        }
        .ok_or_else(|| format!("{text:?} is not x,y,width,height").into())
    }

    fn millis(name: &str, default: Duration) -> Result<Duration, Box<dyn std::error::Error>> {
        let Ok(value) = env::var(name) else {
            return Ok(default);
        };
        Ok(Duration::from_millis(value.parse()?))
    }
}

#[cfg(not(windows))]
mod live {
    //! The live capture needs Windows.

    use super::{BarFill, BenchResult, Write};

    pub(super) fn run(out: &mut impl Write, _fill: BarFill) -> BenchResult {
        writeln!(out, "capture: needs Windows")?;
        Ok(())
    }
}
