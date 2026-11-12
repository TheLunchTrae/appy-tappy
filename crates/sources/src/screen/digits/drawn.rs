//! Text drawn for tests the way a game draws a counter: bold white strokes
//! with a black outline on a colored background, anti-aliased, at any size
//! and at any fraction of a pixel.
//!
//! The same character then lands on the pixel grid a little differently
//! each time, as it does in an application that scales its interface or
//! centers a number whose width changes. The strokes are made up here, so
//! the tests carry no application's font.

use crate::screen::{Image, Size};

/// Sub-samples along each side of a pixel: its color is the mean of
/// `SAMPLES` by `SAMPLES` points.
const SAMPLES: u32 = 4;

/// Half a stroke's width, in digit heights: a bold font.
const STROKE: f64 = 0.09;

/// The outline's width, in digit heights.
const OUTLINE: f64 = 0.05;

/// The space between two characters' outlines, in digit heights.
const GAP: f64 = 0.12;

/// The background's blue, green, and red.
const BACKGROUND: [u32; 3] = [200, 120, 60];

/// The characters' color.
const INK: u32 = 255;

/// A stroke's center line, from one point to another. In a glyph the
/// points are in digit heights, across from the glyph's left and down from
/// the digits' top, so a digit spans 0 to 1 down; on a canvas they are in
/// pixels.
type Stroke = [(f64, f64); 2];

/// How a `%` is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Percent {
    /// Its slash runs under one ring and over the other, so the three
    /// share columns and read as one character.
    Joined,
    /// Its rings stand clear of its slash, with blank columns between the
    /// three marks.
    Apart,
}

/// Where and how large text is drawn.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Pen {
    /// The digits' height in pixels, between the center lines of their top
    /// and bottom strokes.
    pub(crate) height: f64,
    /// Where the first character's center lines start: pixels across and
    /// down the canvas, fractions included.
    pub(crate) at: (f64, f64),
    /// How each `%` is drawn.
    pub(crate) percent: Percent,
}

impl Pen {
    /// A pen for digits `height` pixels tall that starts `across` pixels
    /// from the canvas's left, with room for the outline above.
    pub(crate) fn new(height: f64, across: f64) -> Self {
        Self {
            height,
            at: (across, height * 0.25),
            percent: Percent::Joined,
        }
    }
}

/// The strokes that join `points` in order, back to the first when
/// `closed`.
fn strokes(points: &[(f64, f64)], closed: bool) -> Vec<Stroke> {
    let mut all: Vec<Stroke> = points.windows(2).map(|pair| [pair[0], pair[1]]).collect();
    if closed && let (Some(&last), Some(&first)) = (points.last(), points.first()) {
        all.push([last, first]);
    }
    all
}

/// A twelve-sided ring around (`across`, `down`).
fn ring(across: f64, down: f64, radius: f64) -> Vec<Stroke> {
    let points: Vec<(f64, f64)> = (0..12_u8)
        .map(|corner| {
            let angle = std::f64::consts::TAU * f64::from(corner) / 12.0;
            (
                radius.mul_add(angle.cos(), across),
                radius.mul_add(angle.sin(), down),
            )
        })
        .collect();
    strokes(&points, true)
}

const SIX: [(f64, f64); 12] = [
    (0.55, 0.05),
    (0.3, 0.0),
    (0.1, 0.15),
    (0.0, 0.5),
    (0.0, 0.8),
    (0.15, 1.0),
    (0.45, 1.0),
    (0.6, 0.8),
    (0.6, 0.6),
    (0.45, 0.45),
    (0.15, 0.45),
    (0.0, 0.6),
];

/// The strokes of `character` and the width of their center lines, in digit
/// heights.
///
/// # Panics
///
/// Panics for a character it cannot draw.
fn glyph(character: char, percent: Percent) -> (Vec<Stroke>, f64) {
    match character {
        '0' => (
            strokes(
                &[
                    (0.15, 0.0),
                    (0.45, 0.0),
                    (0.6, 0.2),
                    (0.6, 0.8),
                    (0.45, 1.0),
                    (0.15, 1.0),
                    (0.0, 0.8),
                    (0.0, 0.2),
                ],
                true,
            ),
            0.6,
        ),
        '1' => (strokes(&[(0.0, 0.25), (0.2, 0.0), (0.2, 1.0)], false), 0.2),
        '2' => (
            strokes(
                &[
                    (0.0, 0.2),
                    (0.15, 0.0),
                    (0.45, 0.0),
                    (0.6, 0.2),
                    (0.6, 0.4),
                    (0.0, 1.0),
                    (0.6, 1.0),
                ],
                false,
            ),
            0.6,
        ),
        '3' => (
            strokes(
                &[
                    (0.0, 0.1),
                    (0.2, 0.0),
                    (0.45, 0.0),
                    (0.6, 0.15),
                    (0.6, 0.35),
                    (0.4, 0.5),
                    (0.6, 0.65),
                    (0.6, 0.85),
                    (0.45, 1.0),
                    (0.2, 1.0),
                    (0.0, 0.9),
                ],
                false,
            ),
            0.6,
        ),
        '4' => (
            strokes(&[(0.45, 1.0), (0.45, 0.0), (0.0, 0.7), (0.6, 0.7)], false),
            0.6,
        ),
        '5' => (
            strokes(
                &[
                    (0.6, 0.0),
                    (0.05, 0.0),
                    (0.0, 0.45),
                    (0.4, 0.4),
                    (0.6, 0.55),
                    (0.6, 0.85),
                    (0.45, 1.0),
                    (0.15, 1.0),
                    (0.0, 0.9),
                ],
                false,
            ),
            0.6,
        ),
        '6' => (strokes(&SIX, false), 0.6),
        '7' => (strokes(&[(0.0, 0.0), (0.6, 0.0), (0.25, 1.0)], false), 0.6),
        '8' => {
            let mut all = strokes(
                &[
                    (0.15, 0.0),
                    (0.45, 0.0),
                    (0.55, 0.12),
                    (0.55, 0.33),
                    (0.45, 0.45),
                    (0.15, 0.45),
                    (0.05, 0.33),
                    (0.05, 0.12),
                ],
                true,
            );
            all.extend(strokes(
                &[
                    (0.15, 0.45),
                    (0.45, 0.45),
                    (0.6, 0.6),
                    (0.6, 0.85),
                    (0.45, 1.0),
                    (0.15, 1.0),
                    (0.0, 0.85),
                    (0.0, 0.6),
                ],
                true,
            ));
            (all, 0.6)
        },
        // A 6 turned upside down.
        '9' => {
            let turned: Vec<(f64, f64)> = SIX
                .iter()
                .map(|&(across, down)| (0.6 - across, 1.0 - down))
                .collect();
            (strokes(&turned, false), 0.6)
        },
        '.' => (vec![[(0.0, 0.97), (0.0, 0.97)]], 0.0),
        ',' => (vec![[(0.08, 0.97), (0.0, 1.2)]], 0.08),
        '-' => (vec![[(0.0, 0.55), (0.3, 0.55)]], 0.3),
        '%' => match percent {
            Percent::Joined => {
                let mut all = ring(0.16, 0.2, 0.16);
                all.extend(ring(0.74, 0.8, 0.16));
                all.push([(0.15, 1.0), (0.75, 0.0)]);
                (all, 0.9)
            },
            Percent::Apart => {
                let mut all = ring(0.16, 0.2, 0.16);
                all.extend(ring(1.24, 0.8, 0.16));
                all.push([(0.62, 1.0), (0.78, 0.0)]);
                (all, 1.4)
            },
        },
        other => panic!("no glyph for {other:?}"),
    }
}

/// How far (`across`, `down`) is from `stroke`'s center line.
fn distance([(left, top), (right, bottom)]: Stroke, across: f64, down: f64) -> f64 {
    let (run, fall) = (right - left, bottom - top);
    let length = run.hypot(fall).powi(2);
    let along = if length == 0.0 {
        0.0
    } else {
        ((across - left).mul_add(run, (down - top) * fall) / length).clamp(0.0, 1.0)
    };
    (across - along.mul_add(run, left)).hypot(down - along.mul_add(fall, top))
}

/// `text` drawn with `pen` on a `canvas`, which must be large enough: what
/// falls outside it is cut off.
///
/// # Panics
///
/// Panics for a character [`glyph`] cannot draw.
pub(crate) fn draw(text: &str, pen: Pen, canvas: Size) -> Image {
    let reach = (STROKE + OUTLINE) * pen.height;
    let mut placed: Vec<Stroke> = Vec::new();
    let (mut across, down) = pen.at;
    for character in text.chars() {
        let (all, width) = glyph(character, pen.percent);
        placed.extend(all.into_iter().map(|stroke| {
            stroke.map(|(x, y)| (x.mul_add(pen.height, across), y.mul_add(pen.height, down)))
        }));
        let advance = 2.0_f64.mul_add(STROKE + OUTLINE, width) + GAP;
        across = advance.mul_add(pen.height, across);
    }
    let mut bgra = Vec::new();
    for row in 0..canvas.height {
        for column in 0..canvas.width {
            let (left, top) = (f64::from(column), f64::from(row));
            // Only the strokes near the pixel can color it.
            let near: Vec<Stroke> = placed
                .iter()
                .filter(|[(x0, y0), (x1, y1)]| {
                    left + 1.0 >= x0.min(*x1) - reach
                        && left <= x0.max(*x1) + reach
                        && top + 1.0 >= y0.min(*y1) - reach
                        && top <= y0.max(*y1) + reach
                })
                .copied()
                .collect();
            bgra.extend(pixel(&near, (left, top), pen.height));
            bgra.push(u8::MAX);
        }
    }
    Image::new(canvas, bgra).expect("a pixel for each of the canvas's")
}

/// The blue, green, and red of the pixel whose corner is (`left`, `top`):
/// the mean of its sub-samples, each ink within a stroke, black within the
/// outline around it, and background elsewhere.
fn pixel(near: &[Stroke], (left, top): (f64, f64), height: f64) -> [u8; 3] {
    let (mut ink, mut background, mut samples) = (0_u32, 0_u32, 0_u32);
    let offset = |index: u32| (f64::from(index) + 0.5) / f64::from(SAMPLES);
    for row in 0..SAMPLES {
        for column in 0..SAMPLES {
            let (across, down) = (left + offset(column), top + offset(row));
            let nearest = near
                .iter()
                .map(|&stroke| distance(stroke, across, down))
                .fold(f64::INFINITY, f64::min);
            ink = ink.saturating_add(u32::from(nearest <= STROKE * height));
            background =
                background.saturating_add(u32::from(nearest > (STROKE + OUTLINE) * height));
            samples = samples.saturating_add(1);
        }
    }
    BACKGROUND.map(|channel| {
        ink.checked_mul(INK)
            .zip(background.checked_mul(channel))
            .and_then(|(ink, background)| ink.checked_add(background)?.checked_div(samples))
            .and_then(|mean| u8::try_from(mean).ok())
            .expect("a mean of bytes")
    })
}
