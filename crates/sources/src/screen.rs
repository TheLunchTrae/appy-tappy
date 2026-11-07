//! The screen region source: one window's captured frames, a region of them,
//! and the checks that read a value from that region.
//!
//! Capture is the only part that needs the operating system, so it sits
//! behind the [`Capture`] trait. Everything else here is portable Rust over a
//! cropped [`Image`] and gives the same result on every platform.
//! `docs/screen.md` describes the source and its checks.

#![forbid(unsafe_code)]

pub mod check;
pub mod digits;
pub mod saved;

use std::convert::Infallible;
use std::error::Error;
use std::fmt;
use std::io::{self, Cursor};
use std::num::NonZeroU32;
use std::time::{Duration, Instant};

use crate::system::{Rgb, Unsupported};

/// The widest or tallest image a PNG may hold, in pixels.
const MAX_PNG_SIDE: u32 = 8192;

/// The most memory PNG decoding may allocate, in bytes.
const MAX_PNG_BYTES: usize = 256 << 20;

/// Bytes per pixel in an [`Image`]: blue, green, red, and an unused byte.
const BYTES_PER_PIXEL: usize = 4;

/// A width and a height in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    /// The width.
    pub width: u32,
    /// The height.
    pub height: u32,
}

impl Size {
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
    fn rows(&self) -> impl Iterator<Item = &[[u8; BYTES_PER_PIXEL]]> {
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

    /// Decodes a PNG file, such as a screen binding's reference image. Every
    /// PNG color type and bit depth reads as 8-bit color, and any alpha is
    /// dropped.
    ///
    /// # Errors
    ///
    /// Returns [`ImageError::Decode`] for a malformed file, and
    /// [`ImageError::TooLarge`] for an image wider or taller than 8192 pixels.
    pub fn decode_png(bytes: &[u8]) -> Result<Self, ImageError> {
        let mut decoder = png::Decoder::new_with_limits(
            Cursor::new(bytes),
            png::Limits {
                bytes: MAX_PNG_BYTES,
            },
        );
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder.read_info().map_err(ImageError::Decode)?;
        let header = reader.info();
        let size = Size {
            width: header.width,
            height: header.height,
        };
        if size.width > MAX_PNG_SIDE || size.height > MAX_PNG_SIDE {
            return Err(ImageError::TooLarge { size });
        }
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
        let width = usize::try_from(size.width)
            .ok()
            .ok_or(ImageError::TooLarge { size })?;
        let pixels_per_row = width.saturating_mul(channels);
        for row in scanlines.chunks_exact(info.line_size) {
            let row = row.get(..pixels_per_row).ok_or(ImageError::Unsupported)?;
            for pixel in row.chunks_exact(channels) {
                let (red, green, blue) = match *pixel {
                    [gray] | [gray, _] => (gray, gray, gray),
                    [red, green, blue] | [red, green, blue, _] => (red, green, blue),
                    _ => return Err(ImageError::Unsupported),
                };
                bgra.extend_from_slice(&[blue, green, red, u8::MAX]);
            }
        }
        Self::new(size, bgra)
    }

    /// Encodes the image as an 8-bit RGB PNG file.
    ///
    /// # Errors
    ///
    /// Returns [`ImageError::Encode`] when the encoder fails.
    pub fn encode_png(&self) -> Result<Vec<u8>, ImageError> {
        let mut rgb = Vec::with_capacity(self.bgra.len());
        for &[blue, green, red, _] in self.pixels() {
            rgb.extend_from_slice(&[red, green, blue]);
        }
        let mut file = Vec::new();
        let mut encoder = png::Encoder::new(&mut file, self.size.width, self.size.height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(ImageError::Encode)?;
        writer.write_image_data(&rgb).map_err(ImageError::Encode)?;
        writer.finish().map_err(ImageError::Encode)?;
        Ok(file)
    }
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
                "a {size} image is larger than {MAX_PNG_SIDE} pixels on a side"
            ),
            Self::Unsupported => formatter.write_str("the PNG file uses an unsupported layout"),
            Self::Decode(_) => formatter.write_str("the PNG file could not be decoded"),
            Self::Encode(_) => formatter.write_str("the image could not be encoded as PNG"),
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

/// What a capture copies out of each frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Crop {
    /// The whole frame, as the setup app's preview and screenshot need.
    Whole,
    /// One region, as a screen binding reads.
    Region(Region),
}

/// One captured frame's pixels, lent to the frame handler. The image is
/// reused for the next frame, so a handler copies what it keeps.
#[derive(Debug)]
pub struct Frame<'buffer> {
    /// The size of the window's whole frame, which a region's coordinates
    /// refer to.
    pub window_size: Size,
    /// The cropped pixels.
    pub image: &'buffer Image,
    /// When AppyTappy received the frame.
    pub received: Instant,
    /// How long the frame waited between the system composing it and
    /// AppyTappy receiving it.
    pub delay: Duration,
}

impl Frame<'_> {
    /// The time from the system composing the frame until now: the
    /// capture-to-decision latency when a handler calls it after deciding.
    #[must_use]
    pub fn latency(&self) -> Duration {
        self.delay.saturating_add(self.received.elapsed())
    }
}

/// Receives a capture's newest frame, or the error that stopped one frame.
///
/// It returns how long to wait before handling another, such as the wait a
/// [`crate::schedule::ReadSchedule`] gives, and runs on the capture's own
/// thread.
pub type FrameHandler = Box<dyn FnMut(io::Result<Frame<'_>>) -> Duration + Send>;

/// Captures one window's frames.
///
/// A capture is pushed frames as the window's content changes and never
/// polls: a window that does not change costs nothing. Frames that arrive
/// during the wait the handler returned are released without being copied;
/// when the wait ends, the newest frame since then is handled.
pub trait Capture {
    /// A running capture.
    type Session: CaptureSession;

    /// Starts capturing `window`, a handle from [`crate::system::System::windows`],
    /// and calls `handler` with `crop` of new frames. Where the system
    /// supports it, it also asks the system for at most one frame every
    /// `throttle`, which saves the system's own work as well as ours.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error when the capture cannot start.
    fn start(
        &self,
        window: u64,
        crop: Crop,
        throttle: Duration,
        handler: FrameHandler,
    ) -> io::Result<Self::Session>;
}

/// A running capture.
pub trait CaptureSession {
    /// Stops the capture and waits for its thread to finish.
    ///
    /// # Errors
    ///
    /// Returns an error when the capture's thread failed.
    fn close(self) -> io::Result<()>;
}

/// The session type of [`Unsupported`], which cannot be constructed.
#[derive(Debug)]
pub struct NoSession(Infallible);

impl CaptureSession for NoSession {
    // `NoSession` holds `Infallible`, so no value exists and this body never
    // runs. No test can call it, so mutating it cannot be caught.
    #[mutants::skip]
    fn close(self) -> io::Result<()> {
        match self.0 {}
    }
}

impl Capture for Unsupported {
    type Session = NoSession;

    fn start(
        &self,
        _window: u64,
        _crop: Crop,
        _throttle: Duration,
        _handler: FrameHandler,
    ) -> io::Result<NoSession> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "AppyTappy captures windows on Windows only",
        ))
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

    #[test]
    fn rejects_malformed_and_oversized_pngs() {
        assert!(matches!(
            Image::decode_png(b"not a png"),
            Err(ImageError::Decode(_))
        ));
        assert!(Image::decode_png(&gray_png(MAX_PNG_SIDE, 1)).is_ok());
        assert!(Image::decode_png(&gray_png(1, MAX_PNG_SIDE)).is_ok());
        for (width, height) in [(MAX_PNG_SIDE + 1, 1), (1, MAX_PNG_SIDE + 1)] {
            assert!(matches!(
                Image::decode_png(&gray_png(width, height)),
                Err(ImageError::TooLarge { .. })
            ));
        }
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
                "a 3x2 image is larger than 8192 pixels on a side",
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
    fn a_frame_s_latency_adds_its_delay_to_the_time_since_it_arrived() {
        let picture = image(1, 1, &[[0, 0, 0]]);
        let frame = Frame {
            window_size: picture.size(),
            image: &picture,
            received: Instant::now(),
            delay: Duration::from_secs(60),
        };
        let latency = frame.latency();
        assert!(latency >= Duration::from_secs(60), "{latency:?}");
        assert!(latency < Duration::from_secs(61), "{latency:?}");
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

    #[test]
    fn unsupported_platforms_cannot_capture() {
        let error = Unsupported
            .start(0, Crop::Whole, Duration::ZERO, Box::new(|_| Duration::ZERO))
            .expect_err("unsupported");
        assert_eq!(error.kind(), io::ErrorKind::Unsupported);
    }

    /// The setup flow on two screenshots of one fixture layout: a progress bar
    /// at 37% and at 90%, a percent counter under it, and a panel that turns
    /// gold on the second. `digits.png` shows the counter's font.
    mod fixtures {
        use appytappy_config::signal::{Number, Value};

        use super::*;
        use crate::screen::check::{BarFill, Check, ColorMatch, ReferenceMatch};
        use crate::screen::digits::DigitReader;

        const WHITE: Rgb = Rgb {
            red: 255,
            green: 255,
            blue: 255,
        };

        fn screenshots() -> [Image; 2] {
            [
                include_bytes!("../tests/fixtures/progress_37.png").as_slice(),
                include_bytes!("../tests/fixtures/progress_90.png").as_slice(),
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
            let glyphs = Image::decode_png(include_bytes!("../tests/fixtures/digits.png")).unwrap();
            let mut reader = DigitReader::new(WHITE, 60);
            reader.learn(&glyphs, "0123456789.%").unwrap();
            let check = Check::Number(reader);
            let counter = Region::new(120, 38, 100, 28).unwrap();
            assert_eq!(
                read(&check, &first, counter),
                Value::Number(Number::new(37.0).unwrap())
            );
            assert_eq!(
                read(&check, &second, counter),
                Value::Number(Number::new(90.5).unwrap())
            );
        }

        #[test]
        fn color_and_reference_see_the_panel_change() {
            let [first, second] = screenshots();
            let panel = Region::new(100, 110, 120, 50).unwrap();
            let gold = Check::Color(ColorMatch {
                color: second.pixel(105, 115).unwrap(),
                tolerance: 40,
            });
            let unchanged = Check::Reference(ReferenceMatch {
                reference: first.crop(panel).unwrap(),
                tolerance: 8,
            });
            let strip = Region::new(100, 110, 120, 12).unwrap();
            assert_eq!(read(&gold, &first, strip), Value::Flag(false));
            assert_eq!(read(&gold, &second, strip), Value::Flag(true));
            assert_eq!(read(&unchanged, &first, panel), Value::Flag(true));
            assert_eq!(read(&unchanged, &second, panel), Value::Flag(false));
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
    }
}
