//! The values, jobs, and frames the reader's tests read.

use appytappy_config::anti_cheat::ProcessId;
use appytappy_config::app_file::{PinDef, SizeChangesDef};
use appytappy_config::signal::SignalName;
use appytappy_config::window_match::{MustMatch, Pattern, WindowMatch};
use appytappy_sources::screen::check::{Check, ColorMatch};
use appytappy_sources::screen::saved::{SavedCheck, SavedValue};
use appytappy_sources::screen::{Image, OutsideWindow, Region};
use appytappy_sources::system::Rgb;

use super::*;
use crate::plan::{Labels, Runs, ValuePlan};

pub(super) const RED: Rgb = Rgb {
    red: 255,
    green: 0,
    blue: 0,
};
pub(super) const WINDOW: Size = Size {
    width: 800,
    height: 600,
};
pub(super) const SMALL: Size = Size {
    width: 640,
    height: 480,
};
/// A 2x2 value that is true while its region is red.
pub(super) fn red_at(name: &str, x: u32, y: u32) -> ValuePlan {
    drawn_at(name, x, y, WINDOW)
}

/// A 2x2 value drawn on a window of `window_size`, read only at that
/// size.
pub(super) fn drawn_at(name: &str, x: u32, y: u32, window_size: Size) -> ValuePlan {
    scaling(name, x, y, window_size, SizeChangesDef::Rarely)
}

/// A 2x2 value drawn on a window of `window_size`, pinned to its top
/// left, read as `size_changes` say.
pub(super) fn scaling(
    name: &str,
    x: u32,
    y: u32,
    window_size: Size,
    size_changes: SizeChangesDef,
) -> ValuePlan {
    ValuePlan {
        signal: SignalName::new(name).expect("valid"),
        check: SavedValue {
            drawn: SavedCheck {
                window_size,
                region: Region::new(x, y, 2, 2).expect("not empty"),
                check: Check::Color(ColorMatch {
                    color: RED,
                    tolerance: 10,
                }),
            },
            fixes: Vec::new(),
            pin: PinDef::TopLeft,
            size_changes,
        },
    }
}

pub(super) fn plan(values: Vec<ValuePlan>) -> Arc<AppPlan> {
    Arc::new(AppPlan {
        labels: Labels {
            name: "Example App".to_owned(),
            values: values
                .iter()
                .map(|value| value.signal.to_string())
                .collect(),
            checked: None,
        },
        runs: Runs {
            window: WindowMatch {
                title: Pattern::new("Main*"),
                class: Pattern::new("*"),
                executable: Pattern::new("Example.exe"),
                must_match: MustMatch {
                    title: true,
                    class: true,
                },
                fallback: false,
            },
            values,
            rules: Vec::new(),
            clicks: Arc::default(),
        },
    })
}

pub(super) fn job(values: Vec<ValuePlan>) -> Job {
    Job {
        session: SessionId(3),
        reader: ReaderNumber(2),
        plan: plan(values),
        watched: Watched {
            process: ProcessId::new(43),
            executable: "Example.exe".to_owned(),
        },
        interval: ReadInterval::DEFAULT,
    }
}

/// A `width` by `height` frame, red where `red` says and black elsewhere.
pub(super) fn frame_image(width: u32, height: u32, red: impl Fn(u32, u32) -> bool) -> Image {
    let mut bgra = Vec::new();
    for y in 0..height {
        for x in 0..width {
            bgra.extend(if red(x, y) {
                [0, 0, 255, 255]
            } else {
                [0, 0, 0, 255]
            });
        }
    }
    Image::new(Size { width, height }, bgra).expect("matching length")
}

pub(super) fn frame(image: &Image, window_size: Size) -> Frame<'_> {
    Frame {
        window_size,
        image,
        received: Instant::now(),
        delay: Duration::ZERO,
    }
}

pub(super) fn outside(window_size: Size) -> io::Result<Frame<'static>> {
    Err(OutsideWindow { window_size }.into())
}

/// Why a value drawn only at other sizes is not read, as it is sent.
pub(super) const NOT_DRAWN: &str = "is not read at the window's size now: it is read only at the window \
                         sizes it was drawn at; resize the window, or fix the value at this \
                         size in AppyTappy Setup";

/// Why a value that scales past the window is not read, as it is sent.
pub(super) const OUTSIDE: &str = "is not read at the window's size now: scaled to this window size, its \
                       region reaches outside the window; fix the value at this size in \
                       AppyTappy Setup";

/// What a reader sends, as a reader with a bug that panics whenever a
/// value reads true would send it.
#[derive(Debug, PartialEq)]
pub(super) struct Tripwire(pub(super) Reading);

#[expect(
    clippy::fallible_impl_from,
    reason = "the panic stands in for a bug in reading a frame, which the frame handler must survive"
)]
impl From<Reading> for Tripwire {
    fn from(reading: Reading) -> Self {
        if let Reading::Sample {
            value: Value::Flag(true),
            ..
        } = reading
        {
            panic!("broke");
        }
        Self(reading)
    }
}
