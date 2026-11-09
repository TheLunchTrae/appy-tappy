//! A click's picture, ready to look for in its area of the window, and where
//! the click lands when the picture is found there.
//!
//! The runtime places a picture click with a [`ClickSearch`], and the setup
//! app's preview of where a click lands and its check that a click's picture
//! can be used make the same one, so all three agree with where the runtime
//! clicks.

use std::error::Error;
use std::fmt;

use appytappy_config::click::{ClickPicture, InvalidClick};

use super::find::PictureSearch;
use super::{Image, ImageError, Region, Size};

/// A click's picture, ready to look for in its area of the window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClickSearch {
    search: PictureSearch,
    /// The area of the window it is looked for in.
    area: Region,
    /// Where on the picture to press, from its top-left corner.
    press: (u32, u32),
    /// The window's size when the picture was taken.
    taken: Size,
}

/// Where a click on a picture lands, in pixels from the window's top-left
/// corner, and how alike the place found is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Landing {
    /// Across.
    pub x: u32,
    /// Down.
    pub y: u32,
    /// How alike the place found is to the picture, from 0 to 100 percent.
    pub likeness: u8,
}

impl ClickSearch {
    /// The search for `wanted`'s picture, decoded from `bytes`, its PNG
    /// file. This is the one check that a click's picture can be used: it
    /// reads, it fits the area it is looked for in, and the place the click
    /// presses lies on it.
    ///
    /// # Errors
    ///
    /// Returns [`ClickPictureError::Read`] when `bytes` is not a PNG file
    /// it can read, or [`ClickPictureError::Misfit`] when the picture does
    /// not suit the click.
    pub fn new(wanted: &ClickPicture, bytes: &[u8]) -> Result<Self, ClickPictureError> {
        let (picture, mask) = Image::decode_png_masked(bytes).map_err(ClickPictureError::Read)?;
        let size = picture.size();
        wanted
            .fits_picture(size.width, size.height)
            .map_err(ClickPictureError::Misfit)?;
        let area = wanted.search;
        // `fits_picture` has already refused an empty area, since no picture
        // fits one, so this fallback is only defensive.
        let area = Region::new(area.x, area.y, area.width, area.height)
            .ok_or(ClickPictureError::Misfit(InvalidClick::Area))?;
        // A decoded picture's mask has the picture's size.
        let search = PictureSearch::new(picture, mask.as_ref(), wanted.tolerance)
            .map_err(|error| ClickPictureError::Read(ImageError::Mask(error)))?;
        Ok(Self {
            search,
            area,
            press: wanted.press,
            taken: Size {
                width: wanted.window_width,
                height: wanted.window_height,
            },
        })
    }

    /// The window's size when the picture was taken. The area means
    /// something only while the window has this size.
    #[must_use]
    pub const fn taken(&self) -> Size {
        self.taken
    }

    /// The area of the window the picture is looked for in.
    #[must_use]
    pub const fn area(&self) -> Region {
        self.area
    }

    /// Where the click lands when its picture is in `searched`, the pixels
    /// of [`Self::area`] in a frame of the window, or `None` when it is not
    /// there.
    #[must_use]
    pub fn locate(&self, searched: &Image) -> Option<Landing> {
        let found = self.search.find(searched)?;
        let (press_x, press_y) = self.press;
        // The area lies in a window at most 16384 pixels on a side, and the
        // press on the picture found inside it, so neither sum overflows.
        let x = self.area.x().checked_add(found.x)?.checked_add(press_x)?;
        let y = self.area.y().checked_add(found.y)?.checked_add(press_y)?;
        Some(Landing {
            x,
            y,
            likeness: found.likeness,
        })
    }
}

/// A click's picture cannot be used.
#[derive(Debug)]
pub enum ClickPictureError {
    /// The file is not a PNG file AppyTappy can read.
    Read(ImageError),
    /// The picture does not suit the click: it is larger than the area it
    /// is looked for in, or the place the click presses lies outside it.
    Misfit(InvalidClick),
}

impl fmt::Display for ClickPictureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(_) => {
                formatter.write_str("the click's picture is not a PNG file it can read")
            },
            Self::Misfit(misfit) => misfit.fmt(formatter),
        }
    }
}

impl Error for ClickPictureError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(source) => Some(source),
            Self::Misfit(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use appytappy_config::click::Area;

    use super::*;

    /// A 20 by 10 frame of gray 10 with a 4 by 2 patch of grays 50 to 120
    /// at (12, 6).
    fn frame() -> Image {
        let mut bgra = Vec::new();
        for y in 0..10_u32 {
            for x in 0..20_u32 {
                let gray = patch_gray(x, y).unwrap_or(10);
                bgra.extend([gray, gray, gray, u8::MAX]);
            }
        }
        Image::new(
            Size {
                width: 20,
                height: 10,
            },
            bgra,
        )
        .unwrap()
    }

    fn patch_gray(x: u32, y: u32) -> Option<u8> {
        let across = x.checked_sub(12).filter(|&across| across < 4)?;
        let down = y.checked_sub(6).filter(|&down| down < 2)?;
        let step = down.checked_mul(4)?.checked_add(across)?;
        u8::try_from(step.checked_mul(10)?.checked_add(50)?).ok()
    }

    fn patch_png() -> Vec<u8> {
        frame()
            .crop(Region::new(12, 6, 4, 2).unwrap())
            .unwrap()
            .encode_png()
            .unwrap()
    }

    fn wanted(search: Area, press: (u32, u32)) -> ClickPicture {
        ClickPicture::new("patch-1.png", (20, 10), search, press, 0).unwrap()
    }

    fn area(x: u32, y: u32, width: u32, height: u32) -> Area {
        Area {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn a_click_lands_on_its_press_where_its_picture_is_found_in_its_area() {
        // The patch is 3 across and 2 down in the area, and pressed 3
        // across and 1 down, so a swapped axis lands elsewhere.
        let search = ClickSearch::new(&wanted(area(9, 4, 11, 6), (3, 1)), &patch_png()).unwrap();
        assert_eq!(
            search.taken(),
            Size {
                width: 20,
                height: 10
            }
        );
        assert_eq!(search.area(), Region::new(9, 4, 11, 6).unwrap());
        let searched = frame().crop(search.area()).unwrap();
        assert_eq!(
            search.locate(&searched),
            Some(Landing {
                x: 15,
                y: 7,
                likeness: 100,
            })
        );
        let elsewhere = ClickSearch::new(&wanted(area(0, 0, 10, 6), (0, 0)), &patch_png()).unwrap();
        let searched = frame().crop(elsewhere.area()).unwrap();
        assert_eq!(elsewhere.locate(&searched), None);
    }

    #[test]
    fn a_picture_must_read_fit_its_area_and_hold_the_press() {
        let fits = |search, press| {
            ClickSearch::new(&wanted(search, press), &patch_png())
                .map(drop)
                .map_err(|error| match error {
                    ClickPictureError::Misfit(misfit) => Some(misfit),
                    ClickPictureError::Read(_) => None,
                })
        };
        assert_eq!(
            fits(area(0, 0, 4, 2), (3, 1)),
            Ok(()),
            "a picture as large as its area, pressed at its last pixel"
        );
        assert_eq!(
            fits(area(0, 0, 3, 2), (0, 0)),
            Err(Some(InvalidClick::LargePicture))
        );
        assert_eq!(
            fits(area(0, 0, 4, 1), (0, 0)),
            Err(Some(InvalidClick::LargePicture))
        );
        for press in [(4, 0), (0, 2)] {
            assert_eq!(
                fits(area(0, 0, 10, 10), press),
                Err(Some(InvalidClick::PressOutside)),
                "{press:?}"
            );
        }
        let unread = ClickSearch::new(&wanted(area(0, 0, 4, 2), (0, 0)), b"not a png");
        let Err(error) = unread else {
            panic!("not a PNG file");
        };
        assert!(matches!(error, ClickPictureError::Read(_)));
        assert!(error.source().is_some(), "the decoder's error is kept");
        assert_eq!(
            error.to_string(),
            "the click's picture is not a PNG file it can read"
        );
        assert_eq!(
            ClickPictureError::Misfit(InvalidClick::PressOutside).to_string(),
            InvalidClick::PressOutside.to_string()
        );
    }
}
