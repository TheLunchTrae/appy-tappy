//! A reading's ink as the text reader takes it.

use appytappy_sources::screen::Image;
use appytappy_sources::screen::digits::DigitReader;

/// The ink of `image` as the text reader takes it, as `reader` tells ink
/// from background: one byte a pixel, black ink on white, cut to the line
/// of text with a margin of white around it, as text on a page has. Black
/// on white reads better than the application's own colors, which can be
/// any pair. The line is the rows the number reader takes,
/// [`DigitReader::line_rows`], so a stray pixel above or below the text
/// does not stretch it. `None` without ink.
pub(super) fn line(image: &Image, reader: &DigitReader) -> Option<Line> {
    let rows = reader.line_rows(image)?;
    let top = usize::try_from(rows.start).ok()?;
    let width = usize::try_from(image.size().width).ok()?;
    let inked: Vec<Vec<bool>> = image
        .pixels()
        .chunks_exact(width.max(1))
        .skip(top)
        .take(rows.len())
        .map(|row| row.iter().map(|&pixel| reader.is_ink(pixel)).collect())
        .collect();
    let columns = |row: &Vec<bool>| {
        let first = row.iter().position(|&ink| ink)?;
        let last = row.iter().rposition(|&ink| ink)?;
        Some((first, last.saturating_add(1)))
    };
    let left = inked
        .iter()
        .filter_map(columns)
        .map(|(first, _)| first)
        .min()?;
    let right = inked.iter().filter_map(columns).map(|(_, end)| end).max()?;
    // Half the line's height, at least two pixels.
    let margin = (inked.len() / 2).max(2);
    let line_width = right
        .saturating_sub(left)
        .saturating_add(margin.saturating_mul(2));
    let height = inked.len().saturating_add(margin.saturating_mul(2));
    let mut pixels = vec![u8::MAX; line_width.saturating_mul(height)];
    for (out, row) in pixels.chunks_exact_mut(line_width).skip(margin).zip(&inked) {
        let ink = row.iter().take(right).skip(left);
        for (pixel, _) in out
            .iter_mut()
            .skip(margin)
            .zip(ink)
            .filter(|(_, ink)| **ink)
        {
            *pixel = 0;
        }
    }
    Some(Line {
        width: u32::try_from(line_width).ok()?,
        height: u32::try_from(height).ok()?,
        pixels,
    })
}

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

#[cfg(test)]
mod tests {
    use appytappy_sources::screen::Size;
    use appytappy_sources::system::Rgb;

    use super::*;

    /// A 6 by 7 image, white where `ink` says so and black elsewhere.
    fn image(ink: impl Fn(usize, usize) -> bool) -> Image {
        let pixels = (0..7)
            .flat_map(|y| (0..6).map(move |x| (x, y)))
            .flat_map(|(x, y)| if ink(x, y) { [255; 4] } else { [0, 0, 0, 255] })
            .collect();
        Image::new(
            Size {
                width: 6,
                height: 7,
            },
            pixels,
        )
        .expect("matching length")
    }

    fn reader() -> DigitReader {
        DigitReader::new(
            Rgb {
                red: 255,
                green: 255,
                blue: 255,
            },
            0,
        )
    }

    #[test]
    fn the_line_is_the_text_s_rows_in_black_on_white_with_a_margin() {
        // Text at columns 2 and 3 of rows 3 and 4, with a gap in its second
        // row, and a stray pixel on row 0 further left.
        let text = |x: usize, y: usize| (y == 3 && (2..4).contains(&x)) || (y == 4 && x == 3);
        let line =
            line(&image(|x, y| text(x, y) || (x, y) == (0, 0)), &reader()).expect("it shows ink");
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
    fn a_tall_line_has_a_margin_of_half_its_height() {
        let line = line(&image(|x, y| x == 1 && y < 6), &reader()).expect("it shows ink");
        assert_eq!((line.width, line.height), (7, 12));
    }

    #[test]
    fn an_image_without_ink_has_no_line() {
        assert_eq!(line(&image(|_, _| false), &reader()), None);
    }
}
