//! Finding more text like a reference's sample on the screenshot it was
//! drawn on, and having the text reader guess what each find says, so the
//! user labels them all at once.

use appytappy_sources::screen::digits::DigitReader;
use appytappy_sources::screen::{Image, Region};
use serde::Serialize;

use crate::check_spec::{self, CheckSpec, ColorSpec, Label, RegionSpec};
use crate::pixels;
use crate::recording::{Text, find, ocr};

use super::Owner;
use super::host::Host;

/// The text a search found, kept until the user adds it or moves on.
#[derive(Debug)]
pub(crate) struct Found {
    /// Its number in the session; the text reader's guesses for an earlier
    /// search are dropped.
    id: u64,
    /// The screenshot it searched.
    screenshot: usize,
    /// Each find's region, pixels, and text.
    items: Vec<Item>,
    /// Finds past the most a search keeps.
    more: usize,
    /// Runs in the characters' color of another size.
    other_sizes: usize,
}

/// One find.
#[derive(Debug)]
struct Item {
    region: Region,
    image: Image,
    text: Text,
}

/// A search as the UI shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct FoundView {
    /// Its number in the session.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) id: u64,
    /// The screenshot it searched.
    pub(crate) screenshot: usize,
    /// Each find, top to bottom and left to right.
    pub(crate) finds: Vec<FindView>,
    /// Finds past the most a search keeps.
    pub(crate) more: usize,
    /// Runs in the characters' color of another size, left out.
    pub(crate) other_sizes: usize,
}

/// One find as the UI shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct FindView {
    /// Where it is on the screenshot.
    pub(crate) region: RegionSpec,
    /// What the text reader made of it.
    pub(crate) text: Text,
}

impl Found {
    fn view(&self) -> FoundView {
        FoundView {
            id: self.id,
            screenshot: self.screenshot,
            finds: self
                .items
                .iter()
                .map(|item| FindView {
                    region: RegionSpec::from(item.region),
                    text: item.text.clone(),
                })
                .collect(),
            more: self.more,
            other_sizes: self.other_sizes,
        }
    }
}

impl<H: Host> Owner<H> {
    /// Finds text like what `region` of screenshot `screenshot` shows,
    /// in the characters' color `ink` within `tolerance` and at its size,
    /// elsewhere on that screenshot and away from `added`, the finds added
    /// already, in place of any earlier search, and sends each find to the
    /// text reader.
    pub(crate) fn find_like(
        &mut self,
        screenshot: usize,
        region: RegionSpec,
        added: &[RegionSpec],
        ink: ColorSpec,
        tolerance: u8,
    ) -> Result<FoundView, String> {
        self.found = None;
        let image = self
            .screenshots
            .get(screenshot)
            .ok_or_else(|| "Take a screenshot first.".to_owned())?;
        let region = Region::new(region.x, region.y, region.width, region.height)
            .ok_or_else(|| "Draw the region around the first sample first.".to_owned())?;
        if !region.fits(image.size()) {
            return Err(
                "The region reaches past the edge of the window. Draw it inside the window."
                    .to_owned(),
            );
        }
        let reader = DigitReader::new(ink.into(), tolerance);
        let added: Vec<Region> = added
            .iter()
            .filter_map(|spec| Region::new(spec.x, spec.y, spec.width, spec.height))
            .collect();
        let search = find::like(image, region, &added, &reader).ok_or_else(|| {
            "The region shows no characters in that color. Pick the characters' color first."
                .to_owned()
        })?;
        self.search_id = self.search_id.wrapping_add(1);
        let id = self.search_id;
        let mut items = Vec::with_capacity(search.finds.len());
        for found in search.finds {
            let Some(cropped) = image.crop(found.region) else {
                continue;
            };
            // Its place among the finds kept, which the reader's guess names.
            let index = items.len();
            let text = match found.line {
                Some(line) => {
                    let job = ocr::Job {
                        source: ocr::Source::Search(id),
                        index,
                        line,
                    };
                    if self.ocr.send(job).is_ok() {
                        Text::Pending
                    } else {
                        Text::Failed("The text reader stopped. Type what this shows.".to_owned())
                    }
                },
                None => Text::Failed("Type what this shows.".to_owned()),
            };
            items.push(Item {
                region: found.region,
                image: cropped,
                text,
            });
        }
        let found = Found {
            id,
            screenshot,
            items,
            more: search.more,
            other_sizes: search.other_sizes,
        };
        let view = found.view();
        self.found = Some(found);
        Ok(view)
    }

    /// Keeps the text reader's guess for find `index` of search `search`,
    /// unless the user moved on from it.
    pub(super) fn found_text(&mut self, search: u64, index: usize, text: Text) {
        if let Some(item) = self
            .found
            .as_mut()
            .filter(|found| found.id == search)
            .and_then(|found| found.items.get_mut(index))
        {
            item.text = text;
        }
    }

    /// The search as the UI shows it, or `None` without one.
    pub(crate) fn found_state(&self) -> Option<FoundView> {
        self.found.as_ref().map(Found::view)
    }

    /// Find `index`'s pixels of search `search`, at most `max_width` wide;
    /// `None` when that search was replaced or forgotten.
    pub(crate) fn found_pixels(
        &self,
        search: u64,
        index: usize,
        max_width: u32,
    ) -> Option<Vec<u8>> {
        self.found
            .as_ref()
            .filter(|found| found.id == search)
            .and_then(|found| found.items.get(index))
            .map(|item| pixels::rgba(&item.image, max_width))
    }

    /// Forgets the search.
    pub(crate) fn forget_found(&mut self) {
        self.found = None;
    }

    /// For each of `found`, why the reference `spec` teaches cannot learn
    /// it, or `None` when it can; an error when `spec`'s own labels cannot
    /// be learned.
    pub(crate) fn found_problems(
        &self,
        spec: &CheckSpec,
        found: &[Label],
    ) -> Result<Vec<Option<String>>, String> {
        self.with_session(|session| check_spec::label_problems(spec, session, found))
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use appytappy_sources::screen::{FrameHandler, Size};

    use crate::owner::messages::Message;
    use crate::owner::test_support::{FakeHost, Harness, harness};

    use super::*;

    const INK: [u8; 4] = [0, 0, 255, 0];
    const PAPER: [u8; 4] = [40, 40, 40, 0];

    /// A 20 by 12 frame with two 2 by 3 marks of ink on its top line, at x
    /// 1 and 8, and one 2 by 3 mark on the line below, at x 1.
    fn marks() -> Image {
        let pixels = (0..12_u32)
            .flat_map(|y| {
                (0..20_u32).flat_map(move |x| {
                    let top = (1..4).contains(&y) && ((1..3).contains(&x) || (8..10).contains(&x));
                    let below = (7..10).contains(&y) && (1..3).contains(&x);
                    if top || below { INK } else { PAPER }
                })
            })
            .collect();
        Image::new(
            Size {
                width: 20,
                height: 12,
            },
            pixels,
        )
        .expect("matching length")
    }

    fn region(x: u32, y: u32, width: u32, height: u32) -> RegionSpec {
        serde_json::from_str(&format!(
            r#"{{"x":{x},"y":{y},"width":{width},"height":{height}}}"#
        ))
        .expect("valid JSON")
    }

    fn red() -> ColorSpec {
        serde_json::from_str(r#"{"red":255,"green":0,"blue":0}"#).expect("valid JSON")
    }

    /// A harness with screenshot 0 of [`marks`].
    fn searchable() -> (Harness, FrameHandler) {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&marks()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        (harness, handler)
    }

    fn find_like(
        region: RegionSpec,
    ) -> impl FnOnce(&mut Owner<FakeHost>) -> Result<FoundView, String> {
        move |owner: &mut Owner<FakeHost>| owner.find_like(0, region, &[], red(), 0)
    }

    #[test]
    fn a_search_finds_the_other_marks_and_has_each_read() {
        let (mut harness, _handler) = searchable();
        let found = harness.ask(find_like(region(0, 0, 4, 5))).expect("found");
        assert_eq!(found.id, 1);
        assert_eq!(found.screenshot, 0);
        let regions: Vec<RegionSpec> = found.finds.iter().map(|find| find.region).collect();
        assert_eq!(
            regions,
            [region(7, 0, 4, 5), region(0, 6, 4, 5)],
            "top to bottom then left to right, each with a margin"
        );
        assert!(found.finds.iter().all(|find| find.text == Text::Pending));
        let jobs: Vec<(ocr::Source, usize)> = harness
            .jobs
            .try_iter()
            .map(|job| (job.source, job.index))
            .collect();
        assert_eq!(
            jobs,
            [(ocr::Source::Search(1), 0), (ocr::Source::Search(1), 1)]
        );
        for (search, text) in [(0, "stale"), (1, "7")] {
            assert!(
                harness
                    .owner
                    .handle(Message::FoundText {
                        search,
                        index: 1,
                        text: Text::Read(text.to_owned()),
                    })
                    .is_continue()
            );
        }
        let state = harness.ask(|owner| owner.found_state()).expect("kept");
        assert_eq!(state.finds[0].text, Text::Pending);
        assert_eq!(state.finds[1].text, Text::Read("7".to_owned()));
        let pixels_of = |search, index| {
            move |owner: &mut Owner<FakeHost>| owner.found_pixels(search, index, 20)
        };
        let crop = marks()
            .crop(Region::new(0, 6, 4, 5).expect("not empty"))
            .expect("inside");
        assert_eq!(harness.ask(pixels_of(1, 1)), Some(pixels::rgba(&crop, 20)));
        assert_eq!(harness.ask(pixels_of(1, 2)), None);
        assert_eq!(harness.ask(pixels_of(0, 1)), None, "an earlier search's");
    }

    #[test]
    fn a_search_leaves_out_the_finds_added_already() {
        let (mut harness, _handler) = searchable();
        let found = harness
            .ask(move |owner: &mut Owner<FakeHost>| {
                owner.find_like(0, region(0, 0, 4, 5), &[region(7, 0, 4, 5)], red(), 0)
            })
            .expect("found");
        let regions: Vec<RegionSpec> = found.finds.iter().map(|find| find.region).collect();
        assert_eq!(regions, [region(0, 6, 4, 5)]);
        let indexes: Vec<usize> = harness.jobs.try_iter().map(|job| job.index).collect();
        assert_eq!(indexes, [0]);
    }

    #[test]
    fn a_new_search_replaces_the_old_and_moving_on_forgets_it() {
        let (mut harness, _handler) = searchable();
        harness.ask(find_like(region(0, 0, 4, 5))).expect("found");
        let again = harness.ask(find_like(region(0, 0, 4, 5))).expect("found");
        assert_eq!(again.id, 2);
        assert!(
            harness
                .owner
                .handle(Message::FoundText {
                    search: 1,
                    index: 0,
                    text: Text::Read("1".to_owned()),
                })
                .is_continue()
        );
        assert_eq!(
            harness
                .ask(|owner| owner.found_state())
                .expect("kept")
                .finds[0]
                .text,
            Text::Pending,
            "the old search's text is dropped"
        );
        harness.ask(Owner::forget_found);
        assert_eq!(harness.ask(|owner| owner.found_state()), None);
        harness.ask(find_like(region(0, 0, 4, 5))).expect("found");
        let _next = harness.pick();
        assert_eq!(
            harness.ask(|owner| owner.found_state()),
            None,
            "another window drops it"
        );
    }

    #[test]
    fn a_search_needs_a_screenshot_and_ink_in_a_region_inside_it() {
        let mut harness = harness(false);
        let _handler = harness.pick();
        assert_eq!(
            harness.ask(find_like(region(0, 0, 4, 5))),
            Err("Take a screenshot first.".to_owned())
        );
        let (mut harness, _handler) = searchable();
        assert_eq!(
            harness.ask(find_like(region(0, 0, 0, 5))),
            Err("Draw the region around the first sample first.".to_owned())
        );
        assert_eq!(
            harness.ask(find_like(region(18, 0, 4, 5))),
            Err(
                "The region reaches past the edge of the window. Draw it inside the window."
                    .to_owned()
            )
        );
        assert_eq!(
            harness.ask(find_like(region(14, 0, 4, 5))),
            Err(
                "The region shows no characters in that color. Pick the characters' color first."
                    .to_owned()
            )
        );
        harness.ask(find_like(region(0, 0, 4, 5))).expect("found");
        assert!(
            harness.ask(find_like(region(14, 0, 4, 5))).is_err(),
            "a failed search"
        );
        assert_eq!(
            harness.ask(|owner| owner.found_state()),
            None,
            "a failed search leaves no earlier one behind"
        );
    }

    #[test]
    fn a_find_the_reader_cannot_take_says_to_type_it() {
        let (mut harness, _handler) = searchable();
        let (closed, _) = std::sync::mpsc::channel();
        harness.owner.ocr = closed;
        let found = harness.ask(find_like(region(0, 0, 4, 5))).expect("found");
        assert_eq!(
            found.finds[0].text,
            Text::Failed("The text reader stopped. Type what this shows.".to_owned())
        );
    }

    #[test]
    fn found_problems_judge_each_find_on_the_ones_before_it() {
        let (mut harness, _handler) = searchable();
        let spec: CheckSpec = serde_json::from_str(
            r#"{"kind":"characters","region":{"x":0,"y":0,"width":4,"height":5},"screenshot":0,
                "ink":{"red":255,"green":0,"blue":0},"tolerance":0,
                "labels":[{"screenshot":0,"text":"1"}],"recorded":[]}"#,
        )
        .expect("valid JSON");
        let label = |x: u32, y: u32, text: &str| Label {
            screenshot: 0,
            region: Some(region(x, y, 4, 5)),
            text: text.to_owned(),
        };
        let found = [
            label(7, 0, "12"),
            label(7, 0, "1"),
            label(18, 0, "1"),
            label(0, 6, "1"),
        ];
        let problems = harness
            .ask(move |owner| owner.found_problems(&spec, &found))
            .expect("the label is learned");
        assert_eq!(
            problems[0].as_deref(),
            Some(
                "The region shows 1 characters but the text has 2; characters that touch count \
                 as one."
            )
        );
        assert_eq!(problems[1], None);
        assert_eq!(
            problems[2].as_deref(),
            Some("The region reaches past the edge of the window. Draw it inside the window.")
        );
        assert_eq!(problems[3], None);
    }
}
