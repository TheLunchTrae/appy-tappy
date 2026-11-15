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

/// How far past the colors of a sample's background and characters a
/// pixel's red, green, or blue must be for [`hides_a_tone`] to take it for
/// another tone: further than the grain of a flat color.
const GRAIN: u8 = 16;

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
/// A sample hides a tone when, for each [`CHARACTER_PIXELS_PER_HIDDEN`]
/// pixels of its characters, it has a pixel inside its border that is not
/// ink and whose red, green, or blue is more than [`GRAIN`] outside what
/// the border's pixels and the characters' span between them. The border is
/// the background, whatever colors it has, so a gradient, grain, or a
/// panel's line through the sample is no tone of the text. A character's
/// anti-aliased edge blends the background and the character, so each of
/// its channels lies between theirs.
///
/// An outline or a shadow whose every channel lies between the
/// background's and the characters' looks like such an edge, as blue
/// around white on gray does, so this finds none there. It finds none that
/// reaches the border either, since it is then among the background's
/// colors.
pub(crate) fn hides_a_tone(samples: &[(Image, &str)], tolerance: u8) -> bool {
    samples
        .iter()
        .any(|(picture, _text)| hidden_tone(picture, tolerance))
}

/// [`hides_a_tone`] for one picture.
fn hidden_tone(picture: &Image, tolerance: u8) -> bool {
    let ink = DigitReader::new(InkRule::Contrast { tolerance }).ink(picture);
    let Size { width, height } = picture.size();
    let mut expected = Span::default();
    let mut character_pixels = 0_u64;
    // The pixels inside the border that the characters leave out.
    let mut left_out = Vec::new();
    for (x, y) in (0..height).flat_map(|y| (0..width).map(move |x| (x, y))) {
        let Some(color) = picture.pixel(x, y) else {
            continue;
        };
        let on_border =
            x == 0 || y == 0 || x.saturating_add(1) == width || y.saturating_add(1) == height;
        if ink.at(x, y) {
            expected.add(color);
            character_pixels = character_pixels.saturating_add(1);
        } else if on_border {
            expected.add(color);
        } else {
            left_out.push(color);
        }
    }
    let hidden = left_out
        .into_iter()
        .filter(|&color| !expected.holds(color))
        .count();
    character_pixels > 0
        && u64::try_from(hidden)
            .unwrap_or(u64::MAX)
            .saturating_mul(CHARACTER_PIXELS_PER_HIDDEN)
            >= character_pixels
}

/// The lowest and the highest red, green, and blue among the colors added
/// to it.
struct Span {
    lowest: [u8; 3],
    highest: [u8; 3],
}

impl Default for Span {
    fn default() -> Self {
        Self {
            lowest: [u8::MAX; 3],
            highest: [0; 3],
        }
    }
}

impl Span {
    fn add(&mut self, color: Rgb) {
        for ((lowest, highest), channel) in self
            .lowest
            .iter_mut()
            .zip(&mut self.highest)
            .zip(channels(color))
        {
            *lowest = (*lowest).min(channel);
            *highest = (*highest).max(channel);
        }
    }

    /// Whether each of `color`'s red, green, and blue is within [`GRAIN`]
    /// of the span of that channel. An empty span holds no color.
    fn holds(&self, color: Rgb) -> bool {
        self.lowest
            .into_iter()
            .zip(self.highest)
            .zip(channels(color))
            .all(|((lowest, highest), channel)| {
                (lowest.saturating_sub(GRAIN)..=highest.saturating_add(GRAIN)).contains(&channel)
            })
    }
}

/// The red, green, and blue of `color`.
fn channels(color: Rgb) -> [u8; 3] {
    [color.red, color.green, color.blue]
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

    /// A picture of `rows`, each pixel the color `color` gives its
    /// character, column, and row.
    fn picture(rows: &[&str], color: impl Fn(char, u32, u32) -> [u8; 3]) -> Image {
        let bgra = (0_u32..)
            .zip(rows)
            .flat_map(|(y, row)| {
                (0_u32..)
                    .zip(row.chars())
                    .map(move |(x, pixel)| (pixel, x, y))
            })
            .flat_map(|(pixel, x, y)| {
                let [red, green, blue] = color(pixel, x, y);
                [blue, green, red, 0]
            })
            .collect();
        let size = Size {
            width: u32::try_from(rows[0].len()).expect("a small picture"),
            height: u32::try_from(rows.len()).expect("a small picture"),
        };
        Image::new(size, bgra).expect("matching length")
    }

    /// A picture of `rows`, whose `.` is `background`, `o` `outline`, `#`
    /// white, and `+` halfway from the background to white, as an
    /// anti-aliased edge is.
    fn outlined(rows: &[&str], background: [u8; 3], outline: [u8; 3]) -> Image {
        let halfway = background.map(|channel| channel.midpoint(u8::MAX));
        picture(rows, |pixel, _, _| match pixel {
            'o' => outline,
            '#' => [u8::MAX; 3],
            '+' => halfway,
            _ => background,
        })
    }

    /// [`outlined`] in black.
    fn painted(rows: &[&str], background: [u8; 3]) -> Image {
        outlined(rows, background, [0; 3])
    }

    /// Whether the pixel of `picture` at `x` and `y` is ink at `tolerance`.
    fn inked(picture: &Image, tolerance: u8, x: u32, y: u32) -> bool {
        DigitReader::new(InkRule::Contrast { tolerance })
            .ink(picture)
            .at(x, y)
    }

    /// Two white characters whose outlines touch.
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

    /// The color of a background at a column and a row.
    type ColorAt = dyn Fn(u32, u32) -> [u8; 3];

    /// Two white characters with nothing around them.
    const PLAIN: [&str; 8] = [
        "............",
        "............",
        "..##....##..",
        "..##....##..",
        "..##....##..",
        "..##....##..",
        "............",
        "............",
    ];

    const DARK: [u8; 3] = [10, 30, 90];
    const BRIGHT: [u8; 3] = [60, 120, 200];

    #[test]
    fn a_tolerance_that_hides_an_outline_reads_only_over_a_background_that_hides_it() {
        // On the dark background the outline is 90 from it and the
        // characters 245, so only tolerances from 90 up tell the two
        // characters apart, and they leave the outline out.
        let samples = [(painted(&OUTLINED, DARK), "11")];
        let tolerance = fitted(&samples, 40);
        assert!(!inked(&samples[0].0, tolerance, 1, 1));
        let mut reader = DigitReader::new(InkRule::Contrast { tolerance });
        reader.learn(&samples[0].0, "11").expect("learns");
        assert!(reader.read(&painted(&OUTLINED, DARK), false).is_ok());
        // On the bright one the outline is 200 from the background, so it
        // is ink, and the two characters are one that matches nothing.
        assert!(reader.read(&painted(&OUTLINED, BRIGHT), false).is_err());
        assert!(hides_a_tone(&samples, tolerance));
    }

    #[test]
    fn an_outline_of_a_color_beside_the_background_and_the_text_is_hidden() {
        // A red outline around white on teal is brighter than the
        // background in red and darker in green and blue, so it is not
        // behind the background as seen from the text.
        let samples = [(outlined(&OUTLINED, [0, 80, 80], [200, 0, 0]), "11")];
        let tolerance = fitted(&samples, 40);
        assert!(!inked(&samples[0].0, tolerance, 1, 1));
        assert!(hides_a_tone(&samples, tolerance));
    }

    #[test]
    fn a_background_of_more_than_one_color_is_no_hidden_tone() {
        // Each background runs 60 or more from end to end, so much of it
        // is further from its median than a plain background's grain, and
        // darker than it.
        let across = |x: u32, _: u32| [10, 30, u8::try_from(x).map_or(u8::MAX, |x| x * 8 + 60)];
        let down = |_: u32, y: u32| [10, 30, u8::try_from(y).map_or(u8::MAX, |y| y * 9 + 60)];
        // No pattern a row or a column shares.
        let noisy = |x: u32, y: u32| {
            let grain = u8::try_from((x * 7 + y * 13 + x * y * 5) % 61).unwrap_or(0);
            [10, 30, 60 + grain]
        };
        // A panel's darker line through the text's rows.
        let lined = |_: u32, y: u32| if y == 4 { [0, 10, 40] } else { DARK };
        // A color of its own on each of three sides, corners aside, as
        // where the sample meets a panel's edges: only that side of the
        // border has it.
        let framed = |x: u32, y: u32| match (x, y) {
            (_, 0) => [100, 100, 40],
            (_, 7) => [100; 3],
            (0, _) => [40, 100, 100],
            (11, _) => [100, 40, 100],
            _ => [100; 3],
        };
        let backgrounds: [(&str, &ColorAt); 5] = [
            ("across", &across),
            ("down", &down),
            ("noisy", &noisy),
            ("lined", &lined),
            ("framed", &framed),
        ];
        for (name, background) in backgrounds {
            let sample = picture(&PLAIN, |pixel, x, y| match pixel {
                '#' => [u8::MAX; 3],
                _ => background(x, y),
            });
            let samples = [(sample, "11")];
            let tolerance = fitted(&samples, 40);
            assert!(inked(&samples[0].0, tolerance, 2, 2), "{name}");
            assert!(!hides_a_tone(&samples, tolerance), "{name}");
        }
    }

    #[test]
    fn only_a_tone_beside_the_background_and_the_text_is_hidden() {
        // Where the outline is ink, nothing is left out.
        assert!(!hides_a_tone(&[(painted(&OUTLINED, BRIGHT), "1")], 40));
        // An anti-aliased edge is left out, 122 from the background, and
        // lies between it and the characters.
        let edged = [
            ".........",
            ".+##+##+.",
            ".+##+##+.",
            ".+##+##+.",
            ".........",
        ];
        let samples = [(painted(&edged, DARK), "11")];
        assert!(!inked(&samples[0].0, 150, 4, 2));
        assert!(!hides_a_tone(&samples, 150));
        // A pixel or two of another tone is no outline.
        let speck = [".......", ".o##.#.", "..##.#.", "..##.#.", "......."];
        assert!(!hides_a_tone(&[(painted(&speck, DARK), "11")], 168));
        // One pixel of it for each four of the characters is one.
        let shadow = [".......", ".o##.#.", ".o##.#.", "..##...", "......."];
        assert!(hides_a_tone(&[(painted(&shadow, DARK), "11")], 168));
        // A picture without ink hides none.
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
