//! The pixels a capture hands over: sizes, regions of a window, the cropped
//! image itself, the buffer a capture fills, and their errors.

use std::error::Error;
use std::fmt;
use std::io::{self, Cursor};
use std::num::NonZeroU32;

use appytappy_config::app_file::{ColorDef, RegionDef, SizeDef};

use super::mask::{self, Mask, MaskError};

/// The widest or tallest picture AppyTappy reads or writes, in pixels.
const MAX_PNG_SIDE: u32 = 8192;

/// The most pixels a picture AppyTappy reads or writes may hold: an 8K
/// display's worth.
///
/// Every picture AppyTappy saves is a window's content or a part of it. A
/// window can span displays and grow past this, but AppyTappy chooses this
/// bound, a whole 8K display, so that what one picture costs, about 10 bytes
/// a pixel while it decodes, stays bounded wherever the file came from.
pub const LARGEST_PICTURE: Size = Size {
    width: 7680,
    height: 4320,
};

/// The most memory the PNG decoder may allocate beside the picture's own
/// pixels, which [`LARGEST_PICTURE`] bounds: a row at a time and the file's
/// metadata, such as a color profile, which a picture AppyTappy writes does
/// not have.
const MAX_PNG_BYTES: usize = 4 << 20;

/// Bytes per pixel in an [`Image`]: blue, green, red, and an unused byte.
const BYTES_PER_PIXEL: usize = 4;

/// The least alpha a PNG pixel needs to be kept by its mask: half opaque.
const HALF_OPAQUE: u8 = 128;

/// A width and a height in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    /// The width.
    pub width: u32,
    /// The height.
    pub height: u32,
}

impl Size {
    /// The number of pixels.
    #[must_use]
    pub const fn pixels(self) -> u64 {
        (self.width as u64).saturating_mul(self.height as u64)
    }

    /// Whether AppyTappy reads and writes a picture of this size: no wider
    /// or taller than 8192 pixels, and with no more pixels than
    /// [`LARGEST_PICTURE`]. A larger one is [`ImageError::TooLarge`].
    const fn fits_picture(self) -> Result<(), ImageError> {
        if self.width > MAX_PNG_SIDE
            || self.height > MAX_PNG_SIDE
            || self.pixels() > LARGEST_PICTURE.pixels()
        {
            return Err(ImageError::TooLarge { size: self });
        }
        Ok(())
    }

    /// The number of bytes an image of this size holds, if it fits in memory.
    fn bytes(self) -> Option<usize> {
        usize::try_from(self.width)
            .ok()?
            .checked_mul(usize::try_from(self.height).ok()?)?
            .checked_mul(BYTES_PER_PIXEL)
    }
}

impl fmt::Display for Size {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}x{}", self.width, self.height)
    }
}

impl From<SizeDef> for Size {
    fn from(SizeDef { width, height }: SizeDef) -> Self {
        Self { width, height }
    }
}

impl From<Size> for SizeDef {
    fn from(Size { width, height }: Size) -> Self {
        Self { width, height }
    }
}

/// A pixel's color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgb {
    /// Red.
    pub red: u8,
    /// Green.
    pub green: u8,
    /// Blue.
    pub blue: u8,
}

impl Rgb {
    /// The color of `pixel`, as `[blue, green, red, unused]` like
    /// [`Image::pixels`] holds it.
    #[must_use]
    pub const fn of_pixel([blue, green, red, _]: [u8; 4]) -> Self {
        Self { red, green, blue }
    }

    /// Whether each of this color's red, green, and blue is within
    /// `tolerance` of `other`'s. Every check that asks whether two colors
    /// are the same asks it here: a number's ink, a color check, and a
    /// selection's spread.
    #[must_use]
    pub fn near(self, other: Self, tolerance: u8) -> bool {
        self.farthest_channel(other) <= tolerance
    }

    /// How far this color is from `other`: the largest difference among
    /// their red, green, and blue.
    #[must_use]
    pub fn farthest_channel(self, other: Self) -> u8 {
        self.red
            .abs_diff(other.red)
            .max(self.green.abs_diff(other.green))
            .max(self.blue.abs_diff(other.blue))
    }
}

impl From<ColorDef> for Rgb {
    fn from(ColorDef { red, green, blue }: ColorDef) -> Self {
        Self { red, green, blue }
    }
}

impl From<Rgb> for ColorDef {
    fn from(Rgb { red, green, blue }: Rgb) -> Self {
        Self { red, green, blue }
    }
}

/// A non-empty rectangle of a frame, in pixels from the frame's top-left
/// corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    x: u32,
    y: u32,
    width: NonZeroU32,
    height: NonZeroU32,
}

impl Region {
    /// The rectangle at (`x`, `y`) of `width` by `height` pixels, or `None`
    /// when it is empty or its far edge passes `u32::MAX`.
    #[must_use]
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Option<Self> {
        x.checked_add(width)?;
        y.checked_add(height)?;
        Some(Self {
            x,
            y,
            width: NonZeroU32::new(width)?,
            height: NonZeroU32::new(height)?,
        })
    }

    /// The whole of a frame of `size`, or `None` when it is empty.
    #[must_use]
    pub fn whole(size: Size) -> Option<Self> {
        Self::new(0, 0, size.width, size.height)
    }

    /// The left edge.
    #[must_use]
    pub const fn x(self) -> u32 {
        self.x
    }

    /// The top edge.
    #[must_use]
    pub const fn y(self) -> u32 {
        self.y
    }

    /// The width and height.
    #[must_use]
    pub const fn size(self) -> Size {
        Size {
            width: self.width.get(),
            height: self.height.get(),
        }
    }

    /// The right edge, exclusive. [`Region::new`] checked that it fits.
    #[must_use]
    pub const fn right(self) -> u32 {
        self.x.saturating_add(self.width.get())
    }

    /// The bottom edge, exclusive. [`Region::new`] checked that it fits.
    #[must_use]
    pub const fn bottom(self) -> u32 {
        self.y.saturating_add(self.height.get())
    }

    /// Whether the region lies inside a frame of `size`.
    #[must_use]
    pub const fn fits(self, size: Size) -> bool {
        self.right() <= size.width && self.bottom() <= size.height
    }
}

/// A saved region was empty, or its far edge passed `u32::MAX`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmptyRegion;

impl fmt::Display for EmptyRegion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a region needs a width and a height that end inside a frame")
    }
}

impl Error for EmptyRegion {}

impl TryFrom<RegionDef> for Region {
    type Error = EmptyRegion;

    fn try_from(region: RegionDef) -> Result<Self, EmptyRegion> {
        Self::new(region.x, region.y, region.width, region.height).ok_or(EmptyRegion)
    }
}

impl From<Region> for RegionDef {
    fn from(region: Region) -> Self {
        let Size { width, height } = region.size();
        Self {
            x: region.x,
            y: region.y,
            width,
            height,
        }
    }
}

/// Pixels in rows from the top, each pixel stored as blue, green, red, and an
/// unused byte: the layout Windows capture produces, so a frame's rows copy
/// in without conversion.
///
/// An image is never empty.
#[derive(Clone, PartialEq, Eq)]
pub struct Image {
    size: Size,
    bgra: Vec<u8>,
}

impl fmt::Debug for Image {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Image")
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

impl Image {
    /// An image of `size` from its `bgra` bytes.
    ///
    /// # Errors
    ///
    /// Returns [`ImageError::Empty`] for a zero width or height and
    /// [`ImageError::Length`] when `bgra` does not hold exactly four bytes per
    /// pixel.
    pub fn new(size: Size, bgra: Vec<u8>) -> Result<Self, ImageError> {
        if size.width == 0 || size.height == 0 {
            return Err(ImageError::Empty);
        }
        if size.bytes() != Some(bgra.len()) {
            return Err(ImageError::Length { size });
        }
        Ok(Self { size, bgra })
    }

    /// The image's size.
    #[must_use]
    pub const fn size(&self) -> Size {
        self.size
    }

    /// The pixel at (`x`, `y`), or `None` outside the image.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> Option<Rgb> {
        if x >= self.size.width || y >= self.size.height {
            return None;
        }
        let index = usize::try_from(y)
            .ok()?
            .checked_mul(usize::try_from(self.size.width).ok()?)?
            .checked_add(usize::try_from(x).ok()?)?
            .checked_mul(BYTES_PER_PIXEL)?;
        match self.bgra.get(index..)? {
            [blue, green, red, ..] => Some(Rgb {
                red: *red,
                green: *green,
                blue: *blue,
            }),
            _ => None,
        }
    }

    /// The pixels as blue, green, red, and unused bytes, row by row.
    #[must_use]
    pub fn as_bgra(&self) -> &[u8] {
        &self.bgra
    }

    /// The pixels as `[blue, green, red, unused]`, row by row.
    #[must_use]
    pub fn pixels(&self) -> &[[u8; BYTES_PER_PIXEL]] {
        self.bgra.as_chunks().0
    }

    /// The rows of the image, from the top.
    pub(super) fn rows(&self) -> impl Iterator<Item = &[[u8; BYTES_PER_PIXEL]]> {
        let width = usize::try_from(self.size.width).unwrap_or(usize::MAX);
        self.pixels().chunks_exact(width.max(1))
    }

    /// A copy of `region`, or `None` when the region does not fit.
    #[must_use]
    pub fn crop(&self, region: Region) -> Option<Self> {
        if !region.fits(self.size) {
            return None;
        }
        let start = usize::try_from(region.x()).ok()?;
        let end = usize::try_from(region.right()).ok()?;
        let top = usize::try_from(region.y()).ok()?;
        let height = usize::try_from(region.size().height).ok()?;
        let mut bgra = Vec::with_capacity(region.size().bytes()?);
        for row in self.rows().skip(top).take(height) {
            bgra.extend(row.get(start..end)?.iter().flatten());
        }
        Some(Self {
            size: region.size(),
            bgra,
        })
    }

    /// The size a PNG file's header gives its picture, read without
    /// decoding the picture.
    ///
    /// # Errors
    ///
    /// Returns [`ImageError::Decode`] for a file without a PNG header, and
    /// [`ImageError::TooLarge`] for a picture larger than AppyTappy reads.
    pub fn png_size(bytes: &[u8]) -> Result<Size, ImageError> {
        header_size(&mut decoder(bytes))
    }

    /// Decodes a PNG file, such as a screen binding's reference image. Every
    /// PNG color type and bit depth reads as 8-bit color, and any alpha is
    /// dropped.
    ///
    /// # Errors
    ///
    /// Returns [`ImageError::Decode`] for a malformed file, and
    /// [`ImageError::TooLarge`] for a picture larger than AppyTappy reads,
    /// as its header says, before any of it is decoded.
    pub fn decode_png(bytes: &[u8]) -> Result<Self, ImageError> {
        Self::decode(bytes).map(|(image, _alpha)| image)
    }

    /// Decodes a PNG file as [`Image::decode_png`] does, with the pixels its
    /// alpha leaves out: a pixel less than half opaque is not kept. The mask
    /// is `None` when the file keeps every pixel.
    ///
    /// # Errors
    ///
    /// Returns the errors [`Image::decode_png`] does, and
    /// [`ImageError::Mask`] when no pixel is at least half opaque.
    pub fn decode_png_masked(bytes: &[u8]) -> Result<(Self, Option<Mask>), ImageError> {
        let (image, alpha) = Self::decode(bytes)?;
        let kept: Vec<bool> = alpha.iter().map(|&alpha| alpha >= HALF_OPAQUE).collect();
        if !kept.contains(&false) {
            return Ok((image, None));
        }
        let mask = Mask::new(image.size, kept).map_err(ImageError::Mask)?;
        Ok((image, Some(mask)))
    }

    /// Decodes a PNG file into an image and each pixel's alpha.
    fn decode(bytes: &[u8]) -> Result<(Self, Vec<u8>), ImageError> {
        let mut decoder = decoder(bytes);
        let size = header_size(&mut decoder)?;
        let mut reader = decoder.read_info().map_err(ImageError::Decode)?;
        let length = reader
            .output_buffer_size()
            .ok_or(ImageError::TooLarge { size })?;
        let mut scanlines = vec![0; length];
        let info = reader
            .next_frame(&mut scanlines)
            .map_err(ImageError::Decode)?;
        let channels = match info.color_type {
            png::ColorType::Grayscale => 1,
            png::ColorType::GrayscaleAlpha => 2,
            // The expand transformation turns palette images into RGB.
            png::ColorType::Rgb | png::ColorType::Indexed => 3,
            png::ColorType::Rgba => 4,
        };
        let mut bgra = Vec::with_capacity(size.bytes().ok_or(ImageError::TooLarge { size })?);
        let mut alpha = Vec::with_capacity(mask::pixels(size).unwrap_or_default());
        let width = usize::try_from(size.width)
            .ok()
            .ok_or(ImageError::TooLarge { size })?;
        let pixels_per_row = width.saturating_mul(channels);
        for row in scanlines.chunks_exact(info.line_size) {
            let row = row.get(..pixels_per_row).ok_or(ImageError::Unsupported)?;
            for pixel in row.chunks_exact(channels) {
                let (red, green, blue, opacity) = match *pixel {
                    [gray] => (gray, gray, gray, u8::MAX),
                    [gray, opacity] => (gray, gray, gray, opacity),
                    [red, green, blue] => (red, green, blue, u8::MAX),
                    [red, green, blue, opacity] => (red, green, blue, opacity),
                    _ => return Err(ImageError::Unsupported),
                };
                bgra.extend_from_slice(&[blue, green, red, u8::MAX]);
                alpha.push(opacity);
            }
        }
        Ok((Self::new(size, bgra)?, alpha))
    }

    /// Encodes the image as an 8-bit RGB PNG file.
    ///
    /// # Errors
    ///
    /// Returns [`ImageError::TooLarge`] for an image larger than AppyTappy
    /// reads back, and [`ImageError::Encode`] when the encoder fails.
    pub fn encode_png(&self) -> Result<Vec<u8>, ImageError> {
        self.size.fits_picture()?;
        let mut rgb = Vec::with_capacity(self.bgra.len());
        for &[blue, green, red, _] in self.pixels() {
            rgb.extend_from_slice(&[red, green, blue]);
        }
        self.encode(png::ColorType::Rgb, &rgb)
    }

    /// Encodes the image as an 8-bit RGBA PNG file whose pixels outside
    /// `mask` are transparent, which [`Image::decode_png_masked`] reads back
    /// as the same mask. A full mask writes an RGB file.
    ///
    /// # Errors
    ///
    /// Returns [`ImageError::Mask`] when the mask's size differs from the
    /// image's, and the errors [`Image::encode_png`] does.
    pub fn encode_png_masked(&self, mask: &Mask) -> Result<Vec<u8>, ImageError> {
        self.size.fits_picture()?;
        if mask.size() != self.size {
            return Err(ImageError::Mask(MaskError::SizeMismatch {
                image: self.size,
                mask: mask.size(),
            }));
        }
        if mask.is_full() {
            return self.encode_png();
        }
        let mut rgba = Vec::with_capacity(self.bgra.len());
        for (&[blue, green, red, _], &kept) in self.pixels().iter().zip(mask.kept()) {
            rgba.extend_from_slice(&[red, green, blue, if kept { u8::MAX } else { 0 }]);
        }
        self.encode(png::ColorType::Rgba, &rgba)
    }

    /// Writes `data`, rows of the image in `color` channels, as a PNG file.
    fn encode(&self, color: png::ColorType, data: &[u8]) -> Result<Vec<u8>, ImageError> {
        let mut file = Vec::new();
        let mut encoder = png::Encoder::new(&mut file, self.size.width, self.size.height);
        encoder.set_color(color);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(ImageError::Encode)?;
        writer.write_image_data(data).map_err(ImageError::Encode)?;
        writer.finish().map_err(ImageError::Encode)?;
        Ok(file)
    }
}

/// A decoder for the PNG file `bytes`, which reads every color type and bit
/// depth as 8-bit color.
fn decoder(bytes: &[u8]) -> png::Decoder<Cursor<&[u8]>> {
    let mut decoder = png::Decoder::new_with_limits(
        Cursor::new(bytes),
        png::Limits {
            bytes: MAX_PNG_BYTES,
        },
    );
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    decoder
}

/// The size `decoder`'s header gives its picture, once it fits.
fn header_size(decoder: &mut png::Decoder<Cursor<&[u8]>>) -> Result<Size, ImageError> {
    let header = decoder.read_header_info().map_err(ImageError::Decode)?;
    let size = Size {
        width: header.width,
        height: header.height,
    };
    size.fits_picture()?;
    Ok(size)
}

/// The reusable buffer a capture copies each frame's region into, so a
/// running capture allocates only when the region grows.
#[derive(Debug, Default)]
pub struct FrameBuffer {
    image: Option<Image>,
}

impl FrameBuffer {
    /// Returns the bytes to fill for a region of `size`, keeping the existing
    /// allocation when it is large enough.
    ///
    /// # Errors
    ///
    /// Returns [`ImageError::Empty`] for a zero width or height and
    /// [`ImageError::TooLarge`] when the size does not fit in memory.
    pub fn prepare(&mut self, size: Size) -> Result<&mut [u8], ImageError> {
        if size.width == 0 || size.height == 0 {
            return Err(ImageError::Empty);
        }
        let length = size.bytes().ok_or(ImageError::TooLarge { size })?;
        let image = self.image.get_or_insert_with(|| Image {
            size,
            bgra: Vec::new(),
        });
        image.size = size;
        image.bgra.resize(length, 0);
        Ok(&mut image.bgra)
    }

    /// Fills the buffer with a `size` image whose rows lie `pitch` bytes
    /// apart in `source`, as graphics APIs lay out a mapped texture: each row
    /// is `size.width * 4` bytes, followed by padding up to the pitch.
    ///
    /// # Errors
    ///
    /// Returns [`ImageError::Empty`] or [`ImageError::TooLarge`] as
    /// [`FrameBuffer::prepare`] does, and [`ImageError::Length`] when
    /// `pitch` is shorter than a row or `source` ends before the last row.
    pub fn fill_pitched(
        &mut self,
        size: Size,
        source: &[u8],
        pitch: usize,
    ) -> Result<(), ImageError> {
        let target = self.prepare(size)?;
        let row = usize::try_from(size.width)
            .ok()
            .and_then(|width| width.checked_mul(BYTES_PER_PIXEL))
            .ok_or(ImageError::TooLarge { size })?;
        if pitch < row {
            return Err(ImageError::Length { size });
        }
        for (index, line) in target.chunks_exact_mut(row).enumerate() {
            let start = index
                .checked_mul(pitch)
                .ok_or(ImageError::Length { size })?;
            let bytes = start
                .checked_add(row)
                .and_then(|end| source.get(start..end))
                .ok_or(ImageError::Length { size })?;
            line.copy_from_slice(bytes);
        }
        Ok(())
    }

    /// The image the last [`FrameBuffer::prepare`] sized, or `None` before
    /// the first one.
    #[must_use]
    pub const fn image(&self) -> Option<&Image> {
        self.image.as_ref()
    }
}

/// An image could not be built, decoded, or encoded.
#[derive(Debug)]
pub enum ImageError {
    /// The width or height was zero.
    Empty,
    /// The byte buffer did not hold four bytes for each pixel of this size.
    Length {
        /// The size the buffer should have matched.
        size: Size,
    },
    /// The image was larger than AppyTappy accepts.
    TooLarge {
        /// The image's size.
        size: Size,
    },
    /// The PNG file used a layout the decoder did not expand.
    Unsupported,
    /// The PNG file could not be decoded.
    Decode(png::DecodingError),
    /// The PNG encoder failed.
    Encode(png::EncodingError),
    /// The mask did not fit the image, or the file kept no pixel.
    Mask(MaskError),
}

impl fmt::Display for ImageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("an image needs a width and a height above zero"),
            Self::Length { size } => {
                write!(formatter, "a {size} image needs four bytes for each pixel")
            },
            Self::TooLarge { size } => write!(
                formatter,
                "a {size} picture is larger than AppyTappy reads, which is at most \
                 {MAX_PNG_SIDE} pixels on a side and no more pixels than {LARGEST_PICTURE}"
            ),
            Self::Unsupported => formatter.write_str("the PNG file uses an unsupported layout"),
            Self::Decode(_) => formatter.write_str("the PNG file could not be decoded"),
            Self::Encode(_) => formatter.write_str("the image could not be encoded as PNG"),
            Self::Mask(error) => error.fmt(formatter),
        }
    }
}

impl Error for ImageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Decode(error) => Some(error),
            Self::Encode(error) => Some(error),
            _ => None,
        }
    }
}

/// A capture's region does not fit the window, such as after it shrank.
///
/// The capture reports it inside an [`io::Error`] of kind
/// [`io::ErrorKind::InvalidInput`], so a reader can learn the window's new
/// size and capture a region that fits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutsideWindow {
    /// The size of the window's content.
    pub window_size: Size,
}

impl OutsideWindow {
    /// The window's size when `error` is this problem.
    #[must_use]
    pub fn in_error(error: &io::Error) -> Option<Size> {
        error
            .get_ref()
            .and_then(|inner| inner.downcast_ref::<Self>())
            .map(|outside| outside.window_size)
    }
}

impl fmt::Display for OutsideWindow {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "the region lies outside the {} window",
            self.window_size
        )
    }
}

impl Error for OutsideWindow {}

impl From<OutsideWindow> for io::Error {
    fn from(outside: OutsideWindow) -> Self {
        Self::new(io::ErrorKind::InvalidInput, outside)
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn image(width: u32, height: u32, pixels: &[[u8; 3]]) -> Image {
        let bgra = pixels
            .iter()
            .flat_map(|&[red, green, blue]| [blue, green, red, u8::MAX])
            .collect();
        Image::new(Size { width, height }, bgra).unwrap()
    }

    #[test]
    fn a_region_outside_the_window_carries_the_windows_size() {
        let window_size = Size {
            width: 640,
            height: 480,
        };
        let error = io::Error::from(OutsideWindow { window_size });
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(
            error.to_string(),
            "the region lies outside the 640x480 window"
        );
        assert_eq!(OutsideWindow::in_error(&error), Some(window_size));
        assert_eq!(OutsideWindow::in_error(&io::Error::other("other")), None);
        assert_eq!(
            OutsideWindow::in_error(&io::Error::from(io::ErrorKind::InvalidInput)),
            None
        );
    }

    #[test]
    fn a_color_is_as_far_as_its_farthest_channel() {
        let color = |red, green, blue| Rgb { red, green, blue };
        let red = color(200, 20, 20);
        assert_eq!(red.farthest_channel(color(197, 25, 20)), 5);
        assert_eq!(red.farthest_channel(color(200, 20, 13)), 7);
        assert_eq!(red.farthest_channel(color(209, 20, 20)), 9);
        // Exactly the tolerance is near; one more is not, on any channel.
        assert!(red.near(color(210, 10, 30), 10));
        assert!(!red.near(color(211, 20, 20), 10));
        assert!(!red.near(color(200, 9, 20), 10));
        assert!(!red.near(color(200, 20, 31), 10));
        assert_eq!(Rgb::of_pixel([3, 2, 1, 255]), color(1, 2, 3));
    }

    #[test]
    fn saved_types_convert_field_by_field() {
        let region = RegionDef {
            x: 1,
            y: 2,
            width: 3,
            height: 4,
        };
        assert_eq!(
            Region::try_from(region),
            Ok(Region::new(1, 2, 3, 4).unwrap())
        );
        assert_eq!(RegionDef::from(Region::new(1, 2, 3, 4).unwrap()), region);
        assert_eq!(
            Region::try_from(RegionDef { width: 0, ..region }),
            Err(EmptyRegion)
        );
        assert_eq!(
            EmptyRegion.to_string(),
            "a region needs a width and a height that end inside a frame"
        );
        let size = Size {
            width: 3,
            height: 4,
        };
        let saved = SizeDef {
            width: 3,
            height: 4,
        };
        assert_eq!(Size::from(saved), size);
        assert_eq!(SizeDef::from(size), saved);
        let color = Rgb {
            red: 1,
            green: 2,
            blue: 3,
        };
        let saved = ColorDef {
            red: 1,
            green: 2,
            blue: 3,
        };
        assert_eq!(Rgb::from(saved), color);
        assert_eq!(ColorDef::from(color), saved);
    }

    #[test]
    fn regions_are_non_empty_and_bounded() {
        assert_eq!(Region::new(0, 0, 0, 1), None);
        assert_eq!(Region::new(0, 0, 1, 0), None);
        assert_eq!(Region::new(u32::MAX, 0, 1, 1), None);
        assert_eq!(Region::new(0, u32::MAX, 1, 1), None);
        let region = Region::new(u32::MAX - 2, 4, 2, 3).unwrap();
        assert_eq!((region.right(), region.bottom()), (u32::MAX, 7));
    }

    #[test]
    fn a_region_fits_only_inside_the_frame() {
        let frame = Size {
            width: 10,
            height: 5,
        };
        assert!(Region::new(8, 3, 2, 2).unwrap().fits(frame));
        assert!(!Region::new(9, 3, 2, 2).unwrap().fits(frame));
        assert!(!Region::new(8, 4, 2, 2).unwrap().fits(frame));
        assert_eq!(Region::whole(frame), Region::new(0, 0, 10, 5));
    }

    #[test]
    fn images_hold_four_bytes_per_pixel() {
        let size = Size {
            width: 2,
            height: 1,
        };
        assert!(matches!(
            Image::new(size, vec![0; 7]),
            Err(ImageError::Length { .. })
        ));
        assert!(matches!(
            Image::new(
                Size {
                    width: 0,
                    height: 1
                },
                Vec::new()
            ),
            Err(ImageError::Empty)
        ));
        assert!(Image::new(size, vec![0; 8]).is_ok());
    }

    #[test]
    fn pixels_and_crops_address_rows_from_the_top_left() {
        let picture = image(
            3,
            2,
            &[
                [1, 2, 3],
                [4, 5, 6],
                [7, 8, 9],
                [10, 11, 12],
                [13, 14, 15],
                [16, 17, 18],
            ],
        );
        assert_eq!(
            picture.pixel(2, 1),
            Some(Rgb {
                red: 16,
                green: 17,
                blue: 18
            })
        );
        assert_eq!(picture.pixel(3, 0), None);
        assert_eq!(picture.pixel(0, 2), None);
        let cropped = picture.crop(Region::new(1, 1, 2, 1).unwrap()).unwrap();
        assert_eq!(cropped, image(2, 1, &[[13, 14, 15], [16, 17, 18]]));
        assert_eq!(picture.crop(Region::new(2, 0, 2, 1).unwrap()), None);
    }

    #[test]
    fn decodes_gray_and_alpha_pngs_to_opaque_pixels() {
        let mut file = Vec::new();
        let mut encoder = png::Encoder::new(&mut file, 2, 1);
        encoder.set_color(png::ColorType::GrayscaleAlpha);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[10, 0, 200, 255]).unwrap();
        writer.finish().unwrap();
        let decoded = Image::decode_png(&file).unwrap();
        assert_eq!(decoded.as_bgra(), [10, 10, 10, 255, 200, 200, 200, 255]);
        let (masked, mask) = Image::decode_png_masked(&file).unwrap();
        assert_eq!(masked, decoded);
        assert_eq!(mask.unwrap().kept(), [false, true]);
    }

    fn rgba_png(alpha: [u8; 3]) -> Vec<u8> {
        let mut file = Vec::new();
        let mut encoder = png::Encoder::new(&mut file, 3, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        let [first, second, third] = alpha;
        writer
            .write_image_data(&[1, 2, 3, first, 4, 5, 6, second, 7, 8, 9, third])
            .unwrap();
        writer.finish().unwrap();
        file
    }

    #[test]
    fn a_pixel_at_least_half_opaque_is_kept() {
        let (picture, mask) = Image::decode_png_masked(&rgba_png([127, 128, 255])).unwrap();
        assert_eq!(picture, image(3, 1, &[[1, 2, 3], [4, 5, 6], [7, 8, 9]]));
        assert_eq!(mask.unwrap().kept(), [false, true, true]);
        let (_, opaque) = Image::decode_png_masked(&rgba_png([128, 200, 255])).unwrap();
        assert_eq!(opaque, None);
        assert!(matches!(
            Image::decode_png_masked(&rgba_png([0, 10, 127])),
            Err(ImageError::Mask(MaskError::Empty))
        ));
        assert_eq!(Image::decode_png_masked(&gray_png(2, 2)).unwrap().1, None);
    }

    #[test]
    fn a_masked_png_is_transparent_outside_its_mask() {
        let picture = image(3, 1, &[[1, 2, 3], [4, 5, 6], [7, 8, 9]]);
        let mask = Mask::new(picture.size(), vec![true, false, true]).unwrap();
        let file = picture.encode_png_masked(&mask).unwrap();
        assert_eq!(
            Image::decode_png_masked(&file).unwrap(),
            (picture.clone(), Some(mask))
        );
        let full = Mask::new(picture.size(), vec![true; 3]).unwrap();
        assert_eq!(
            picture.encode_png_masked(&full).unwrap(),
            picture.encode_png().unwrap()
        );
        let other = Mask::new(
            Size {
                width: 1,
                height: 1,
            },
            vec![true],
        )
        .unwrap();
        assert!(matches!(
            picture.encode_png_masked(&other),
            Err(ImageError::Mask(MaskError::SizeMismatch { .. }))
        ));
    }

    fn gray_png(width: u32, height: u32) -> Vec<u8> {
        let mut file = Vec::new();
        let mut encoder = png::Encoder::new(&mut file, width, height);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        let length = usize::try_from(width.checked_mul(height).unwrap()).unwrap();
        writer.write_image_data(&vec![0; length]).unwrap();
        writer.finish().unwrap();
        file
    }

    /// A PNG file with a header that says its picture is `width` by
    /// `height` pixels, and no pixels.
    fn header(width: u32, height: u32) -> Vec<u8> {
        let mut file = Vec::new();
        // The writer ends the file as it drops, with no image data.
        png::Encoder::new(&mut file, width, height)
            .write_header()
            .unwrap();
        file
    }

    #[test]
    fn rejects_malformed_and_oversized_pngs() {
        assert!(matches!(
            Image::decode_png(b"not a png"),
            Err(ImageError::Decode(_))
        ));
        assert!(matches!(
            Image::png_size(b"not a png"),
            Err(ImageError::Decode(_))
        ));
        assert!(Image::decode_png(&gray_png(MAX_PNG_SIDE, 1)).is_ok());
        assert!(Image::decode_png(&gray_png(1, MAX_PNG_SIDE)).is_ok());
        let size = |width, height| Size { width, height };
        // 4050 by 8192 is exactly as many pixels as the largest picture.
        for fits in [
            LARGEST_PICTURE,
            size(MAX_PNG_SIDE, 1),
            size(1, MAX_PNG_SIDE),
            size(4050, MAX_PNG_SIDE),
        ] {
            let file = header(fits.width, fits.height);
            assert_eq!(Image::png_size(&file).unwrap(), fits);
            // It goes on to the pixels, which this file lacks.
            assert!(matches!(
                Image::decode_png(&file),
                Err(ImageError::Decode(_))
            ));
        }
        for over in [
            size(7681, 4320),
            size(7680, 4321),
            size(4051, MAX_PNG_SIDE),
            size(MAX_PNG_SIDE + 1, 1),
            size(1, MAX_PNG_SIDE + 1),
        ] {
            let file = header(over.width, over.height);
            assert!(matches!(
                Image::png_size(&file),
                Err(ImageError::TooLarge { size }) if size == over
            ));
            // Refused from its header, before the missing pixels are found.
            assert!(matches!(
                Image::decode_png(&file),
                Err(ImageError::TooLarge { size }) if size == over
            ));
            assert!(matches!(
                Image::decode_png_masked(&file),
                Err(ImageError::TooLarge { size }) if size == over
            ));
        }
    }

    #[test]
    fn a_picture_larger_than_appytappy_reads_is_not_written() {
        let size = Size {
            width: 7681,
            height: 4320,
        };
        let length = size.bytes().unwrap();
        // The refusal reads none of the pixels, so the zeroed memory is
        // never touched.
        let picture = Image::new(size, vec![0; length]).unwrap();
        assert!(matches!(
            picture.encode_png(),
            Err(ImageError::TooLarge { size: refused }) if refused == size
        ));
        let mut kept = vec![false; mask::pixels(size).unwrap()];
        kept[0] = true;
        let mask = Mask::new(size, kept).unwrap();
        assert!(matches!(
            picture.encode_png_masked(&mask),
            Err(ImageError::TooLarge { size: refused }) if refused == size
        ));
    }

    #[test]
    fn a_picture_carries_no_more_metadata_than_the_decoder_may_hold() {
        let with_text = |length: usize| {
            let mut file = Vec::new();
            let mut encoder = png::Encoder::new(&mut file, 1, 1);
            encoder
                .add_text_chunk("Comment".to_owned(), "a".repeat(length))
                .unwrap();
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[0]).unwrap();
            writer.finish().unwrap();
            file
        };
        // The decoder counts a chunk's bytes more than once as it reads
        // them, so a picture keeps somewhat less than the limit.
        assert!(Image::decode_png(&with_text(MAX_PNG_BYTES / 4)).is_ok());
        assert!(matches!(
            Image::decode_png(&with_text(MAX_PNG_BYTES)),
            Err(ImageError::Decode(png::DecodingError::LimitsExceeded))
        ));
    }

    #[test]
    fn decodes_palette_pngs() {
        let mut file = Vec::new();
        let mut encoder = png::Encoder::new(&mut file, 2, 1);
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_palette(vec![1, 2, 3, 4, 5, 6]);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[1, 0]).unwrap();
        writer.finish().unwrap();
        let decoded = Image::decode_png(&file).unwrap();
        assert_eq!(decoded, image(2, 1, &[[4, 5, 6], [1, 2, 3]]));
    }

    #[test]
    fn errors_explain_themselves_and_keep_their_source() {
        let size = Size {
            width: 3,
            height: 2,
        };
        assert_eq!(size.to_string(), "3x2");
        let decode = Image::decode_png(b"not a png").unwrap_err();
        assert!(decode.source().is_some());
        let encode = ImageError::Encode(io::Error::other("disk full").into());
        assert_eq!(encode.to_string(), "the image could not be encoded as PNG");
        assert!(encode.source().is_some());
        assert_eq!(
            format!("{:?}", image(3, 2, &[[0; 3]; 6])),
            "Image { size: Size { width: 3, height: 2 }, .. }"
        );
        for (error, message) in [
            (
                ImageError::Empty,
                "an image needs a width and a height above zero",
            ),
            (
                ImageError::Length { size },
                "a 3x2 image needs four bytes for each pixel",
            ),
            (
                ImageError::TooLarge { size },
                "a 3x2 picture is larger than AppyTappy reads, which is at most 8192 pixels on \
                 a side and no more pixels than 7680x4320",
            ),
            (
                ImageError::Unsupported,
                "the PNG file uses an unsupported layout",
            ),
            (decode, "the PNG file could not be decoded"),
        ] {
            assert_eq!(error.to_string(), message);
            if !matches!(error, ImageError::Decode(_)) {
                assert!(error.source().is_none());
            }
        }
    }

    #[test]
    fn a_frame_buffer_reuses_its_allocation() {
        let mut buffer = FrameBuffer::default();
        assert!(buffer.image().is_none());
        let large = Size {
            width: 4,
            height: 4,
        };
        buffer.prepare(large).unwrap().fill(7);
        let capacity = buffer.image().unwrap().bgra.capacity();
        let small = Size {
            width: 2,
            height: 1,
        };
        assert_eq!(buffer.prepare(small).unwrap().len(), 8);
        let image = buffer.image().unwrap();
        assert_eq!(image.size(), small);
        assert_eq!(image.bgra.capacity(), capacity);
        assert!(
            buffer
                .prepare(Size {
                    width: 0,
                    height: 1
                })
                .is_err()
        );
    }

    #[test]
    fn a_frame_buffer_copies_pitched_rows() {
        let mut buffer = FrameBuffer::default();
        let size = Size {
            width: 1,
            height: 2,
        };
        // Two four-byte rows, each padded to six bytes; the last row needs no
        // padding after it.
        let source = [1, 2, 3, 4, 0, 0, 5, 6, 7, 8];
        buffer.fill_pitched(size, &source, 6).unwrap();
        assert_eq!(buffer.image().unwrap().as_bgra(), [1, 2, 3, 4, 5, 6, 7, 8]);
        buffer.fill_pitched(size, &source[..8], 4).unwrap();
        assert_eq!(buffer.image().unwrap().as_bgra(), [1, 2, 3, 4, 0, 0, 5, 6]);
        assert!(matches!(
            buffer.fill_pitched(size, &source, 3),
            Err(ImageError::Length { .. })
        ));
        assert!(matches!(
            buffer.fill_pitched(size, &source[..9], 6),
            Err(ImageError::Length { .. })
        ));
    }

    /// The setup flow on two screenshots of one fixture layout: a progress bar
    /// at 37% and at 90%, a percent counter under it, and a panel that turns
    /// gold on the second. `digits.png` shows the counter's font.
    mod fixtures {
        use appytappy_config::signal::{Number, StateName, Value};

        use super::*;
        use crate::screen::check::{BarFill, Check, ColorMatch, ImageMatch, StateMatch};
        use crate::screen::digits::DigitReader;

        const WHITE: Rgb = Rgb {
            red: 255,
            green: 255,
            blue: 255,
        };

        fn screenshots() -> [Image; 2] {
            [
                include_bytes!("../../tests/fixtures/progress_37.png").as_slice(),
                include_bytes!("../../tests/fixtures/progress_90.png").as_slice(),
            ]
            .map(|file| Image::decode_png(file).unwrap())
        }

        fn read(check: &Check, screenshot: &Image, region: Region) -> Value {
            check.read(&screenshot.crop(region).unwrap()).unwrap()
        }

        #[test]
        fn bar_fill_reads_the_bar() {
            let [first, second] = screenshots();
            let bar = Region::new(59, 19, 202, 12).unwrap();
            // The user clicks a filled and an empty spot on the screenshot.
            let filled = first.pixel(70, 25).unwrap();
            let empty = first.pixel(250, 25).unwrap();
            let check = Check::BarFill(BarFill::new(filled, empty).unwrap());
            assert_eq!(
                read(&check, &first, bar),
                Value::Number(Number::new(37.0).unwrap())
            );
            assert_eq!(
                read(&check, &second, bar),
                Value::Number(Number::new(90.0).unwrap())
            );
        }

        #[test]
        fn digits_read_the_counter() {
            let [first, second] = screenshots();
            let glyphs =
                Image::decode_png(include_bytes!("../../tests/fixtures/digits.png")).unwrap();
            let mut reader = DigitReader::new(WHITE, 60);
            reader.learn(&glyphs, "0123456789.%").unwrap();
            let counter = Region::new(120, 38, 100, 28).unwrap();
            let check = Check::Number {
                reader: reader.clone(),
                whole: false,
            };
            assert_eq!(
                read(&check, &first, counter),
                Value::Number(Number::new(37.0).unwrap())
            );
            assert_eq!(
                read(&check, &second, counter),
                Value::Number(Number::new(90.5).unwrap())
            );
            let whole = Check::Number {
                reader,
                whole: true,
            };
            assert_eq!(
                read(&whole, &second, counter),
                Value::Number(Number::new(90.0).unwrap())
            );
        }

        #[test]
        fn color_and_state_see_the_panel_change() {
            let [first, second] = screenshots();
            let panel = Region::new(100, 110, 120, 50).unwrap();
            let gold = Check::Color(ColorMatch {
                color: second.pixel(105, 115).unwrap(),
                tolerance: 40,
            });
            let before = StateName::new("Before").unwrap();
            let unchanged = Check::State(StateMatch::new(vec![(
                before.clone(),
                ImageMatch::new(first.crop(panel).unwrap(), None, 8).unwrap(),
            )]));
            let strip = Region::new(100, 110, 120, 12).unwrap();
            assert_eq!(read(&gold, &first, strip), Value::Flag(false));
            assert_eq!(read(&gold, &second, strip), Value::Flag(true));
            assert_eq!(read(&unchanged, &first, panel), Value::State(before));
            assert_eq!(
                read(&unchanged, &second, panel),
                Value::State(StateName::none())
            );
        }
    }

    fn any_image() -> impl Strategy<Value = Image> {
        (1_u32..12, 1_u32..12).prop_flat_map(|(width, height)| {
            let pixels = usize::try_from(width.checked_mul(height).unwrap()).unwrap();
            prop::collection::vec(any::<[u8; 3]>(), pixels)
                .prop_map(move |pixels| image(width, height, &pixels))
        })
    }

    proptest! {
        #[test]
        fn png_round_trips(picture in any_image()) {
            let decoded = Image::decode_png(&picture.encode_png().unwrap()).unwrap();
            prop_assert_eq!(decoded, picture);
        }

        #[test]
        fn masked_png_round_trips(picture in any_image(), seed in any::<u64>()) {
            let count = picture.pixels().len();
            let mut kept: Vec<bool> = (0..count).map(|index| (seed >> (index % 64)) & 1 == 1).collect();
            if let Some(first) = kept.first_mut() {
                *first = true;
            }
            let mask = Mask::new(picture.size(), kept).unwrap();
            let file = picture.encode_png_masked(&mask).unwrap();
            let (decoded, read) = Image::decode_png_masked(&file).unwrap();
            prop_assert_eq!(decoded, picture);
            prop_assert_eq!(read.unwrap_or_else(|| mask.clone()), mask);
        }
    }
}
