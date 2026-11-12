//! Drafts: listing, saving, and deleting them, and taking a value's or a
//! reference's screenshots back from its draft.

use appytappy_sources::screen::{Image, ImageError};

use crate::drafts::{DraftDef, DraftSummary};
use crate::screenshots::Screenshots;

use super::Owner;
use super::host::Host;

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
    /// so the flow that finishes it can draw on them again.
    pub(crate) fn save_draft(
        &self,
        id: Option<&str>,
        draft: &DraftDef,
        screenshots: bool,
    ) -> Result<String, String> {
        let stores = self.stores()?;
        let pictures = if screenshots {
            self.screenshots
                .slots()
                .iter()
                .enumerate()
                .filter_map(|(index, slot)| Some((index, slot.as_ref()?)))
                .map(|(index, image)| Ok((index, image.encode_png()?)))
                .collect::<Result<Vec<_>, ImageError>>()
                .map_err(|error| format!("A screenshot could not be kept: {error}."))?
        } else {
            Vec::new()
        };
        stores
            .drafts
            .save(id, draft, &pictures)
            .map_err(|error| error.to_string())
    }

    /// Makes the screenshots of the draft `id` this session's, in place of
    /// any it took, each under the index it had when the draft was saved.
    /// Answers those indexes, lowest first.
    pub(crate) fn restore_draft_screenshots(&mut self, id: &str) -> Result<Vec<usize>, String> {
        let screenshots = self
            .stores()?
            .drafts
            .screenshots(id)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|(index, bytes)| Ok((index, Image::decode_png(&bytes)?)))
            .collect::<Result<Vec<_>, ImageError>>()
            .map_err(|error| format!("A screenshot of the draft cannot be read: {error}."))?;
        let indexes = screenshots.iter().map(|(index, _)| *index).collect();
        self.screenshots = Screenshots::restored(screenshots);
        Ok(indexes)
    }

    /// Deletes the draft `id`.
    pub(crate) fn delete_draft(&self, id: &str) -> Result<(), String> {
        self.stores()?
            .drafts
            .delete(id)
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use crate::drafts::{DraftDef, DraftKind};
    use crate::owner::Owner;
    use crate::owner::test_support::{NO_STORE, frame_image, harness, harness_with, scratch_store};
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
            Ok(vec![0, 2])
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
