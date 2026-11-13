//! Values placed at one window size, and what a frame means to the capture
//! that reads them there.
//!
//! A capture is started for one window size: its crop and packing are fixed
//! then. Every reader of saved values, the runtime's and the setup app's,
//! asks [`FrameMeaning`] what a frame means, so a frame of another size means the
//! same to all of them.

use std::io;

use super::saved::{AtSize, Unread};
use super::{Frame, Image, Layout, OutsideWindow, Size};

/// What a frame means to a capture that reads at one window size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameMeaning<T> {
    /// The window has the size the capture reads at: what the capture
    /// copied of it.
    Readable(T),
    /// The window has this other size, so nothing is read. Once the size
    /// has settled ([`super::settle::Settle`]), a reader places its values
    /// there.
    Resized(Size),
    /// The window has the size the capture reads at, and a region it copies
    /// lies outside it all the same. A capture started again would find the
    /// same, so this is no reason to start one.
    Unreadable(OutsideWindow),
}

impl<T> FrameMeaning<T> {
    /// What a frame of a window of `window_size` means to a capture that
    /// reads at `reads_at`. `copied` is what the capture copied of the
    /// frame, or `None` when it copied nothing because a region lies
    /// outside the window ([`OutsideWindow`]).
    #[must_use]
    pub fn of(reads_at: Size, window_size: Size, copied: Option<T>) -> Self {
        if window_size != reads_at {
            return Self::Resized(window_size);
        }
        copied.map_or(
            Self::Unreadable(OutsideWindow { window_size }),
            Self::Readable,
        )
    }
}

impl<'buffer> FrameMeaning<Frame<'buffer>> {
    /// What `frame` means, as a capture hands it to its handler, to a
    /// capture that reads at `reads_at`.
    ///
    /// # Errors
    ///
    /// Returns the frame's error when it says nothing about the window's
    /// size, such as that the capture ended.
    pub fn of_frame(reads_at: Size, frame: io::Result<Frame<'buffer>>) -> io::Result<Self> {
        match frame {
            Ok(frame) => Ok(Self::of(reads_at, frame.window_size, Some(frame))),
            Err(error) => OutsideWindow::in_error(&error)
                .map(|window_size| Self::of(reads_at, window_size, None))
                .ok_or(error),
        }
    }
}

/// Several values placed at one window size.
///
/// It holds where each reads or why it does not, and what a capture copies
/// to read those that do. A reader makes one for each size a window settles
/// at, since a scaled state check redraws its pictures
/// ([`super::saved::SavedValue::at`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    size: Size,
    values: Box<[Result<AtSize, Unread>]>,
    /// The regions of the values that read, packed, or `None` when none
    /// reads.
    layout: Option<Layout>,
}

impl Placed {
    /// `values` as they are placed at `size`, in their order.
    #[must_use]
    pub fn new(size: Size, values: impl IntoIterator<Item = Result<AtSize, Unread>>) -> Self {
        let values: Box<[_]> = values.into_iter().collect();
        let layout = Layout::new(values.iter().map(|at| at.as_ref().ok().map(|at| at.region)));
        Self {
            size,
            values,
            layout,
        }
    }

    /// The window size.
    #[must_use]
    pub const fn size(&self) -> Size {
        self.size
    }

    /// Each value's place, or why it does not read at the size.
    #[must_use]
    pub fn values(&self) -> &[Result<AtSize, Unread>] {
        &self.values
    }

    /// What a capture copies to read the values, or `None` when none reads
    /// at the size, so there is nothing to capture.
    #[must_use]
    pub const fn layout(&self) -> Option<&Layout> {
        self.layout.as_ref()
    }

    /// What `frame` means to a capture of [`Placed::layout`].
    ///
    /// # Errors
    ///
    /// Returns the frame's error when it says nothing about the window's
    /// size, such as that the capture ended.
    pub fn meaning<'buffer>(
        &self,
        frame: io::Result<Frame<'buffer>>,
    ) -> io::Result<FrameMeaning<Frame<'buffer>>> {
        FrameMeaning::of_frame(self.size, frame)
    }

    /// The pixels of the value at `position` in `image`, a readable frame's
    /// image, or `None` for a value that does not read at the size.
    #[must_use]
    pub fn cropped(&self, position: usize, image: &Image) -> Option<Image> {
        image.crop(self.layout.as_ref()?.packed(position)?)
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use appytappy_config::app_file::{PinDef, SizeChangesDef};

    use super::super::check::{Check, ColorMatch};
    use super::super::saved::{SavedCheck, SavedValue};
    use super::super::{Region, Rgb};
    use super::*;

    const WINDOW: Size = Size {
        width: 16,
        height: 12,
    };
    const SMALL: Size = Size {
        width: 8,
        height: 6,
    };

    fn frame(image: &Image, window_size: Size) -> Frame<'_> {
        Frame {
            window_size,
            image,
            received: Instant::now(),
            delay: Duration::ZERO,
        }
    }

    #[test]
    fn a_frame_is_read_only_at_the_size_its_capture_reads_at() {
        // What the capture copied at another size is of regions placed for
        // this one, so it is not read.
        assert_eq!(
            FrameMeaning::of(WINDOW, WINDOW, Some(7)),
            FrameMeaning::Readable(7)
        );
        assert_eq!(
            FrameMeaning::of(WINDOW, SMALL, Some(7)),
            FrameMeaning::Resized(SMALL)
        );
        // A window too small for the regions is another size, not a region
        // that cannot be read; one of the capture's own size is.
        assert_eq!(
            FrameMeaning::<u8>::of(WINDOW, SMALL, None),
            FrameMeaning::Resized(SMALL)
        );
        assert_eq!(
            FrameMeaning::<u8>::of(WINDOW, WINDOW, None),
            FrameMeaning::Unreadable(OutsideWindow {
                window_size: WINDOW
            })
        );
    }

    #[test]
    fn a_frame_error_about_the_windows_size_is_not_a_failed_capture() {
        let outside = |window_size| Err(io::Error::from(OutsideWindow { window_size }));
        let image = Image::new(SMALL, vec![0; 8 * 6 * 4]).unwrap();
        let shown = |frame| {
            FrameMeaning::of_frame(WINDOW, frame).map(|shown| match shown {
                FrameMeaning::Readable(frame) => FrameMeaning::Readable(frame.window_size),
                FrameMeaning::Resized(size) => FrameMeaning::Resized(size),
                FrameMeaning::Unreadable(outside) => FrameMeaning::Unreadable(outside),
            })
        };
        assert_eq!(
            shown(Ok(frame(&image, WINDOW))).unwrap(),
            FrameMeaning::Readable(WINDOW)
        );
        assert_eq!(
            shown(Ok(frame(&image, SMALL))).unwrap(),
            FrameMeaning::Resized(SMALL)
        );
        assert_eq!(shown(outside(SMALL)).unwrap(), FrameMeaning::Resized(SMALL));
        assert_eq!(
            shown(outside(WINDOW)).unwrap(),
            FrameMeaning::Unreadable(OutsideWindow {
                window_size: WINDOW
            })
        );
        let ended = shown(Err(io::ErrorKind::NotConnected.into())).unwrap_err();
        assert_eq!(ended.kind(), io::ErrorKind::NotConnected);
    }

    #[test]
    fn a_capture_copies_only_the_values_that_read_at_the_size() {
        let region = |x| Region::new(x, 0, 2, 2).unwrap();
        // A color value drawn at `x` on a window of [`WINDOW`], where it
        // reads.
        let at = |x| {
            SavedValue {
                drawn: SavedCheck {
                    window_size: WINDOW,
                    region: region(x),
                    check: Check::Color(ColorMatch {
                        color: Rgb {
                            red: 255,
                            green: 0,
                            blue: 0,
                        },
                        tolerance: 0,
                    }),
                },
                fixes: Vec::new(),
                pin: PinDef::TopLeft,
                size_changes: SizeChangesDef::Rarely,
            }
            .first()
        };
        let placed = Placed::new(WINDOW, [Ok(at(0)), Err(Unread::NotDrawn), Ok(at(8))]);
        let layout = placed.layout().expect("two values read");
        assert_eq!(
            layout
                .copies()
                .iter()
                .map(|copy| copy.from)
                .collect::<Vec<_>>(),
            [region(0), region(8)]
        );
        // The packed image holds the first value's pixels over the third's.
        let packed = Image::new(
            layout.size(),
            [
                [1_u8; 4], [1; 4], [1; 4], [1; 4], [3; 4], [3; 4], [3; 4], [3; 4],
            ]
            .concat(),
        )
        .unwrap();
        let pixels = |position| {
            placed
                .cropped(position, &packed)
                .map(|image| image.pixels()[0])
        };
        assert_eq!(pixels(0), Some([1; 4]));
        assert_eq!(pixels(1), None);
        assert_eq!(pixels(2), Some([3; 4]));

        let unread = Placed::new(SMALL, [Err(Unread::Outside), Err(Unread::NotDrawn)]);
        assert_eq!(unread.layout(), None, "nothing to capture");
        assert_eq!(unread.cropped(0, &packed), None);
    }
}
