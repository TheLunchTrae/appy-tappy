//! Frames and screenshots as the web UI draws them.
//!
//! The UI paints pixels into a canvas, which takes RGBA rows. A capture holds
//! BGRA rows at the window's full size, so the preview is scaled down first:
//! a full-size frame of a large window would cost tens of megabytes a second
//! to pass to the UI.

use appytappy_sources::screen::Image;

/// The bytes before the pixels: the width, then the height, as
/// little-endian `u32` values.
const HEADER_BYTES: usize = 8;

/// Encodes `image` for the UI, scaled down by whole-pixel sampling so it is
/// at most `max_width` pixels wide. The result is the width and the height as
/// little-endian `u32` values, then the RGBA rows with every alpha opaque.
///
/// A `max_width` of zero is treated as one, so the result always holds at
/// least one pixel.
#[must_use]
pub(crate) fn rgba(image: &Image, max_width: u32) -> Vec<u8> {
    let size = image.size();
    let width = size.width.min(max_width.max(1));
    // `width` is at most `size.width`, so the scaled height is at most
    // `size.height` and never below one.
    let height = u64::from(size.height)
        .saturating_mul(u64::from(width))
        .checked_div(u64::from(size.width))
        .and_then(|height| u32::try_from(height).ok())
        .unwrap_or(1)
        .max(1);
    let rows: Vec<&[[u8; 4]]> = usize::try_from(size.width)
        .ok()
        .map(|row| image.pixels().chunks_exact(row).collect())
        .unwrap_or_default();
    let columns: Vec<usize> = (0..width)
        .filter_map(|x| source(x, width, size.width))
        .collect();
    let mut bytes = Vec::with_capacity(
        HEADER_BYTES.saturating_add(
            columns
                .len()
                .saturating_mul(4)
                .saturating_mul(usize::try_from(height).unwrap_or(0)),
        ),
    );
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    for y in 0..height {
        let row = source(y, height, size.height).and_then(|index| rows.get(index));
        for &column in &columns {
            let [blue, green, red, _] = row
                .and_then(|row| row.get(column))
                .copied()
                .unwrap_or_default();
            bytes.extend_from_slice(&[red, green, blue, u8::MAX]);
        }
    }
    bytes
}

/// The source index that output index `index` of `scaled` samples, out of
/// `full`.
fn source(index: u32, scaled: u32, full: u32) -> Option<usize> {
    let source = u64::from(index)
        .checked_mul(u64::from(full))?
        .checked_div(u64::from(scaled))?;
    usize::try_from(source).ok()
}

#[cfg(test)]
mod tests {
    use appytappy_sources::screen::Size;

    use super::*;

    /// A `width` by `height` image whose pixel at (x, y) has blue `x`, green
    /// `y`, and red 200.
    fn gradient(width: u32, height: u32) -> Image {
        let mut bgra = Vec::new();
        for y in 0..height {
            for x in 0..width {
                bgra.extend_from_slice(&[
                    u8::try_from(x).expect("small"),
                    u8::try_from(y).expect("small"),
                    200,
                    0,
                ]);
            }
        }
        Image::new(Size { width, height }, bgra).expect("matching length")
    }

    fn header(bytes: &[u8]) -> (u32, u32) {
        let width = u32::from_le_bytes(bytes[0..4].try_into().expect("four bytes"));
        let height = u32::from_le_bytes(bytes[4..8].try_into().expect("four bytes"));
        (width, height)
    }

    #[test]
    fn a_small_image_keeps_its_size_and_turns_into_opaque_rgba() {
        let bytes = rgba(&gradient(2, 2), 100);
        assert_eq!(header(&bytes), (2, 2));
        assert_eq!(
            &bytes[8..],
            [
                200, 0, 0, 255, 200, 0, 1, 255, 200, 1, 0, 255, 200, 1, 1, 255
            ]
        );
    }

    #[test]
    fn a_wide_image_scales_down_keeping_its_aspect_ratio() {
        let bytes = rgba(&gradient(8, 4), 4);
        assert_eq!(header(&bytes), (4, 2));
        // Every second column and row: blue 0, 2, 4, 6 and green 0, 2.
        let blues: Vec<u8> = bytes[8..]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| pixel[2])
            .collect();
        let greens: Vec<u8> = bytes[8..]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| pixel[1])
            .collect();
        assert_eq!(blues, [0, 2, 4, 6, 0, 2, 4, 6]);
        assert_eq!(greens, [0, 0, 0, 0, 2, 2, 2, 2]);
    }

    #[test]
    fn a_thin_image_keeps_at_least_one_row_and_column() {
        let bytes = rgba(&gradient(100, 1), 0);
        assert_eq!(header(&bytes), (1, 1));
        assert_eq!(bytes.len(), 12);
    }
}
