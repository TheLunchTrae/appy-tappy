//! A click's target: keeping a picture of what it clicks on, showing it,
//! finding where the click would land on the picked window's newest frame
//! with the runtime's own [`ClickSearch`], and sending one click when the
//! user asks for it.

use appytappy_config::anti_cheat::{AntiCheatClass, allows_input};
use appytappy_config::app_file::AppFile;
use appytappy_config::automation_file::{ClickAtDef, ClickButtonDef, ClickDeliveryDef};
use appytappy_config::click::{Click, ClickAt, ClickPicture};
use appytappy_sources::screen::click::{ClickPictureError, ClickSearch};
use appytappy_sources::screen::mask::Mask;
use appytappy_sources::screen::place::Target;
use appytappy_sources::screen::{Image, Region};
use serde::{Deserialize, Serialize};

use crate::check_spec::{RegionSpec, sentence};
use crate::pixels;
use crate::selection::{self, SelectionSpec};

use super::Owner;
use super::host::{ClickNow, ClickedWindow, Host};

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

/// A click to send now, as the click target screen sets it up.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ClickSpec {
    /// The ID of the saved application whose value the automation watches,
    /// or nothing before the automation watches one.
    pub(crate) watched: Option<String>,
    /// The ID of the saved application it clicks in.
    pub(crate) app: String,
    /// Which button, and how many times.
    pub(crate) button: ClickButtonDef,
    /// How it reaches the window.
    pub(crate) delivery: ClickDeliveryDef,
    /// Whether the window that was in front gets the focus back after a
    /// click with the cursor.
    pub(crate) return_focus: bool,
    /// Where it clicks.
    pub(crate) at: ClickAtDef,
}

/// The window a click sent on request went to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClickSent {
    /// The window's handle, as picking a window names it.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) window: u64,
    /// The window's title, which tells an application's windows apart.
    pub(crate) title: String,
}

/// Refuses a click while the application `file` describes runs with
/// anti-cheat, or with none set: a click is input that AppyTappy makes for
/// another program however it is delivered, which the anti-cheat policy
/// allows only for the class none.
fn allows_clicks(file: &AppFile) -> Result<(), String> {
    if allows_input(AntiCheatClass::from(file.anti_cheat)) {
        return Ok(());
    }
    Err(format!(
        "AppyTappy cannot click while {name} runs, because anti-cheat can treat a click as \
         cheating. Set {name}'s anti-cheat to None on the Applications page first.",
        name = file.name
    ))
}

impl<H: Host> Owner<H> {
    /// Sends the click `click` now, once, because the user asked for it on
    /// the click target screen. It is placed and sent the way the runtime
    /// does both, so it lands, or fails, as the automation's click would.
    ///
    /// It needs what an automation's click needs to run: the application it
    /// clicks in, and the watched one when the automation has one, with the
    /// anti-cheat class none.
    ///
    /// The click goes to the window that the application's window rules
    /// find now, as the runtime's does, which with several of the
    /// application's windows open may be another one than the picked
    /// window. The answer names it.
    ///
    /// `Ok` says the click was sent. Whether its application acted on it,
    /// nothing reports.
    pub(crate) fn click_now(&self, click: &ClickSpec) -> Result<ClickSent, String> {
        let invalid = |error: appytappy_config::click::InvalidClick| sentence(&error.to_string());
        let at = click.at.to_at().map_err(invalid)?;
        let delivery = click.delivery.to_delivery(click.return_focus);
        let click_ = Click::new(&click.app, click.button.into(), delivery, at).map_err(invalid)?;
        let stores = self.stores()?;
        if let Some(watched) = &click.watched {
            allows_clicks(&stores.app(watched)?.file)?;
        }
        let app = stores.app(&click_.app)?.file;
        allows_clicks(&app)?;
        let search;
        let target = match &click_.at {
            ClickAt::Spot(spot) => Target::Spot(*spot),
            ClickAt::Picture(wanted) => {
                search = self.click_search(wanted)?;
                Target::Picture(&search)
            },
        };
        self.host
            .click(ClickNow {
                app: &app.name,
                window: &app.window.to_match(),
                target,
                button: click_.button,
                delivery: click_.delivery,
            })
            .map(|ClickedWindow { handle, title }| ClickSent {
                window: handle,
                title,
            })
            .map_err(|error| sentence(&error))
    }

    /// The search for the click picture `wanted` names, read from its file.
    fn click_search(&self, wanted: &ClickPicture) -> Result<ClickSearch, String> {
        let name = &wanted.image;
        let unread =
            |error: &dyn std::fmt::Display| format!("The picture {name} cannot be read: {error}.");
        let bytes = self
            .stores()?
            .automations
            .picture(name)
            .map_err(|error| unread(&error))?;
        ClickSearch::new(wanted, &bytes).map_err(|error| match error {
            ClickPictureError::Read(error) => unread(&error),
            ClickPictureError::Misfit(misfit) => sentence(&misfit.to_string()),
        })
    }

    /// Keeps `region` of screenshot `screenshot` as a new click picture and
    /// answers its name with the window's size. With `selection`, only the
    /// pixels it picks are compared; the rest of the picture is kept
    /// transparent. A picture that no saved automation names goes the next
    /// time the automations are saved, or once the automation's flow ends
    /// ([`Owner::forget_unsaved_click_pictures`]).
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
                let search = self.click_search(&wanted)?;
                let taken = search.taken();
                if size != taken {
                    return Err(format!(
                        "The window is {}x{} now, but the picture was taken at {}x{}. A click \
                         looks for it only while the window has that size.",
                        size.width, size.height, taken.width, taken.height
                    ));
                }
                let searched = frame
                    .crop(search.area())
                    .ok_or_else(|| "The search area reaches past the window.".to_owned())?;
                search
                    .locate(&searched)
                    .map(|landing| FoundClick {
                        x: landing.x,
                        y: landing.y,
                        likeness: Some(landing.likeness),
                    })
                    .ok_or_else(|| {
                        "The picture is not in its search area now, so the click would not \
                         happen."
                            .to_owned()
                    })
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
    use appytappy_config::app_file::{AntiCheatDef, RegionDef, SizeChangesDef, SizeDef, WindowDef};
    use appytappy_config::automation_file::{ClickAtDef, CornerDef, PointDef};
    use appytappy_config::click::{ClickButton, ClickDelivery, Corner, Spot};
    use appytappy_config::window_match::WindowFacts;

    use super::*;
    use crate::applications::AppSpec;
    use crate::owner::test_support::{
        FakeHost, Harness, SentClick, add_example_app, frame_image, harness, harness_with,
        scratch_store,
    };
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
        // A missing picture says so before the window's size is checked.
        let gone =
            harness.ask(|owner| owner.find_click(&picture("gone-1.png", (8, 2), (0, 0, 2, 2), 0)));
        assert!(gone.expect_err("missing").contains("cannot be read"));
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

    /// A left click on a spot in the application `app`, for an automation
    /// that watches `watched`.
    fn spot_click(watched: Option<&str>, app: &str, delivery: ClickDeliveryDef) -> ClickSpec {
        ClickSpec {
            watched: watched.map(str::to_owned),
            app: app.to_owned(),
            button: ClickButtonDef::Right,
            delivery,
            return_focus: false,
            at: ClickAtDef::Spot {
                corner: CornerDef::TopRight,
                x: 1,
                y: 0,
            },
        }
    }

    /// Saves `Other App`, with no values, as the application `other`.
    fn add_other_app(harness: &mut Harness) {
        let saved = harness.ask(|owner| {
            owner.save_app(
                None,
                AppSpec {
                    name: "Other App".to_owned(),
                    window: WindowDef::exact(WindowFacts {
                        title: "Other App",
                        class: "OtherWindow",
                        executable: "Other.exe",
                    }),
                    size_changes: SizeChangesDef::Often,
                },
            )
        });
        assert_eq!(saved, Ok("other".to_owned()));
    }

    fn sent(harness: &Harness) -> Vec<SentClick> {
        harness.clicks.try_iter().collect()
    }

    #[test]
    fn a_click_is_sent_as_it_is_set_up_once_both_applications_allow_input() {
        let (stores, root) = scratch_store("click-now");
        let mut harness = harness_with(false, Ok(stores));
        add_example_app(&mut harness);
        add_other_app(&mut harness);
        let message = spot_click(Some("other"), "example", ClickDeliveryDef::Message);
        // A new application's anti-cheat is not set, and each of the two
        // has to allow input, however the click is delivered.
        for (allowed, blocking) in [
            (None, "Other App"),
            (Some("other"), "Example App"),
            (Some("example"), "Other App"),
        ] {
            for app in ["example", "other"] {
                let class = if Some(app) == allowed {
                    AntiCheatDef::None
                } else {
                    AntiCheatDef::Unknown
                };
                harness
                    .ask(|owner| owner.set_anti_cheat(app, class))
                    .expect("set");
            }
            let refused = harness
                .ask(|owner| owner.click_now(&message))
                .expect_err("anti-cheat is not none");
            assert!(
                refused.starts_with(&format!("AppyTappy cannot click while {blocking} runs")),
                "{allowed:?}: {refused}"
            );
            assert_eq!(sent(&harness), [], "{allowed:?}");
        }
        for app in ["example", "other"] {
            harness
                .ask(|owner| owner.set_anti_cheat(app, AntiCheatDef::None))
                .expect("set");
        }
        let example = ClickSent {
            window: 7,
            title: "Example App".to_owned(),
        };
        assert_eq!(
            harness.ask(|owner| owner.click_now(&message)),
            Ok(example.clone()),
            "the answer names the window it went to"
        );
        let spot = Spot::new(Corner::TopRight, 1, 0).expect("valid");
        let window = WindowDef::exact(WindowFacts {
            title: "Example App",
            class: "ExampleWindow",
            executable: "Example.exe",
        })
        .to_match();
        assert_eq!(
            sent(&harness),
            [SentClick {
                app: "Example App".to_owned(),
                window: window.clone(),
                spot: Some(spot),
                button: ClickButton::Right,
                delivery: ClickDelivery::Message,
            }]
        );
        // With no watched application yet, the one clicked in decides, and
        // a click with the cursor keeps where the focus goes after.
        let cursor = spot_click(None, "example", ClickDeliveryDef::Cursor);
        assert_eq!(harness.ask(|owner| owner.click_now(&cursor)), Ok(example));
        assert_eq!(
            sent(&harness),
            [SentClick {
                app: "Example App".to_owned(),
                window,
                spot: Some(spot),
                button: ClickButton::Right,
                delivery: ClickDelivery::Cursor {
                    return_focus: false
                },
            }]
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_click_that_cannot_be_sent_says_why() {
        let (stores, root) = scratch_store("click-now-bad");
        let mut harness = harness_with(false, Ok(stores));
        add_example_app(&mut harness);
        harness
            .ask(|owner| owner.set_anti_cheat("example", AntiCheatDef::None))
            .expect("set");
        let click = |app: &str| spot_click(None, app, ClickDeliveryDef::Message);
        assert_eq!(
            harness.ask(|owner| owner.click_now(&click("gone"))),
            Err("That application is no longer saved.".to_owned())
        );
        assert_eq!(
            harness.ask(|owner| owner.click_now(&click("Not An ID"))),
            Err("A click needs the application it clicks in.".to_owned())
        );
        let gone = spot_click(Some("gone"), "example", ClickDeliveryDef::Message);
        assert_eq!(
            harness.ask(|owner| owner.click_now(&gone)),
            Err("That application is no longer saved.".to_owned()),
            "a watched application that is not saved cannot be checked"
        );
        let mut far = click("example");
        far.at = ClickAtDef::Spot {
            corner: CornerDef::TopLeft,
            x: u32::MAX,
            y: 0,
        };
        assert!(
            harness
                .ask(|owner| owner.click_now(&far))
                .expect_err("too far")
                .contains("16384 pixels")
        );
        let mut pictured = click("example");
        pictured.at = picture("gone-1.png", (4, 2), (0, 0, 2, 2), 0);
        assert!(
            harness
                .ask(|owner| owner.click_now(&pictured))
                .expect_err("no such picture")
                .starts_with("The picture gone-1.png cannot be read: ")
        );
        assert_eq!(sent(&harness), [], "nothing was sent so far");
        harness.owner.host.unclicked = Some("example App has no window open".to_owned());
        assert_eq!(
            harness.ask(|owner| owner.click_now(&click("example"))),
            Err("Example App has no window open.".to_owned())
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_click_on_a_picture_is_sent_with_its_search() {
        let (stores, root) = scratch_store("click-now-picture");
        let mut harness = harness_with(false, Ok(stores));
        add_example_app(&mut harness);
        harness
            .ask(|owner| owner.set_anti_cheat("example", AntiCheatDef::None))
            .expect("set");
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        let kept = harness
            .ask(|owner| owner.add_click_picture(0, region(2, 0, 2, 2), None))
            .expect("kept");
        let mut click = spot_click(None, "example", ClickDeliveryDef::Message);
        click.at = picture(&kept.image, (4, 2), (1, 0, 3, 2), 0);
        assert!(harness.ask(|owner| owner.click_now(&click)).is_ok());
        let sent = sent(&harness);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].spot, None, "it looks for the picture");
        // A picture that does not fit its search area is not sent.
        click.at = picture(&kept.image, (4, 2), (2, 0, 1, 2), 0);
        assert!(
            harness
                .ask(|owner| owner.click_now(&click))
                .expect_err("too narrow")
                .contains("fit inside")
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }
}
