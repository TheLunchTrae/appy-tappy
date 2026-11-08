//! A click's target: keeping a picture of what it clicks on, showing it, and
//! finding where the click would land on the picked window's newest frame,
//! as the runtime does when the click runs.

use appytappy_config::automation_file::ClickAtDef;
use appytappy_config::click::ClickAt;
use appytappy_sources::screen::find::PictureSearch;
use appytappy_sources::screen::mask::Mask;
use appytappy_sources::screen::{Image, Region, Size};
use serde::Serialize;

use crate::check_spec::{RegionSpec, sentence};
use crate::pixels;
use crate::selection::{self, SelectionSpec};

use super::Owner;
use super::host::Host;

/// A click picture just kept, and the size of the window it was taken on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClickPictureView {
    /// Its file name among the click pictures.
    pub(crate) image: String,
    /// The window's width when it was taken.
    pub(crate) window_width: u32,
    /// The window's height when it was taken.
    pub(crate) window_height: u32,
}

/// Where a click would land in the window, from its top-left corner, and
/// for a picture how alike the place found is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct FoundClick {
    /// Across, in pixels.
    pub(crate) x: u32,
    /// Down, in pixels.
    pub(crate) y: u32,
    /// From 0 to 100 percent; `None` for a spot.
    pub(crate) likeness: Option<u8>,
}

impl<H: Host> Owner<H> {
    /// Keeps `region` of screenshot `screenshot` as a new click picture and
    /// answers its name with the window's size. With `selection`, only the
    /// pixels it picks are compared; the rest of the picture is kept
    /// transparent. A picture that no saved automation names goes the next
    /// time the automations are saved.
    pub(crate) fn add_click_picture(
        &self,
        screenshot: usize,
        region: RegionSpec,
        selection: Option<&SelectionSpec>,
    ) -> Result<ClickPictureView, String> {
        let image = self
            .screenshots
            .get(screenshot)
            .ok_or_else(|| "That screenshot is gone. Take another.".to_owned())?;
        let outside = || {
            "The picture reaches past the edge of the window. Draw it inside the window.".to_owned()
        };
        let region =
            Region::new(region.x, region.y, region.width, region.height).ok_or_else(outside)?;
        let picture = image.crop(region).ok_or_else(outside)?;
        let mask = selection
            .map(|selection| {
                selection::select(image, selection)
                    .and_then(|selected| selected.mask_in(region))
                    .ok_or_else(|| "Nothing is selected. Mark the picture again.".to_owned())
            })
            .transpose()?;
        let bytes = match &mask {
            Some(mask) => picture.encode_png_masked(mask),
            None => picture.encode_png(),
        }
        .map_err(|error| sentence(&error.to_string()))?;
        let name = self
            .stores()?
            .automations
            .add_picture(&bytes)
            .map_err(|error| format!("The picture could not be kept: {error}."))?;
        let size = image.size();
        Ok(ClickPictureView {
            image: name,
            window_width: size.width,
            window_height: size.height,
        })
    }

    /// The click picture `name`'s pixels, at most `max_width` wide.
    pub(crate) fn click_picture_pixels(
        &self,
        name: &str,
        max_width: u32,
    ) -> Result<Vec<u8>, String> {
        let (image, _mask) = self.click_picture(name)?;
        Ok(pixels::rgba(&image, max_width))
    }

    /// Where `at` lands on the newest frame of the picked window, or why it
    /// lands nowhere: a spot outside the window, a window of another size
    /// than the picture was taken at, or a picture that is not found.
    pub(crate) fn find_click(&self, at: &ClickAtDef) -> Result<FoundClick, String> {
        if let Some(failure) = &self.failure {
            return Err(sentence(&failure.error));
        }
        let frame = self
            .latest
            .as_ref()
            .ok_or_else(|| "No frame has arrived from the window yet.".to_owned())?;
        let size = frame.size();
        match at.to_at().map_err(|error| sentence(&error.to_string()))? {
            ClickAt::Spot(spot) => spot
                .place(size.width, size.height)
                .map(|(x, y)| FoundClick {
                    x,
                    y,
                    likeness: None,
                })
                .ok_or_else(|| {
                    "The spot lies outside the window. Pick it again on this window.".to_owned()
                }),
            ClickAt::Picture(wanted) => {
                let taken = Size {
                    width: wanted.window_width,
                    height: wanted.window_height,
                };
                if size != taken {
                    return Err(format!(
                        "The window is {}x{} now, but the picture was taken at {}x{}. A click \
                         looks for it only while the window has that size.",
                        size.width, size.height, taken.width, taken.height
                    ));
                }
                let (picture, mask) = self.click_picture(&wanted.image)?;
                let size = picture.size();
                wanted
                    .fits_picture(size.width, size.height)
                    .map_err(|misfit| sentence(&misfit.to_string()))?;
                let area = wanted.search;
                let searched = Region::new(area.x, area.y, area.width, area.height)
                    .and_then(|region| frame.crop(region))
                    .ok_or_else(|| "The search area reaches past the window.".to_owned())?;
                let search = PictureSearch::new(picture, mask.as_ref(), wanted.tolerance)
                    .map_err(|error| sentence(&error.to_string()))?;
                let found = search.find(&searched).ok_or_else(|| {
                    "The picture is not in its search area now, so the click would not happen."
                        .to_owned()
                })?;
                let (press_x, press_y) = wanted.press;
                let x = area
                    .x
                    .checked_add(found.x)
                    .and_then(|x| x.checked_add(press_x));
                let y = area
                    .y
                    .checked_add(found.y)
                    .and_then(|y| y.checked_add(press_y));
                x.zip(y)
                    .map(|(x, y)| FoundClick {
                        x,
                        y,
                        likeness: Some(found.likeness),
                    })
                    .ok_or_else(|| "The place found lies outside the window.".to_owned())
            },
        }
    }

    /// The click picture `name`.
    fn click_picture(&self, name: &str) -> Result<(Image, Option<Mask>), String> {
        let bytes = self
            .stores()?
            .automations
            .picture(name)
            .map_err(|error| format!("The picture {name} cannot be read: {error}."))?;
        Image::decode_png_masked(&bytes)
            .map_err(|error| format!("The picture {name} cannot be read: {error}."))
    }
}

#[cfg(test)]
mod tests {
    use appytappy_config::app_file::{RegionDef, SizeDef};
    use appytappy_config::automation_file::{ClickAtDef, CornerDef, PointDef};

    use super::*;
    use crate::owner::test_support::{FakeHost, frame_image, harness, harness_with, scratch_store};
    use crate::selection::{Combine, SelectStep};

    fn region(x: u32, y: u32, width: u32, height: u32) -> RegionSpec {
        RegionSpec {
            x,
            y,
            width,
            height,
        }
    }

    fn picture(
        image: &str,
        window: (u32, u32),
        search: (u32, u32, u32, u32),
        tolerance: u8,
    ) -> ClickAtDef {
        ClickAtDef::Picture {
            image: image.to_owned(),
            window_size: SizeDef {
                width: window.0,
                height: window.1,
            },
            search: RegionDef {
                x: search.0,
                y: search.1,
                width: search.2,
                height: search.3,
            },
            press: PointDef { x: 0, y: 1 },
            tolerance,
        }
    }

    #[test]
    fn a_picture_is_kept_from_a_screenshot_and_found_on_the_live_frame() {
        let (stores, root) = scratch_store("click-picture");
        let mut harness = harness_with(false, Ok(stores));
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        // The gray right half of the 4 by 2 frame.
        let kept = harness
            .ask(|owner| owner.add_click_picture(0, region(2, 0, 2, 2), None))
            .expect("kept");
        assert_eq!(
            kept,
            ClickPictureView {
                image: "click-1.png".to_owned(),
                window_width: 4,
                window_height: 2,
            }
        );
        let thumbnail = harness
            .ask(|owner| owner.click_picture_pixels("click-1.png", 10))
            .expect("shows");
        assert_eq!(thumbnail.get(..8), Some(&[2, 0, 0, 0, 2, 0, 0, 0][..]));

        let found =
            harness.ask(|owner| owner.find_click(&picture("click-1.png", (4, 2), (1, 0, 3, 2), 0)));
        assert_eq!(
            found,
            Ok(FoundClick {
                x: 2,
                y: 1,
                likeness: Some(100),
            })
        );
        let missing =
            harness.ask(|owner| owner.find_click(&picture("click-1.png", (4, 2), (0, 0, 2, 2), 0)));
        assert!(
            missing
                .expect_err("the red half")
                .contains("not in its search area")
        );
        let resized =
            harness.ask(|owner| owner.find_click(&picture("click-1.png", (8, 2), (0, 0, 2, 2), 0)));
        assert!(resized.expect_err("another size").contains("4x2 now"));
        let narrow =
            harness.ask(|owner| owner.find_click(&picture("click-1.png", (4, 2), (2, 0, 1, 2), 0)));
        assert!(narrow.expect_err("too narrow").contains("fit inside"));
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_selection_keeps_only_what_it_picks() {
        let (stores, root) = scratch_store("click-picture-mask");
        let mut harness = harness_with(false, Ok(stores));
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        // A click on the gray half picks it alone, so the red column the
        // region also holds is left out.
        let gray = SelectionSpec {
            tolerance: 0,
            steps: vec![SelectStep::Click {
                x: 3,
                y: 0,
                combine: Combine::Replace,
            }],
        };
        let kept = harness
            .ask(|owner| owner.add_click_picture(0, region(1, 0, 3, 2), Some(&gray)))
            .expect("kept");
        let masked = harness.ask(|owner| {
            owner
                .click_picture(&kept.image)
                .map(|(_picture, mask)| mask.is_some())
        });
        assert_eq!(masked, Ok(true));
        let nothing = SelectionSpec {
            tolerance: 0,
            steps: Vec::new(),
        };
        assert!(
            harness
                .ask(|owner| owner.add_click_picture(0, region(1, 0, 3, 2), Some(&nothing)))
                .expect_err("nothing selected")
                .contains("Nothing is selected")
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_spot_is_placed_on_the_live_frame() {
        let mut harness = harness(false);
        let spot = |x, y| ClickAtDef::Spot {
            corner: CornerDef::BottomRight,
            x,
            y,
        };
        let find = |owner: &mut Owner<FakeHost>| owner.find_click(&spot(1, 0));
        assert!(
            harness
                .ask(find)
                .expect_err("no frame")
                .contains("No frame")
        );
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(
            harness.ask(find),
            Ok(FoundClick {
                x: 2,
                y: 1,
                likeness: None,
            })
        );
        assert!(
            harness
                .ask(|owner| owner.find_click(&spot(4, 0)))
                .expect_err("outside")
                .contains("outside the window")
        );
    }

    #[test]
    fn a_picture_needs_a_screenshot_and_a_region_inside_it() {
        let (stores, root) = scratch_store("click-picture-bad");
        let mut harness = harness_with(false, Ok(stores));
        assert!(
            harness
                .ask(|owner| owner.add_click_picture(0, region(0, 0, 1, 1), None))
                .expect_err("no screenshot")
                .contains("Take another")
        );
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        assert!(
            harness
                .ask(|owner| owner.add_click_picture(0, region(3, 0, 2, 2), None))
                .expect_err("past the edge")
                .contains("past the edge")
        );
        assert!(
            harness
                .ask(|owner| owner.click_picture_pixels("click-9.png", 10))
                .expect_err("missing")
                .contains("cannot be read")
        );
        // Nothing was kept, so nothing was written.
        assert!(!root.exists());
    }
}
