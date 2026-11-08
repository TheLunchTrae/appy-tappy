//! A click in another application: where it lands and how it presses.
//!
//! These are validated types; `wiki/Actions.md` explains how the
//! runtime finds the target and when it clicks.

use std::error::Error;
use std::fmt;

use crate::app_file::is_image_name;
use crate::store::is_app_id;

/// The widest or tallest distance a spot or a picture's area may have, in
/// pixels: wider than any screen, so a bad number fails here and not later.
pub const MAX_CLICK_DISTANCE: u32 = 16_384;

/// The most pixels the area a picture is looked for in may hold, such as
/// 400 by 400. The search takes longer the bigger the area, and actions
/// wait for it.
pub const MAX_SEARCH_PIXELS: u64 = 160_000;

/// A click in a window of another saved application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Click {
    /// The ID of the saved application whose window it clicks in.
    pub app: String,
    /// Which button, and how many times.
    pub button: ClickButton,
    /// Whether the window that was in front gets the focus back after.
    pub return_focus: bool,
    /// Where it clicks.
    pub at: ClickAt,
}

impl Click {
    /// Validates a click in the application `app`.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidClick`] when `app` cannot name a saved application.
    pub fn new(
        app: &str,
        button: ClickButton,
        return_focus: bool,
        at: ClickAt,
    ) -> Result<Self, InvalidClick> {
        if !is_app_id(app) {
            return Err(InvalidClick::App);
        }
        Ok(Self {
            app: app.to_owned(),
            button,
            return_focus,
            at,
        })
    }
}

/// Which mouse button a click presses, and how many times.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickButton {
    /// One press of the left button.
    Left,
    /// One press of the right button.
    Right,
    /// Two quick presses of the left button.
    Double,
}

/// A corner of a window, which a spot is measured from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corner {
    /// The top-left corner: a spot is right of and below it.
    TopLeft,
    /// The top-right corner: a spot is left of and below it.
    TopRight,
    /// The bottom-left corner: a spot is right of and above it.
    BottomLeft,
    /// The bottom-right corner: a spot is left of and above it.
    BottomRight,
}

/// Where a click lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClickAt {
    /// The same place every time, measured from a corner, so it stays put
    /// when the window is resized.
    Spot(Spot),
    /// A place on a picture, wherever the picture is found in an area of
    /// the window. Nothing is clicked when it is not there.
    Picture(ClickPicture),
}

/// A place measured from a corner of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spot {
    /// The corner it is measured from.
    pub corner: Corner,
    /// How far across from the corner, in pixels.
    pub x: u32,
    /// How far up or down from the corner, in pixels.
    pub y: u32,
}

impl Spot {
    /// Validates a spot `x` across and `y` up or down from `corner`.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidClick::Distance`] when either distance is over
    /// [`MAX_CLICK_DISTANCE`].
    pub const fn new(corner: Corner, x: u32, y: u32) -> Result<Self, InvalidClick> {
        if x > MAX_CLICK_DISTANCE || y > MAX_CLICK_DISTANCE {
            return Err(InvalidClick::Distance);
        }
        Ok(Self { corner, x, y })
    }

    /// The spot in a window `width` by `height` pixels, from its top-left
    /// corner, or `None` when it lies outside the window.
    #[must_use]
    pub fn place(self, width: u32, height: u32) -> Option<(u32, u32)> {
        let x = match self.corner {
            Corner::TopLeft | Corner::BottomLeft => self.x,
            Corner::TopRight | Corner::BottomRight => width.checked_sub(1)?.checked_sub(self.x)?,
        };
        let y = match self.corner {
            Corner::TopLeft | Corner::TopRight => self.y,
            Corner::BottomLeft | Corner::BottomRight => {
                height.checked_sub(1)?.checked_sub(self.y)?
            },
        };
        (x < width && y < height).then_some((x, y))
    }
}

/// A rectangle of a window in pixels from its top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Area {
    /// The left edge.
    pub x: u32,
    /// The top edge.
    pub y: u32,
    /// The width.
    pub width: u32,
    /// The height.
    pub height: u32,
}

/// A picture to find in an area of the window, and where on it to press.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClickPicture {
    /// The picture's file name among the click pictures, such as
    /// `scene-button-1.png`. Its transparent pixels are not compared.
    pub image: String,
    /// The window's size when the picture was taken. The area means
    /// something only while the window has this size.
    pub window_width: u32,
    /// The window's height when the picture was taken.
    pub window_height: u32,
    /// The area of the window the picture is looked for in.
    pub search: Area,
    /// Where on the picture to press, from its top-left corner.
    pub press: (u32, u32),
    /// How far, on average, each compared pixel's red, green, and blue may
    /// be from the picture's.
    pub tolerance: u8,
}

impl ClickPicture {
    /// Validates a picture found in `search` of a window `window` in size.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidClick`] when the image name is not a picture file
    /// name, the window or the area is empty, the area does not fit the
    /// window, or it holds more than [`MAX_SEARCH_PIXELS`].
    pub fn new(
        image: &str,
        (window_width, window_height): (u32, u32),
        search: Area,
        press: (u32, u32),
        tolerance: u8,
    ) -> Result<Self, InvalidClick> {
        if !is_image_name(image) {
            return Err(InvalidClick::Image);
        }
        let fits = |start: u32, length: u32, room: u32| {
            length > 0 && start.checked_add(length).is_some_and(|end| end <= room)
        };
        if window_width == 0
            || window_height == 0
            || window_width > MAX_CLICK_DISTANCE
            || window_height > MAX_CLICK_DISTANCE
            || !fits(search.x, search.width, window_width)
            || !fits(search.y, search.height, window_height)
        {
            return Err(InvalidClick::Area);
        }
        if u64::from(search.width).saturating_mul(u64::from(search.height)) > MAX_SEARCH_PIXELS {
            return Err(InvalidClick::LargeArea);
        }
        Ok(Self {
            image: image.to_owned(),
            window_width,
            window_height,
            search,
            press,
            tolerance,
        })
    }

    /// Checks the click against its picture's size, `width` by `height`:
    /// the picture has to fit its search area, and the place it presses has
    /// to lie on it.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidClick::LargePicture`] when the picture is wider or
    /// taller than its search area, or [`InvalidClick::PressOutside`] when
    /// the place it presses lies outside it.
    pub const fn fits_picture(&self, width: u32, height: u32) -> Result<(), InvalidClick> {
        if width > self.search.width || height > self.search.height {
            return Err(InvalidClick::LargePicture);
        }
        let (x, y) = self.press;
        if x >= width || y >= height {
            return Err(InvalidClick::PressOutside);
        }
        Ok(())
    }
}

/// A click could not be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidClick {
    /// The application ID cannot name a saved application.
    App,
    /// A spot is farther than [`MAX_CLICK_DISTANCE`] from its corner.
    Distance,
    /// The picture's name is not a picture file name.
    Image,
    /// The area is empty or does not fit the window it was drawn on.
    Area,
    /// The area holds more than [`MAX_SEARCH_PIXELS`].
    LargeArea,
    /// The picture is wider or taller than the area it is looked for in.
    LargePicture,
    /// The place the click presses lies outside its picture.
    PressOutside,
}

impl fmt::Display for InvalidClick {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::App => "a click needs the application it clicks in",
            Self::Distance => "a click's spot must lie within 16384 pixels of its corner",
            Self::Image => {
                "a click's picture must be a lowercase .png file name of letters, digits, \
                 hyphens, and underscores"
            },
            Self::Area => "the area a click looks for its picture in must lie inside the window",
            Self::LargeArea => {
                "the area a click looks for its picture in can hold at most 160000 pixels, such as \
                 400 by 400"
            },
            Self::LargePicture => "a click's picture must fit inside the area it is looked for in",
            Self::PressOutside => "the place a click presses must lie on its picture",
        })
    }
}

impl Error for InvalidClick {}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(x: u32, y: u32, width: u32, height: u32) -> Area {
        Area {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn a_click_names_an_application_by_its_id() {
        let spot = ClickAt::Spot(Spot::new(Corner::TopLeft, 1, 2).unwrap());
        let click = Click::new("streaming_app", ClickButton::Right, false, spot.clone()).unwrap();
        assert_eq!(click.app, "streaming_app");
        assert_eq!(click.button, ClickButton::Right);
        assert!(!click.return_focus);
        assert_eq!(click.at, spot);
        for bad in ["", "Streaming App", "../apps"] {
            assert_eq!(
                Click::new(bad, ClickButton::Left, true, spot.clone()),
                Err(InvalidClick::App),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn a_spot_is_measured_from_its_corner() {
        let place = |corner, x, y| Spot::new(corner, x, y).unwrap().place(100, 50);
        assert_eq!(place(Corner::TopLeft, 10, 5), Some((10, 5)));
        assert_eq!(place(Corner::TopRight, 10, 5), Some((89, 5)));
        assert_eq!(place(Corner::BottomLeft, 10, 5), Some((10, 44)));
        assert_eq!(place(Corner::BottomRight, 10, 5), Some((89, 44)));
        assert_eq!(place(Corner::TopLeft, 0, 0), Some((0, 0)));
        assert_eq!(place(Corner::BottomRight, 0, 0), Some((99, 49)));
        assert_eq!(place(Corner::TopRight, 99, 49), Some((0, 49)));
    }

    #[test]
    fn a_spot_outside_the_window_has_no_place() {
        let place = |corner, x, y| Spot::new(corner, x, y).unwrap().place(100, 50);
        assert_eq!(place(Corner::TopLeft, 100, 0), None);
        assert_eq!(place(Corner::TopLeft, 0, 50), None);
        assert_eq!(place(Corner::TopRight, 100, 0), None);
        assert_eq!(place(Corner::BottomLeft, 0, 50), None);
        assert_eq!(Spot::new(Corner::TopLeft, 0, 0).unwrap().place(0, 0), None);
        assert_eq!(
            Spot::new(Corner::BottomRight, 0, 0).unwrap().place(0, 5),
            None
        );
        assert_eq!(
            Spot::new(Corner::BottomRight, 0, 0).unwrap().place(5, 0),
            None
        );
    }

    #[test]
    fn a_spot_is_bounded() {
        assert!(Spot::new(Corner::TopLeft, MAX_CLICK_DISTANCE, MAX_CLICK_DISTANCE).is_ok());
        let over = MAX_CLICK_DISTANCE.checked_add(1).unwrap();
        assert_eq!(
            Spot::new(Corner::TopLeft, over, 0),
            Err(InvalidClick::Distance)
        );
        assert_eq!(
            Spot::new(Corner::TopLeft, 0, over),
            Err(InvalidClick::Distance)
        );
    }

    #[test]
    fn a_picture_s_area_lies_inside_its_window() {
        let picture =
            |window, search| ClickPicture::new("button-1.png", window, search, (1, 1), 20);
        let made = picture((200, 100), area(150, 60, 50, 40)).unwrap();
        assert_eq!(made.image, "button-1.png");
        assert_eq!((made.window_width, made.window_height), (200, 100));
        assert_eq!(made.search, area(150, 60, 50, 40));
        assert_eq!(made.press, (1, 1));
        assert_eq!(made.tolerance, 20);
        for (window, search) in [
            ((200, 100), area(151, 60, 50, 40)),
            ((200, 100), area(150, 61, 50, 40)),
            ((200, 100), area(0, 0, 0, 10)),
            ((200, 100), area(0, 0, 10, 0)),
            ((200, 100), area(u32::MAX, 0, 1, 1)),
            ((200, 100), area(0, u32::MAX, 1, 1)),
            ((0, 100), area(0, 0, 1, 1)),
            ((200, 0), area(0, 0, 1, 1)),
            ((MAX_CLICK_DISTANCE + 1, 100), area(0, 0, 1, 1)),
            ((200, MAX_CLICK_DISTANCE + 1), area(0, 0, 1, 1)),
        ] {
            assert_eq!(
                picture(window, search),
                Err(InvalidClick::Area),
                "{window:?} {search:?}"
            );
        }
        assert!(picture((MAX_CLICK_DISTANCE, MAX_CLICK_DISTANCE), area(0, 0, 1, 1)).is_ok());
        assert!(picture((1000, 1000), area(0, 0, 400, 400)).is_ok());
        for search in [area(0, 0, 400, 401), area(0, 0, 401, 400)] {
            assert_eq!(
                picture((1000, 1000), search),
                Err(InvalidClick::LargeArea),
                "{search:?}"
            );
        }
    }

    #[test]
    fn a_picture_fits_its_area_and_holds_its_press() {
        let made =
            ClickPicture::new("button-1.png", (200, 100), area(0, 0, 30, 20), (9, 4), 20).unwrap();
        assert_eq!(made.fits_picture(30, 20), Ok(()));
        assert_eq!(made.fits_picture(10, 5), Ok(()));
        assert_eq!(made.fits_picture(31, 20), Err(InvalidClick::LargePicture));
        assert_eq!(made.fits_picture(30, 21), Err(InvalidClick::LargePicture));
        assert_eq!(made.fits_picture(9, 5), Err(InvalidClick::PressOutside));
        assert_eq!(made.fits_picture(10, 4), Err(InvalidClick::PressOutside));
    }

    #[test]
    fn a_picture_needs_a_picture_file_name() {
        assert_eq!(
            ClickPicture::new("Button.PNG", (10, 10), area(0, 0, 1, 1), (0, 0), 20),
            Err(InvalidClick::Image)
        );
    }

    #[test]
    fn errors_explain_themselves() {
        assert!(InvalidClick::Distance.to_string().contains("16384"));
        assert!(InvalidClick::App.to_string().contains("application"));
        assert!(InvalidClick::Image.to_string().contains(".png"));
        assert!(InvalidClick::Area.to_string().contains("inside the window"));
        assert!(InvalidClick::LargeArea.to_string().contains("160000"));
        assert!(
            InvalidClick::LargePicture
                .to_string()
                .contains("fit inside")
        );
        assert!(
            InvalidClick::PressOutside
                .to_string()
                .contains("on its picture")
        );
    }
}
