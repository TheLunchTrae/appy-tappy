//! Which pixels of an image are ink, and the line of text among them.
//!
//! Every pass over an image's ink reads it here: a read, a lesson, and the
//! setup app's search for text of a size. So they agree about which pixels
//! are ink and about which rows are the line.
//!
//! Text in an application is often more than one color: a gradient, an
//! outline, a shadow, digits in two tones. So a reader finds ink by
//! [contrast](InkRule::Contrast) with the background, which is one color
//! far more often than the text is, and by a
//! [text color](InkRule::Color) only where the user gave it one, for text
//! over a scene that is no one color.

use std::ops::Range;

use super::super::{Image, Rgb, Size};

/// How a reader tells an image's ink from its background.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InkRule {
    /// A pixel is ink when its red, green, or blue is more than `tolerance`
    /// from the background's. The background is the color of the image's
    /// border: for each of red, green, and blue, the median among the
    /// pixels along its four edges, so characters that reach an edge, or
    /// a frame along one, do not change it while they cover less than
    /// half the border.
    Contrast {
        /// How far a pixel may be from the background and still be
        /// background.
        tolerance: u8,
    },
    /// A pixel is ink when each of its red, green, and blue is within
    /// `tolerance` of `color`'s.
    Color {
        /// The text color.
        color: Rgb,
        /// How far a pixel may be from the text color and still be ink.
        tolerance: u8,
    },
}

impl InkRule {
    /// The rule of a reference with the text color `text_color`, or of one
    /// that finds its characters by contrast when it has none. `tolerance`
    /// is the reference's.
    #[must_use]
    pub const fn new(text_color: Option<Rgb>, tolerance: u8) -> Self {
        match text_color {
            Some(color) => Self::Color { color, tolerance },
            None => Self::Contrast { tolerance },
        }
    }
}

/// Which pixels of an image are ink, as [`DigitReader::ink`] finds them
/// once.
///
/// [`DigitReader::ink`]: super::DigitReader::ink
#[derive(Debug, PartialEq, Eq)]
pub struct Ink {
    /// Whether each pixel is ink, row by row from the top.
    inked: Vec<bool>,
    size: Size,
}

impl Ink {
    /// The ink of `image` by `rule`. A rule of contrast takes the
    /// background from `backdrop`: `image` itself for a region read by
    /// itself, or the part of `image` whose text is looked for in the rest
    /// of it.
    pub(super) fn new(image: &Image, rule: InkRule, backdrop: &Image) -> Self {
        // One pass serves both rules: a pixel is ink when it is near the
        // text color, or when it is not near the background.
        let (color, tolerance, near_is_ink) = match rule {
            InkRule::Contrast { tolerance } => (background(backdrop), tolerance, false),
            InkRule::Color { color, tolerance } => (color, tolerance, true),
        };
        Self {
            inked: image
                .pixels()
                .iter()
                .map(|&pixel| Rgb::of_pixel(pixel).near(color, tolerance) == near_is_ink)
                .collect(),
            size: image.size(),
        }
    }

    /// The size of the image.
    #[must_use]
    pub const fn size(&self) -> Size {
        self.size
    }

    /// Whether the pixel at (`x`, `y`) is ink. No pixel outside the image
    /// is.
    #[must_use]
    pub fn at(&self, x: u32, y: u32) -> bool {
        x < self.size.width
            && index(y)
                .checked_mul(index(self.size.width))
                .and_then(|start| self.inked.get(start.checked_add(index(x))?))
                .is_some_and(|&inked| inked)
    }

    /// How many pixels of row `y` in `columns` are ink. The columns and
    /// rows outside the image hold none.
    pub(super) fn count(&self, columns: Range<u32>, y: u32) -> usize {
        self.row(y)
            .get(index(columns.start)..index(columns.end.min(self.size.width)))
            .unwrap_or_default()
            .iter()
            .filter(|&&inked| inked)
            .count()
    }

    /// Row `y`, or no pixels outside the image.
    fn row(&self, y: u32) -> &[bool] {
        let width = index(self.size.width);
        index(y)
            .checked_mul(width)
            .and_then(|start| self.inked.get(start..start.checked_add(width)?))
            .unwrap_or_default()
    }

    /// The rows of the line of text in `columns` of `rows`: the run of
    /// adjacent inked rows there that holds the most ink, the upper of two
    /// that hold as much, so a stray pixel elsewhere does not stretch the
    /// line. `None` without ink there.
    #[must_use]
    pub fn line(&self, columns: Range<u32>, rows: Range<u32>) -> Option<Range<u32>> {
        let mut best: Option<(Range<u32>, usize)> = None;
        let mut run: Option<(Range<u32>, usize)> = None;
        let end = rows.end;
        // The row past the last ends a run that reaches the bottom edge.
        for y in rows.chain([end]) {
            let ink = if y < end {
                self.count(columns.clone(), y)
            } else {
                0
            };
            if ink > 0 {
                let (line, total) = run.get_or_insert((y..y, 0));
                line.end = y.saturating_add(1);
                *total = total.saturating_add(ink);
            } else if let Some(ended) = run.take()
                && best.as_ref().is_none_or(|(_, most)| ended.1 > *most)
            {
                best = Some(ended);
            }
        }
        best.map(|(line, _)| line)
    }
}

/// The background of `image`, as [`InkRule::Contrast`] defines it. Of two
/// values that each half of the border has, the lower one is the median.
fn background(image: &Image) -> Rgb {
    // `chunks_exact` refuses a width of 0, which no image has.
    let mut rows = image
        .pixels()
        .chunks_exact(index(image.size().width).max(1));
    let top = rows.next();
    let bottom = rows.next_back();
    let sides = rows.flat_map(|row| {
        row.first()
            .into_iter()
            .chain(row.get(1..).and_then(<[_]>::last))
    });
    let mut border = ChannelCounts::default();
    for &pixel in top.into_iter().chain(bottom).flatten().chain(sides) {
        border.count(Rgb::of_pixel(pixel));
    }
    border.median()
}

/// How many pixels have each value of red, of green, and of blue.
struct ChannelCounts {
    channels: [[u32; 256]; 3],
    pixels: u32,
}

impl Default for ChannelCounts {
    fn default() -> Self {
        Self {
            channels: [[0; 256]; 3],
            pixels: 0,
        }
    }
}

impl ChannelCounts {
    fn count(&mut self, pixel: Rgb) {
        for (counts, value) in self
            .channels
            .iter_mut()
            .zip([pixel.red, pixel.green, pixel.blue])
        {
            if let Some(count) = counts.get_mut(usize::from(value)) {
                *count = count.saturating_add(1);
            }
        }
        self.pixels = self.pixels.saturating_add(1);
    }

    /// The color made of each channel's median.
    fn median(&self) -> Rgb {
        let [red, green, blue] = self.channels.map(|counts| {
            let mut at_most = 0_u32;
            (0..=u8::MAX)
                .zip(counts)
                .find(|&(_, count)| {
                    at_most = at_most.saturating_add(count);
                    at_most.saturating_mul(2) >= self.pixels
                })
                .map_or(0, |(value, _)| value)
        });
        Rgb { red, green, blue }
    }
}

/// `value` as an index. A pixel's column or row fits a `usize` wherever the
/// image's pixels fit in memory.
fn index(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ink where `rows` have a `#`.
    fn ink(rows: &[&str]) -> Ink {
        let width = u32::try_from(rows[0].len()).unwrap();
        let height = u32::try_from(rows.len()).unwrap();
        Ink {
            inked: rows
                .iter()
                .flat_map(|row| row.bytes())
                .map(|cell| cell == b'#')
                .collect(),
            size: Size { width, height },
        }
    }

    const fn gray(value: u8) -> Rgb {
        Rgb {
            red: value,
            green: value,
            blue: value,
        }
    }

    /// An image of `rows` of pixels.
    fn image(rows: &[&[Rgb]]) -> Image {
        let size = Size {
            width: u32::try_from(rows[0].len()).unwrap(),
            height: u32::try_from(rows.len()).unwrap(),
        };
        let bgra = rows
            .iter()
            .flat_map(|row| row.iter())
            .flat_map(|pixel| [pixel.blue, pixel.green, pixel.red, 0])
            .collect();
        Image::new(size, bgra).unwrap()
    }

    /// `picture` as rows of `#` for ink.
    fn inked(picture: &Image, rule: InkRule) -> Vec<String> {
        let found = Ink::new(picture, rule, picture);
        let Size { width, height } = found.size();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| if found.at(x, y) { '#' } else { '.' })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn a_reference_with_a_text_color_finds_ink_by_it() {
        assert_eq!(
            InkRule::new(Some(gray(9)), 3),
            InkRule::Color {
                color: gray(9),
                tolerance: 3
            }
        );
        assert_eq!(InkRule::new(None, 3), InkRule::Contrast { tolerance: 3 });
    }

    #[test]
    fn ink_by_color_is_each_channel_within_the_tolerance() {
        let color = Rgb {
            red: 200,
            green: 100,
            blue: 50,
        };
        let near = |red, green, blue| Rgb { red, green, blue };
        let picture = image(&[&[
            near(205, 95, 55),
            near(200, 100, 56),
            near(200, 94, 50),
            near(206, 100, 50),
        ]]);
        assert_eq!(
            inked(
                &picture,
                InkRule::Color {
                    color,
                    tolerance: 5
                }
            ),
            ["#..."]
        );
    }

    #[test]
    fn ink_by_contrast_is_any_channel_past_the_tolerance_from_the_background() {
        let back = gray(100);
        let stray = |red, green, blue| Rgb { red, green, blue };
        let picture = image(&[
            &[back, back, back, back, back, back],
            &[
                back,
                stray(105, 95, 100),
                stray(106, 100, 100),
                stray(100, 94, 100),
                stray(100, 100, 106),
                back,
            ],
            &[back, back, back, back, back, back],
        ]);
        assert_eq!(
            inked(&picture, InkRule::Contrast { tolerance: 5 }),
            ["......", "..###.", "......"]
        );
    }

    #[test]
    fn the_background_is_the_color_of_the_border() {
        let (back, text) = (gray(30), gray(220));
        // Most of the picture is text, and none of its border.
        let filled = image(&[
            &[back, back, back, back, back],
            &[back, text, text, text, back],
            &[back, text, text, text, back],
            &[back, text, text, text, back],
            &[back, text, text, text, back],
            &[back, back, back, back, back],
        ]);
        assert_eq!(background(&filled), back);
        // Text reaches 5 of the 12 pixels of the border.
        let cut = image(&[
            &[text, text, text, back],
            &[text, text, text, back],
            &[text, back, back, back],
            &[back, back, back, back],
        ]);
        assert_eq!(background(&cut), back);
        assert_eq!(
            inked(&cut, InkRule::Contrast { tolerance: 0 }),
            ["###.", "###.", "#...", "...."]
        );
    }

    #[test]
    fn the_background_is_the_median_of_each_channel() {
        let color = |red, green| Rgb {
            red,
            green,
            blue: 7,
        };
        let (most, some, few) = (color(10, 200), color(20, 100), color(30, 0));
        // A border of 12 pixels: 5, 4, and 3 of them.
        let picture = image(&[
            &[most, most, most, most],
            &[most, few, few, some],
            &[some, most, most, some],
            &[some, few, few, few],
        ]);
        assert_eq!(background(&picture), color(20, 100));
        // Half the border each: the lower value.
        assert_eq!(background(&image(&[&[gray(20), gray(10)]])), gray(10));
        assert_eq!(background(&image(&[&[gray(3)], &[gray(9)]])), gray(3));
        assert_eq!(
            background(&image(&[&[gray(255)], &[gray(255)], &[gray(255)]])),
            gray(255)
        );
    }

    #[test]
    fn no_pixel_outside_the_image_is_ink() {
        // Ink only at (0, 1): (3, 0) would land on it if the row wrapped.
        let ink = ink(&["...", "#.#"]);
        assert!(ink.at(0, 1));
        assert!(!ink.at(1, 1));
        assert!(!ink.at(3, 0));
        assert!(!ink.at(0, 2));
        assert!(!ink.at(u32::MAX, 1));
        assert!(!ink.at(0, u32::MAX));
        assert_eq!(ink.count(0..3, 1), 2);
        assert_eq!(ink.count(1..3, 1), 1);
        assert_eq!(ink.count(0..2, 1), 1);
        assert_eq!(ink.count(0..u32::MAX, 1), 2, "cut at the right edge");
        assert_eq!(ink.count(4..9, 1), 0, "past the right edge");
        assert_eq!(ink.count(0..3, 2), 0);
    }

    #[test]
    fn the_line_is_the_run_of_inked_rows_with_the_most_ink() {
        let ink = ink(&[
            "#.....", //
            "......", //
            "####..", //
            "####..", //
            "......", //
            "##....", //
            "##....", //
            "##....", //
            "##....", //
            "......", //
            "######", //
            "######", //
        ]);
        let all = || 0..6;
        assert_eq!(ink.line(all(), 0..12), Some(10..12), "the most ink");
        // Rows 2 to 3 and rows 5 to 8 hold 8 pixels each: the upper one.
        assert_eq!(ink.line(all(), 0..10), Some(2..4));
        assert_eq!(ink.line(all(), 4..10), Some(5..9));
        // Only the ink in the columns counts, and only the rows asked for.
        assert_eq!(ink.line(2..6, 0..10), Some(2..4));
        assert_eq!(ink.line(0..2, 0..10), Some(5..9));
        assert_eq!(ink.line(all(), 0..2), Some(0..1));
        assert_eq!(ink.line(all(), 10..11), Some(10..11), "cut by the rows");
        assert_eq!(ink.line(all(), 10..40), Some(10..12), "cut by the image");
        assert_eq!(ink.line(all(), 1..2), None);
        assert_eq!(ink.line(4..6, 0..10), None);
    }
}
