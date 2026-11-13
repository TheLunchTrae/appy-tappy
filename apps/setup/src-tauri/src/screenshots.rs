//! The session's screenshots: whole frames of the picked window, kept in
//! memory from when each is taken or recorded until it is removed or the
//! capture ends. Only a draft writes them to disk, in its own folder
//! ([`crate::drafts`]). The session remembers which of them a recording of
//! looks kept, so a value set up on one says that a recording made it. A
//! draft does not keep that: a restored screenshot counts as taken by hand.

use std::collections::BTreeSet;

use appytappy_sources::screen::Image;

/// Where a check the UI describes finds the screenshots it names by index.
pub(crate) trait Shots {
    /// Screenshot `index`, or `None` when there never was one or it was
    /// removed.
    fn get(&self, index: usize) -> Option<&Image>;

    /// Whether a recording of looks kept screenshot `index`, and the user
    /// did not take it.
    fn is_recorded(&self, index: usize) -> bool;
}

/// The session's screenshots, each named by the index it got when it was
/// kept. Removing one releases its pixels and leaves its index empty, so
/// the others keep theirs and what the UI set up on them stays right.
#[derive(Debug)]
pub(crate) struct Screenshots {
    /// Every index given out so far, in order.
    slots: Vec<Option<Image>>,
    /// The indexes a recording of looks kept.
    recorded: BTreeSet<usize>,
}

impl Screenshots {
    /// No screenshots.
    pub(crate) const fn new() -> Self {
        Self {
            slots: Vec::new(),
            recorded: BTreeSet::new(),
        }
    }

    /// Keeps `image`, which the user took, and answers its index.
    pub(crate) fn push(&mut self, image: Image) -> usize {
        self.slots.push(Some(image));
        self.slots.len().saturating_sub(1)
    }

    /// Keeps `image`, which a recording of looks kept, and answers its
    /// index.
    pub(crate) fn push_recorded(&mut self, image: Image) -> usize {
        let index = self.push(image);
        self.recorded.insert(index);
        index
    }

    /// Screenshot `index`, or `None` when there never was one or it was
    /// removed.
    pub(crate) fn get(&self, index: usize) -> Option<&Image> {
        self.slots.get(index).and_then(Option::as_ref)
    }

    /// Releases screenshot `index`, if there is one.
    pub(crate) fn remove(&mut self, index: usize) {
        if let Some(slot) = self.slots.get_mut(index) {
            *slot = None;
        }
    }

    /// The screenshots a draft kept, each under the index it had when the
    /// draft was saved; the indexes between them are empty, as after a
    /// removal.
    pub(crate) fn restored(kept: Vec<(usize, Image)>) -> Self {
        let mut slots = Vec::new();
        for (index, image) in kept {
            if slots.len() <= index {
                slots.resize_with(index.saturating_add(1), || None);
            }
            if let Some(slot) = slots.get_mut(index) {
                *slot = Some(image);
            }
        }
        Self {
            slots,
            recorded: BTreeSet::new(),
        }
    }

    /// Every index given out so far, in order: the screenshot kept under
    /// it, or `None` for one that was removed.
    pub(crate) fn slots(&self) -> &[Option<Image>] {
        &self.slots
    }

    /// The screenshots still kept, in the order they were kept.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &Image> {
        self.slots.iter().flatten()
    }
}

impl Shots for Screenshots {
    fn get(&self, index: usize) -> Option<&Image> {
        Self::get(self, index)
    }

    fn is_recorded(&self, index: usize) -> bool {
        self.recorded.contains(&index)
    }
}

#[cfg(test)]
mod tests {
    use appytappy_sources::screen::Size;

    use super::*;

    /// A one-pixel image of `shade`.
    fn pixel(shade: u8) -> Image {
        Image::new(
            Size {
                width: 1,
                height: 1,
            },
            vec![shade, shade, shade, 0],
        )
        .expect("matching length")
    }

    #[test]
    fn a_removed_screenshot_leaves_the_others_their_indexes() {
        let mut screenshots = Screenshots::new();
        assert_eq!(screenshots.push(pixel(1)), 0);
        assert_eq!(screenshots.push(pixel(2)), 1);
        assert_eq!(screenshots.push(pixel(3)), 2);
        screenshots.remove(1);
        screenshots.remove(9);
        assert_eq!(screenshots.get(0), Some(&pixel(1)));
        assert_eq!(screenshots.get(1), None, "its pixels are released");
        assert_eq!(Shots::get(&screenshots, 2), Some(&pixel(3)));
        assert_eq!(
            screenshots.iter().collect::<Vec<_>>(),
            [&pixel(1), &pixel(3)]
        );
        assert_eq!(
            screenshots.push(pixel(4)),
            3,
            "a removed index is not reused"
        );
    }

    #[test]
    fn only_a_recorded_screenshot_counts_as_recorded() {
        let mut screenshots = Screenshots::new();
        assert_eq!(screenshots.push(pixel(1)), 0);
        assert_eq!(screenshots.push_recorded(pixel(2)), 1);
        assert_eq!(screenshots.get(1), Some(&pixel(2)));
        assert!(!screenshots.is_recorded(0));
        assert!(screenshots.is_recorded(1));
        assert!(!screenshots.is_recorded(2), "there is no such screenshot");
    }

    #[test]
    fn restored_screenshots_keep_their_indexes() {
        let mut screenshots = Screenshots::restored(vec![(2, pixel(3)), (0, pixel(1))]);
        assert_eq!(screenshots.get(0), Some(&pixel(1)));
        assert_eq!(screenshots.get(1), None);
        assert_eq!(screenshots.get(2), Some(&pixel(3)));
        assert_eq!(screenshots.push(pixel(4)), 3, "the next takes a new index");
        let packed = Screenshots::restored(vec![(0, pixel(1)), (1, pixel(2))]);
        assert_eq!(packed.slots(), [Some(pixel(1)), Some(pixel(2))]);
        assert_eq!(Screenshots::restored(Vec::new()).slots(), []);
    }
}
