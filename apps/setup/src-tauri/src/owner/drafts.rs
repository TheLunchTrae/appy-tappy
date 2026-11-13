//! Drafts: listing, saving, and deleting them, and taking a value's or a
//! reference's screenshots back from its draft.

use appytappy_sources::screen::{Image, ImageError};

use crate::drafts::{DraftDef, DraftShot, DraftSummary};
use crate::screenshots::Screenshots;

use super::Owner;
use super::host::Host;

/// What a draft says is still to do when a screenshot was too large to keep.
const LEFT_OUT: &str =
    "A screenshot was larger than AppyTappy keeps, so make the window smaller and take it again.";

impl<H: Host> Owner<H> {
    /// Every saved draft.
    pub(crate) fn list_drafts(&self) -> Result<Vec<DraftSummary>, String> {
        self.stores()?
            .drafts
            .list()
            .map_err(|error| error.to_string())
    }

    /// Saves `draft` over the draft `id`, or as a new one, and answers its
    /// ID. With `screenshots`, it keeps this session's screenshots with it,
    /// and which of them a recording of looks kept, so the flow that
    /// finishes it can draw on them again. A screenshot
    /// larger than AppyTappy reads back is left out, and the draft says so
    /// with what is still to do.
    pub(crate) fn save_draft(
        &self,
        id: Option<&str>,
        draft: &DraftDef,
        screenshots: bool,
    ) -> Result<String, String> {
        let stores = self.stores()?;
        let mut pictures = Vec::new();
        let mut left_out = false;
        let kept = self.capture.screenshots().kept().filter(|_| screenshots);
        for (shot, image) in kept {
            match image.encode_png() {
                Ok(bytes) => pictures.push((shot, bytes)),
                Err(ImageError::TooLarge { .. }) => left_out = true,
                Err(error) => return Err(format!("A screenshot could not be kept ({error}).")),
            }
        }
        let mut kept = draft.clone();
        if left_out {
            kept.missing.push(LEFT_OUT.to_owned());
        }
        stores
            .drafts
            .save(id, &kept, &pictures)
            .map_err(|error| error.to_string())
    }

    /// Makes the screenshots of the draft `id` this session's, in place of
    /// any it took, each under the index it had when the draft was saved and
    /// marked recorded if a recording of looks had kept it. Answers each,
    /// lowest index first.
    pub(crate) fn restore_draft_screenshots(&mut self, id: &str) -> Result<Vec<DraftShot>, String> {
        let screenshots = self
            .stores()?
            .drafts
            .screenshots(id)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|(shot, bytes)| Ok((shot, Image::decode_png(&bytes)?)))
            .collect::<Result<Vec<_>, ImageError>>()
            .map_err(|error| {
                format!(
                    "A screenshot of the draft cannot be read: {error}. Cancel, then delete the \
                     draft on its page to start again."
                )
            })?;
        let picked = self
            .capture
            .picked_mut()
            .ok_or_else(|| "Pick the window first.".to_owned())?;
        let shots = screenshots.iter().map(|(shot, _)| *shot).collect();
        picked.screenshots = Screenshots::restored(screenshots);
        Ok(shots)
    }

    /// Deletes the draft `id`.
    pub(crate) fn delete_draft(&self, id: &str) -> Result<(), String> {
        self.stores()?
            .drafts
            .delete(id)
            .map_err(|error| error.to_string())
    }

    /// Deletes the drafts that `gone` takes, after what they belong to was
    /// removed. The removal worked either way: a draft that stays says on
    /// its page that it can only be deleted.
    pub(super) fn delete_drafts(&self, gone: impl Fn(&DraftDef) -> bool) {
        if let Ok(stores) = self.stores() {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the item is removed; a draft that stays can be deleted on its page"
            )]
            let _ = stores.drafts.delete_where(gone);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use appytappy_config::app_file::OriginDef;
    use appytappy_sources::screen::Size;
    use serde_json::json;

    use super::{Image, LEFT_OUT};
    use crate::drafts::{DraftDef, DraftKind, DraftShot};
    use crate::owner::Owner;
    use crate::owner::test_support::{
        BAR, NO_STORE, capturing, frame_image, harness, harness_with, scratch_store, spec,
    };
    use crate::pixels;

    fn value_draft() -> DraftDef {
        DraftDef {
            kind: DraftKind::Value,
            app: Some("example".to_owned()),
            changes: None,
            name: "Progress".to_owned(),
            missing: vec!["Pick the filled color".to_owned()],
            pictures: Vec::new(),
            state: json!({ "format": 1 }),
        }
    }

    #[test]
    fn a_draft_keeps_the_screenshots_and_gives_them_back_in_order() {
        let (stores, root) = scratch_store("owner-drafts");
        let mut harness = harness_with(false, Ok(stores));
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        assert_eq!(harness.ask(Owner::screenshot), Ok(1));
        assert_eq!(harness.ask(Owner::screenshot), Ok(2));
        // A removed screenshot leaves its index empty, in the draft too.
        assert_eq!(harness.ask(|owner| owner.remove_screenshots(&[1])), Ok(()));
        let id = harness
            .ask(|owner| owner.save_draft(None, &value_draft(), true))
            .expect("saves");
        let without = harness
            .ask(|owner| owner.save_draft(None, &value_draft(), false))
            .expect("saves");
        let listed = harness.ask(|owner| owner.list_drafts()).expect("lists");
        let counts: Vec<(String, u32)> = listed
            .into_iter()
            .map(|summary| (summary.id, summary.screenshots))
            .collect();
        assert_eq!(counts, [(id.clone(), 2), (without.clone(), 0)]);

        // A new window forgets the session's screenshots; the draft brings them back.
        let _next = harness.pick();
        assert_eq!(harness.ask(|owner| owner.screenshot_pixels(0, 4)), None);
        assert_eq!(
            harness.ask(|owner| owner.restore_draft_screenshots(&id)),
            Ok(vec![
                DraftShot {
                    index: 0,
                    recorded: false
                },
                DraftShot {
                    index: 2,
                    recorded: false
                },
            ])
        );
        for index in [0, 2] {
            assert_eq!(
                harness.ask(|owner| owner.screenshot_pixels(index, 4)),
                Some(pixels::rgba(&frame_image(), 4))
            );
        }
        assert_eq!(harness.ask(|owner| owner.screenshot_pixels(1, 4)), None);
        assert_eq!(
            harness.ask(|owner| owner.restore_draft_screenshots(&without)),
            Ok(Vec::new()),
            "a draft without screenshots leaves none"
        );

        harness
            .ask(|owner| owner.delete_draft(&id))
            .expect("deletes");
        let left = harness.ask(|owner| owner.list_drafts()).expect("lists");
        assert_eq!(left.len(), 1);
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_draft_brings_back_which_screenshots_a_recording_of_looks_kept() {
        let (mut harness, mut handler, root) = capturing("owner-drafts-recorded");
        let region = serde_json::from_str(r#"{"x":2,"y":0,"width":2,"height":2}"#).expect("JSON");
        harness
            .ask(|owner| owner.start_look_recording(region, 0, 0))
            .expect("records");
        // The right half is red here and gray on screenshot 0, so the
        // recording keeps this frame as screenshot 1.
        let red = Image::new(
            Size {
                width: 4,
                height: 2,
            },
            [0, 0, 255, 0].repeat(8),
        )
        .expect("matching length");
        harness.deliver(&mut handler, Ok(&red));
        harness.ask(Owner::stop_recording);
        let id = harness
            .ask(|owner| owner.save_draft(None, &value_draft(), true))
            .expect("saves");

        // A new window forgets the session's screenshots; the draft brings
        // them back with the mark.
        let _next = harness.pick();
        assert_eq!(
            harness.ask(|owner| owner.restore_draft_screenshots(&id)),
            Ok(vec![
                DraftShot {
                    index: 0,
                    recorded: false
                },
                DraftShot {
                    index: 1,
                    recorded: true
                },
            ])
        );
        let mut on_recorded = spec("example", "Progress bar", BAR);
        on_recorded.screenshot = 1;
        harness
            .ask(|owner| owner.save(&on_recorded))
            .expect("saves");
        let saved = harness
            .owner
            .stores()
            .expect("stores")
            .app("example")
            .expect("saved")
            .file;
        assert_eq!(saved.values[0].origin, OriginDef::Recording);
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_screenshot_too_large_to_keep_is_left_out_and_the_draft_says_so() {
        let (stores, root) = scratch_store("owner-drafts-large");
        let mut harness = harness_with(false, Ok(stores));
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        // One pixel wider than AppyTappy reads.
        let wide = Size {
            width: 8193,
            height: 1,
        };
        let large = Image::new(wide, vec![0; 32_772]).expect("matching length");
        harness.deliver(&mut handler, Ok(&large));
        assert_eq!(harness.ask(Owner::screenshot), Ok(1));
        let id = harness
            .ask(|owner| owner.save_draft(None, &value_draft(), true))
            .expect("saves without it");
        let listed = harness.ask(|owner| owner.list_drafts()).expect("lists");
        let [summary] = listed.as_slice() else {
            panic!("one draft");
        };
        assert_eq!((summary.id.as_str(), summary.screenshots), (id.as_str(), 1));
        assert_eq!(
            summary.draft.as_ref().expect("reads").missing,
            ["Pick the filled color", LEFT_OUT]
        );
        let kept = harness
            .ask(|owner| owner.save_draft(None, &value_draft(), false))
            .expect("saves");
        let listed = harness.ask(|owner| owner.list_drafts()).expect("lists");
        let fresh = listed.iter().find(|each| each.id == kept).expect("listed");
        assert_eq!(
            fresh.draft.as_ref().expect("reads").missing,
            ["Pick the filled color"],
            "without screenshots, nothing is left out"
        );
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_broken_screenshot_is_refused_and_keeps_the_session_s() {
        let (stores, root) = scratch_store("owner-drafts-broken");
        let folder = root.join("AppyTappy").join("drafts");
        let mut harness = harness_with(false, Ok(stores));
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        let id = harness
            .ask(|owner| owner.save_draft(None, &value_draft(), true))
            .expect("saves");
        fs::write(folder.join(&id).join("shot-0.png"), b"not a picture").expect("breaks it");
        assert!(
            harness
                .ask(|owner| owner.restore_draft_screenshots(&id))
                .is_err_and(|error| error.starts_with("A screenshot of the draft cannot be read")),
        );
        assert!(
            harness.ask(|owner| owner.screenshot_pixels(0, 4)).is_some(),
            "the session keeps its own"
        );
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn without_a_store_each_says_why() {
        let mut harness = harness(false);
        assert_eq!(
            harness.ask(|owner| owner.list_drafts()),
            Err(NO_STORE.to_owned())
        );
        assert_eq!(
            harness.ask(|owner| owner.save_draft(None, &value_draft(), false)),
            Err(NO_STORE.to_owned())
        );
        assert_eq!(
            harness.ask(|owner| owner.restore_draft_screenshots("a")),
            Err(NO_STORE.to_owned())
        );
        assert_eq!(
            harness.ask(|owner| owner.delete_draft("a")),
            Err(NO_STORE.to_owned())
        );
    }
}
