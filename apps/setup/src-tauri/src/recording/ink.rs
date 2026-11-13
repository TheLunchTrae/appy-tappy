//! A reading's ink as the text reader takes it.

use std::ops::Range;

use appytappy_sources::screen::digits::Ink;

/// One line of text for the text reader: gray pixels, one byte each, row by
/// row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Line {
    /// Its width in pixels.
    pub(crate) width: u32,
    /// Its height in pixels.
    pub(crate) height: u32,
    /// Its pixels: 0 is black, 255 white.
    pub(crate) pixels: Vec<u8>,
}

impl Line {
    /// `ink` as the text reader takes it: black ink on white, cut to the
    /// line of text with a margin of white around it, as text on a page
    /// has. Black on white reads better than the application's own colors,
    /// which can be any pair. The line is [`rows`]. `None` without ink.
    pub(crate) fn of(ink: &Ink) -> Option<Self> {
        let size = ink.size();
        let rows = rows(ink)?;
        let inked = |x: &u32| rows.clone().any(|y| ink.at(*x, y));
        let left = (0..size.width).find(inked)?;
        let right = (0..size.width).rfind(inked)?.saturating_add(1);
        // Half the line's height, at least two pixels.
        let margin = (rows.end.saturating_sub(rows.start) / 2).max(2);
        let width = right
            .saturating_sub(left)
            .saturating_add(margin.saturating_mul(2));
        let height = rows
            .end
            .saturating_sub(rows.start)
            .saturating_add(margin.saturating_mul(2));
        let stride = usize::try_from(width).ok()?;
        let skipped = usize::try_from(margin).ok()?;
        let mut pixels = vec![u8::MAX; stride.checked_mul(usize::try_from(height).ok()?)?];
        for (row, y) in pixels.chunks_exact_mut(stride).skip(skipped).zip(rows) {
            for (pixel, x) in row.iter_mut().skip(skipped).zip(left..right) {
                if ink.at(x, y) {
                    *pixel = 0;
                }
            }
        }
        Some(Self {
            width,
            height,
            pixels,
        })
    }
}

/// The rows of the line of text in `ink`: the rows the number reader takes,
/// [`Ink::line`], with the nearest mark on each side of them. A mark is a
/// block of inked rows no taller than half the letters' height, with at
/// most a quarter of their height of blank rows, and at least one, between
/// it and them. Those hold the dots of an i or a j and the accents, which
/// the number reader has no use for and the text reader needs to tell the
/// letters apart. A dot sits about a tenth to a fifth of the letters'
/// height from them. Only the nearest mark joins, so a column of stray
/// pixels adds at most one. A line of text nearby is usually taller and
/// further away, so it stays out; one that is short and close enough, such
/// as a line the region cuts to a few rows, joins like a mark, since the
/// region is meant to hold one line. `None` without ink.
fn rows(ink: &Ink) -> Option<Range<u32>> {
    let size = ink.size();
    let letters = ink.line(0..size.width, 0..size.height)?;
    let height = letters.end.saturating_sub(letters.start);
    let near = Near {
        gap: (height / 4).max(1),
        tallest: (height / 2).max(1),
    };
    let inked = |y: u32| (0..size.width).any(|x| ink.at(x, y));
    let start = near
        .mark((0..letters.start).rev(), inked)
        .unwrap_or(letters.start);
    let end = near
        .mark(letters.end..size.height, inked)
        .map_or(letters.end, |last| last.saturating_add(1));
    Some(start..end)
}

/// How near the letters, and how small, a mark is that joins their line.
#[derive(Debug, Clone, Copy)]
struct Near {
    /// The most blank rows between the letters and a mark.
    gap: u32,
    /// The most rows a mark takes.
    tallest: u32,
}

impl Near {
    /// The row furthest from the letters of the mark that `outward`, the
    /// rows past one side of them going away from them, holds first, or
    /// `None` when the first inked rows there are too far or too tall.
    fn mark(self, outward: impl Iterator<Item = u32>, inked: impl Fn(u32) -> bool) -> Option<u32> {
        let mut blank = 0_u32;
        let mut mark: Option<(u32, u32)> = None;
        for y in outward {
            match (inked(y), mark) {
                (false, None) => {
                    blank = blank.saturating_add(1);
                    if blank > self.gap {
                        return None;
                    }
                },
                (true, None) => mark = Some((y, 1)),
                (true, Some((_, rows))) => {
                    if rows >= self.tallest {
                        return None;
                    }
                    mark = Some((y, rows.saturating_add(1)));
                },
                (false, Some((last, _))) => return Some(last),
            }
        }
        mark.map(|(last, _)| last)
    }
}

#[cfg(test)]
mod tests {
    use appytappy_sources::screen::digits::DigitReader;
    use appytappy_sources::screen::{Image, Rgb, Size};

    use super::*;

    /// The line of a 6 by 7 image, white where `ink` says so and black
    /// elsewhere, read with white as ink.
    fn line(ink: impl Fn(usize, usize) -> bool) -> Option<Line> {
        let pixels = (0..7)
            .flat_map(|y| (0..6).map(move |x| (x, y)))
            .flat_map(|(x, y)| if ink(x, y) { [255; 4] } else { [0, 0, 0, 255] })
            .collect();
        let image = Image::new(
            Size {
                width: 6,
                height: 7,
            },
            pixels,
        )
        .expect("matching length");
        let white = Rgb {
            red: 255,
            green: 255,
            blue: 255,
        };
        Line::of(&DigitReader::new(white, 0).ink(&image))
    }

    #[test]
    fn the_line_is_the_text_s_rows_in_black_on_white_with_a_margin() {
        // Text at columns 2 and 3 of rows 3 and 4, with a gap in its second
        // row, and a stray pixel on row 0 further left.
        let text = |x: usize, y: usize| (y == 3 && (2..4).contains(&x)) || (y == 4 && x == 3);
        let line = line(|x, y| text(x, y) || (x, y) == (0, 0)).expect("it shows ink");
        assert_eq!(
            (line.width, line.height),
            (6, 6),
            "two by two and a margin of two"
        );
        let black: Vec<usize> = line
            .pixels
            .iter()
            .enumerate()
            .filter_map(|(index, &pixel)| (pixel == 0).then_some(index))
            .collect();
        assert_eq!(black, [14, 15, 21], "the stray pixel is no part of it");
        assert!(line.pixels.iter().all(|&pixel| pixel == 0 || pixel == 255));
    }

    #[test]
    fn the_line_keeps_a_dot_and_a_mark_one_blank_row_from_the_letters() {
        // A stem on rows 2 to 4, an i's dot on row 0 and a mark under it on
        // row 6, each one blank row apart, as in a lowercase line with no
        // ascenders.
        let line = line(|x, y| x == 1 && y != 1 && y != 5).expect("it shows ink");
        assert_eq!(
            (line.width, line.height),
            (7, 13),
            "seven rows and a margin of three"
        );
        let black: Vec<usize> = line
            .pixels
            .iter()
            .enumerate()
            .filter_map(|(index, &pixel)| (pixel == 0).then_some(index))
            .collect();
        assert_eq!(black, [24, 38, 45, 52, 66], "the dot and the mark show");
    }

    /// The rows of the line in ink drawn as `#`, one string a row.
    fn rows_in(drawn: &[&str]) -> Option<Range<u32>> {
        let size = Size {
            width: u32::try_from(drawn[0].len()).expect("narrow"),
            height: u32::try_from(drawn.len()).expect("short"),
        };
        let pixels = drawn
            .iter()
            .flat_map(|row| row.bytes())
            .flat_map(|cell| {
                if cell == b'#' {
                    [255; 4]
                } else {
                    [0, 0, 0, 255]
                }
            })
            .collect();
        let image = Image::new(size, pixels).expect("matching length");
        let white = Rgb {
            red: 255,
            green: 255,
            blue: 255,
        };
        rows(&DigitReader::new(white, 0).ink(&image))
    }

    #[test]
    fn a_dot_or_an_accent_near_the_letters_joins_the_line() {
        let two_rows_one_blank_row_above = [
            ".#.", ".#.", "...", "###", "#.#", "#.#", "###", //
        ];
        assert_eq!(rows_in(&two_rows_one_blank_row_above), Some(0..7));
        // A quarter of eight rows is two.
        let two_blank_rows_above_eight = [
            ".#.", "...", "...", "###", "#.#", "#.#", "#.#", "#.#", "#.#", "#.#", "###",
        ];
        assert_eq!(rows_in(&two_blank_rows_above_eight), Some(0..11));
    }

    #[test]
    fn a_nearby_line_of_text_stays_out_of_the_line() {
        let letters = ["###"; 8];
        let lighter = ["..#"; 5];
        for blank in [1, 2] {
            let below: Vec<&str> = letters
                .iter()
                .chain(&["..."; 2][..blank])
                .chain(&lighter)
                .copied()
                .collect();
            assert_eq!(rows_in(&below), Some(0..8), "{blank} blank rows below");
            let above: Vec<&str> = lighter
                .iter()
                .chain(&["..."; 2][..blank])
                .chain(&letters)
                .copied()
                .collect();
            let first = u32::try_from(blank).expect("small").saturating_add(5);
            assert_eq!(
                rows_in(&above),
                Some(first..first.saturating_add(8)),
                "{blank} blank rows above"
            );
        }
    }

    #[test]
    fn a_column_of_specks_adds_only_the_nearest() {
        let specks = [
            "#..", "...", "#..", "...", "#..", "...", "#..", "...", "###", "###", "###", "###",
            "...", "#..", "...", "#..",
        ];
        assert_eq!(rows_in(&specks), Some(6..14));
    }

    #[test]
    fn a_tall_line_has_a_margin_of_half_its_height() {
        let line = line(|x, y| x == 1 && y < 6).expect("it shows ink");
        assert_eq!((line.width, line.height), (7, 12));
    }

    #[test]
    fn an_image_without_ink_has_no_line() {
        assert_eq!(line(|_, _| false), None);
    }
}
