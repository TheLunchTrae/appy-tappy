//! Choosing the tolerance a reference finds its characters by contrast
//! with, from the samples it learns from.
//!
//! A tolerance too low takes a shadow, a glow, or the blur between two
//! characters for ink, so characters touch; one too high leaves strokes
//! out, so a character falls apart or is gone. Either way a sample shows
//! another count of characters than its text has, and the reference cannot
//! learn it. So the samples say which tolerances work, and nobody has to
//! set one.

use appytappy_sources::screen::Image;
use appytappy_sources::screen::digits::{DigitReader, InkRule};

/// The gap between the tolerances tried. Finer steps cost more lessons
/// and find no tolerance that matters: a sample reads over a run of
/// tolerances far wider than this.
const STEP: u8 = 4;

/// The tolerance at which a reference that finds its characters by
/// contrast learns every one of `samples`, each a picture and the text it
/// shows, in order.
///
/// It is `current` while that works, so the tolerance changes only when a
/// sample needs it to, and what was read or recorded with it stays as it
/// was read. Otherwise it is the middle of the longest run of tolerances
/// that work, the lower of two runs as long, among every [`STEP`]th from
/// 0: as far as can be from both a tolerance too low and one too high.
/// With no tolerance that works, as while a box's text is half typed, it
/// stays `current`.
pub(crate) fn fitted(samples: &[(Image, &str)], current: u8) -> u8 {
    let learns_all = |tolerance: u8| {
        let mut reader = DigitReader::new(InkRule::Contrast { tolerance });
        samples
            .iter()
            .all(|(picture, text)| reader.learn(picture, text).is_ok())
    };
    if learns_all(current) {
        return current;
    }
    let mut longest: Option<(u8, u8)> = None;
    let mut run: Option<(u8, u8)> = None;
    for tolerance in (0..=u8::MAX).step_by(usize::from(STEP)) {
        if learns_all(tolerance) {
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

#[cfg(test)]
mod tests {
    use std::iter::once;

    use appytappy_sources::screen::Size;

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
        // A sample labeled as another character than the shape it shares
        // with an earlier one fails whatever the tolerance.
        assert_eq!(fitted(&[(shadowed(), "11"), (shadowed(), "12")], 40), 40);
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

    #[test]
    fn no_tolerance_that_works_leaves_it_as_it_is() {
        assert_eq!(fitted(&[(shadowed(), "111")], 40), 40);
    }
}
