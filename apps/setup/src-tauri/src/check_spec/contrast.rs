//! Choosing the tolerance a reference finds its characters by contrast
//! with, from the samples it learns from.
//!
//! A tolerance too low takes a shadow, a glow, or the blur between two
//! characters for ink, so characters touch; one too high leaves strokes
//! out, so a character falls apart or is gone. Either way a sample shows
//! another count of characters than its text has, and the reference cannot
//! learn it. So the samples say which tolerances work, and nobody has to
//! set one.

use appytappy_sources::screen::digits::{DigitReader, InkRule};
use appytappy_sources::screen::{Image, Rgb, Size};

/// The gap between the tolerances tried. Finer steps cost more lessons
/// and find no tolerance that matters: a sample reads over a run of
/// tolerances far wider than this.
const STEP: u8 = 4;

/// The tolerance at which a reference that finds its characters by
/// contrast learns the most of `samples`, each a picture and the text it
/// shows, in order: every one of them, when a tolerance does that.
///
/// It is `current` while no tolerance learns more, so the tolerance changes
/// only when a sample needs it to, and what was read or recorded with it
/// stays as it was read. Otherwise it is the middle of the longest run of
/// tolerances that learn the most, the lower of two runs as long, among
/// every [`STEP`]th from 0: as far as can be from both a tolerance too low
/// and one too high.
///
/// A sample no tolerance learns, such as a box whose text is half typed or
/// a saved picture cut for a text color, is left out of the count as it is
/// left out of each lesson, so it does not move the tolerance the others
/// read with.
pub(crate) fn fitted(samples: &[(Image, &str)], current: u8) -> u8 {
    let learned = |tolerance: u8| {
        let mut reader = DigitReader::new(InkRule::Contrast { tolerance });
        samples
            .iter()
            .filter(|(picture, text)| {
                let mut trial = reader.clone();
                let learns = trial.learn(picture, text).is_ok();
                if learns {
                    reader = trial;
                }
                learns
            })
            .count()
    };
    let at_current = learned(current);
    if at_current == samples.len() {
        return current;
    }
    let grid: Vec<(u8, usize)> = (0..=u8::MAX)
        .step_by(usize::from(STEP))
        .map(|tolerance| (tolerance, learned(tolerance)))
        .collect();
    let most = grid
        .iter()
        .map(|&(_, count)| count)
        .fold(at_current, usize::max);
    if at_current == most {
        return current;
    }
    let mut longest: Option<(u8, u8)> = None;
    let mut run: Option<(u8, u8)> = None;
    for (tolerance, count) in grid {
        if count == most {
            let (_, last) = run.get_or_insert((tolerance, tolerance));
            *last = tolerance;
        } else {
            run = None;
        }
        let length = |(first, last): (u8, u8)| last.abs_diff(first);
        if let Some(ongoing) = run
            && longest.is_none_or(|best| length(ongoing) > length(best))
        {
            longest = Some(ongoing);
        }
    }
    longest.map_or(current, |(first, last)| {
        let middle = first.midpoint(last);
        middle.saturating_sub(middle.abs_diff(first) % STEP)
    })
}

/// How far from the background a pixel must be to count as drawn, for
/// [`hides_a_tone`]: further than the grain of a plain background.
const DRAWN: u8 = 16;

/// How many pixels a sample's characters have for each pixel of a tone it
/// leaves out, at the fewest. An outline or a shadow has more pixels than
/// that; a stray dark pixel beside a character has fewer.
const CHARACTER_PIXELS_PER_HIDDEN: u64 = 4;

/// Whether `tolerance` leaves a tone of the text out of one of `samples`,
/// each a picture and the text it shows: an outline or a shadow that is
/// nearer the picture's background than `tolerance`, so that it counts as
/// background there.
///
/// Such a reference reads only over backgrounds that hide the same tone.
/// Over a background further from the outline, the outline is ink, the
/// characters are thicker or touch, and no shape matches what the reference
/// learned.
///
/// The tone is told from a character's anti-aliased edge by where its color
/// lies: an edge is between the background and the characters, and an
/// outline or shadow is on the background's other side.
pub(crate) fn hides_a_tone(samples: &[(Image, &str)], tolerance: u8) -> bool {
    tolerance > DRAWN
        && samples
            .iter()
            .any(|(picture, _text)| hidden_tone(picture, tolerance))
}

/// [`hides_a_tone`] for one picture.
fn hidden_tone(picture: &Image, tolerance: u8) -> bool {
    let characters = DigitReader::new(InkRule::Contrast { tolerance }).ink(picture);
    let drawn = DigitReader::new(InkRule::Contrast { tolerance: DRAWN }).ink(picture);
    let Size { width, height } = picture.size();
    let mut text = Mean::default();
    let mut background = Mean::default();
    // The pixels drawn on the background that the characters leave out.
    let mut left_out = Vec::new();
    for (x, y) in (0..height).flat_map(|y| (0..width).map(move |x| (x, y))) {
        let Some(color) = picture.pixel(x, y) else {
            continue;
        };
        if characters.at(x, y) {
            text.add(color);
        } else if drawn.at(x, y) {
            left_out.push(color);
        } else {
            background.add(color);
        }
    }
    let (Some(text_color), Some(background_color)) = (text.color(), background.color()) else {
        return false;
    };
    let toward_text = offset(text_color, background_color);
    let hidden = left_out
        .into_iter()
        .filter(|&color| {
            let along: i64 = offset(channels(color), background_color)
                .into_iter()
                .zip(toward_text)
                .map(|(from_background, toward)| from_background.saturating_mul(toward))
                .sum();
            along < 0
        })
        .count();
    u64::try_from(hidden)
        .unwrap_or(u64::MAX)
        .saturating_mul(CHARACTER_PIXELS_PER_HIDDEN)
        >= text.count
}

/// The mean color of the pixels added to it.
#[derive(Default)]
struct Mean {
    sums: [u64; 3],
    count: u64,
}

impl Mean {
    fn add(&mut self, color: Rgb) {
        for (sum, channel) in self.sums.iter_mut().zip(channels(color)) {
            *sum = sum.saturating_add(channel.unsigned_abs());
        }
        self.count = self.count.saturating_add(1);
    }

    /// The mean's red, green, and blue, or `None` without a pixel.
    fn color(&self) -> Option<[i64; 3]> {
        let [red, green, blue] = self.sums.map(|sum| {
            sum.checked_div(self.count)
                .and_then(|mean| i64::try_from(mean).ok())
        });
        Some([red?, green?, blue?])
    }
}

/// The red, green, and blue of `color`.
fn channels(color: Rgb) -> [i64; 3] {
    [color.red, color.green, color.blue].map(i64::from)
}

/// How far `color` is from `from` in each of red, green, and blue.
fn offset(color: [i64; 3], from: [i64; 3]) -> [i64; 3] {
    let [red, green, blue] = color;
    let [from_red, from_green, from_blue] = from;
    [
        red.saturating_sub(from_red),
        green.saturating_sub(from_green),
        blue.saturating_sub(from_blue),
    ]
}

#[cfg(test)]
mod tests {
    use std::iter::once;

    use super::*;

    /// A row of gray pixels, three rows tall, on black.
    fn strip(grays: &[u8]) -> Image {
        let width = grays.len().checked_add(2).expect("a short strip");
        let blank = vec![0_u8; width];
        let middle: Vec<u8> = once(&0).chain(grays).chain(once(&0)).copied().collect();
        let bgra = [&blank, &middle, &middle, &middle, &blank]
            .into_iter()
            .flatten()
            .flat_map(|&gray| [gray, gray, gray, 0])
            .collect();
        let size = Size {
            width: u32::try_from(width).expect("a short strip"),
            height: 5,
        };
        Image::new(size, bgra).expect("matching length")
    }

    /// Two strokes of 200 with a shadow of 60 between them: two
    /// characters at the tolerances 60 to 199.
    fn shadowed() -> Image {
        strip(&[200, 60, 200])
    }

    #[test]
    fn the_tolerance_stays_while_every_sample_reads_with_it() {
        assert_eq!(fitted(&[], 40), 40);
        assert_eq!(fitted(&[(shadowed(), "11")], 60), 60);
        assert_eq!(fitted(&[(shadowed(), "11")], 199), 199);
        assert_eq!(fitted(&[(shadowed(), "1")], 59), 59);
    }

    #[test]
    fn a_tolerance_that_fails_a_sample_moves_to_the_middle_of_those_that_work() {
        // 60 to 196 work, 35 tolerances: the 18th is 128.
        assert_eq!(fitted(&[(shadowed(), "11")], 40), 128);
        assert_eq!(fitted(&[(shadowed(), "11")], 200), 128);
        // 0 to 56 work.
        assert_eq!(fitted(&[(shadowed(), "1")], 60), 28);
        // 0 and 4 work, and of two in the middle it is the lower.
        assert_eq!(fitted(&[(strip(&[8, 0, 8]), "11")], 20), 0);
    }

    #[test]
    fn every_sample_has_to_read() {
        // The second sample's shadow is 120, so only 120 to 196 work.
        let samples = [(shadowed(), "11"), (strip(&[200, 120, 200]), "11")];
        assert_eq!(fitted(&samples, 40), 156);
    }

    #[test]
    fn a_sample_no_tolerance_reads_leaves_the_others_their_tolerance() {
        // A sample labeled as another character than the shape it shares
        // with an earlier one fails whatever the tolerance.
        let samples = [(shadowed(), "11"), (shadowed(), "12")];
        assert_eq!(fitted(&samples, 100), 100, "the first reads as it is");
        assert_eq!(fitted(&samples, 40), 128, "the first needs another");
        // It fails first too, and the one after it is learned without it.
        let samples = [(shadowed(), "1"), (strip(&[200, 120, 200]), "11")];
        assert_eq!(fitted(&samples[..1], 156), 28);
        assert_eq!(fitted(&[(shadowed(), "111"), samples[1].clone()], 40), 156);
        // No sample reads at any tolerance.
        assert_eq!(fitted(&[(shadowed(), "111")], 40), 40);
    }

    #[test]
    fn the_longest_run_of_tolerances_wins_and_the_lower_of_two_as_long() {
        // A speck of 120 beside two strokes that a shadow of 60 joins: two
        // characters at 0 to 59, where the joined strokes are one, and at
        // 120 to 199, where the speck is gone.
        let speck = strip(&[200, 60, 200, 0, 120]);
        assert_eq!(fitted(&[(speck, "11")], 80), 156);
        // Two characters at 0 to 31 and at 96 to 127: eight tolerances
        // each.
        let twice = strip(&[127, 32, 127, 0, 95]);
        assert_eq!(fitted(&[(twice, "11")], 64), 12);
    }

    /// A picture of `rows`, whose `.` is `background`, `o` black, `#`
    /// white, and `+` halfway from the background to white, as an
    /// anti-aliased edge is.
    fn painted(rows: &[&str], background: [u8; 3]) -> Image {
        let halfway = background.map(|channel| channel.midpoint(u8::MAX));
        let bgra = rows
            .iter()
            .flat_map(|row| row.chars())
            .flat_map(|pixel| {
                let [red, green, blue] = match pixel {
                    'o' => [0; 3],
                    '#' => [u8::MAX; 3],
                    '+' => halfway,
                    _ => background,
                };
                [blue, green, red, 0]
            })
            .collect();
        let size = Size {
            width: u32::try_from(rows[0].len()).expect("a small picture"),
            height: u32::try_from(rows.len()).expect("a small picture"),
        };
        Image::new(size, bgra).expect("matching length")
    }

    /// Two white characters whose black outlines touch.
    const OUTLINED: [&str; 8] = [
        "............",
        ".oooooooooo.",
        ".o##oooo##o.",
        ".o##oooo##o.",
        ".o##oooo##o.",
        ".o##oooo##o.",
        ".oooooooooo.",
        "............",
    ];

    const DARK: [u8; 3] = [10, 30, 90];
    const BRIGHT: [u8; 3] = [60, 120, 200];

    #[test]
    fn a_tolerance_that_hides_an_outline_reads_only_over_a_background_that_hides_it() {
        // On the dark background the outline is 90 from it and the
        // characters 245, so only tolerances from 90 up tell the two
        // characters apart.
        let samples = [(painted(&OUTLINED, DARK), "11")];
        let tolerance = fitted(&samples, 40);
        assert_eq!(tolerance, 168);
        let mut reader = DigitReader::new(InkRule::Contrast { tolerance });
        reader.learn(&samples[0].0, "11").expect("learns");
        assert!(reader.read(&painted(&OUTLINED, DARK), false).is_ok());
        // On the bright one the outline is 200 from the background, so it
        // is ink, and the two characters are one that matches nothing.
        assert!(reader.read(&painted(&OUTLINED, BRIGHT), false).is_err());
        assert!(hides_a_tone(&samples, tolerance));
    }

    #[test]
    fn only_a_tone_on_the_far_side_of_the_background_is_hidden() {
        // Where the outline is ink, nothing is left out.
        assert!(!hides_a_tone(&[(painted(&OUTLINED, BRIGHT), "1")], 40));
        // An anti-aliased edge is left out, and lies toward the characters.
        let edged = [
            ".........",
            ".+##+##+.",
            ".+##+##+.",
            ".+##+##+.",
            ".........",
        ];
        let samples = [(painted(&edged, DARK), "11")];
        assert_eq!(fitted(&samples, 40), 184);
        assert!(!hides_a_tone(&samples, 184));
        // A pixel or two of another tone is no outline.
        let speck = [".......", ".o##.#.", "..##.#.", "..##.#.", "......."];
        assert!(!hides_a_tone(&[(painted(&speck, DARK), "11")], 168));
        // Neither a low tolerance nor a picture without ink hides one.
        assert!(!hides_a_tone(&[(painted(&OUTLINED, DARK), "1")], DRAWN));
        assert!(!hides_a_tone(
            &[(painted(&["...", "...", "..."], DARK), "")],
            168
        ));
    }

    #[test]
    fn no_tolerance_that_works_leaves_it_as_it_is() {
        assert_eq!(fitted(&[(shadowed(), "111")], 40), 40);
    }
}
