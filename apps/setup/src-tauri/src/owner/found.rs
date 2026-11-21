//! Finding more text like one of a reference's boxes on the screenshot the
//! box is on, and having the text reader guess what each find says, so the
//! user only confirms them.

use std::collections::BTreeSet;

use appytappy_sources::screen::digits::{DigitReader, InkRule};
use appytappy_sources::screen::{Image, Region};
use serde::Serialize;

use appytappy_config::app_file::{ColorDef, RegionDef};

use crate::check_spec::{self, CharactersSpec, Label};
use crate::pixels;
use crate::recording::character::{self, CharacterView};
use crate::recording::{Line, Text, find, ocr};
use crate::ticket::Ticket;

use super::Owner;
use super::host::Host;

/// The text a search found, kept until the user adds it or moves on.
#[derive(Debug)]
pub(super) struct Found {
    /// Its number in the session; the text reader's guesses for an earlier
    /// search are dropped.
    pub(super) id: Ticket,
    /// The screenshot it searched.
    screenshot: usize,
    /// Each find's region, pixels, and text.
    items: Vec<Item>,
    /// Finds past the most a search keeps.
    more: usize,
    /// Finds left out for being at a region the user passed over.
    passed: usize,
}

/// One find.
#[derive(Debug)]
struct Item {
    region: Region,
    /// Its region of the screenshot with the pixels around it, one on each
    /// side the screenshot has one: all that any reference is given of its
    /// region ([`DigitReader::cut`]), kept so a reread does not need the
    /// screenshot.
    around: Image,
    /// Where its region is in `around`.
    within: Region,
    text: Text,
    /// How tall its characters are, in pixels, when that is another size
    /// than the sample's.
    other_size: Option<u32>,
    /// The text color, if any, and tolerance its ink was last cut with for
    /// the text reader.
    read_with: (Option<ColorDef>, u8),
    /// Where each of its characters is in its pixels, as that color and
    /// tolerance split them.
    characters: Vec<CharacterView>,
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
    /// Each find: those of the sample's size first, then each other size
    /// from the nearest to the sample's, and each size's top to bottom and
    /// left to right.
    pub(crate) finds: Vec<FindView>,
    /// Finds past the most a search keeps.
    pub(crate) more: usize,
    /// Finds left out for being at a region the user passed over.
    pub(crate) passed: usize,
}

/// One find as the UI shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct FindView {
    /// Where it is on the screenshot.
    pub(crate) region: RegionDef,
    /// What the text reader made of it.
    pub(crate) text: Text,
    /// How tall its characters are, in pixels, when that is another size
    /// than the sample's.
    #[cfg_attr(test, ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) other_size: Option<u32>,
    /// The reference's text color its ink was last cut with for the text
    /// reader, which reads the same ink as the same text, or `None` when
    /// it was cut by contrast.
    #[cfg_attr(test, ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ink: Option<ColorDef>,
    /// The tolerance its ink was last cut with.
    pub(crate) tolerance: u8,
    /// Where each of its characters is in its region, left to right, as
    /// that color and tolerance split them, with a name for its shape that
    /// a recorded reading's character of the same shape shares.
    pub(crate) characters: Vec<CharacterView>,
}

/// The characters `region` of `image` shows, as `reader` splits them in
/// the pixels it is given of the region ([`DigitReader::cut`]), each where
/// it is in the region; `None` when the region reaches outside `image`. A
/// box's characters and a find's are both these, so a find shows the
/// characters the box it becomes does.
fn characters_in(
    image: &Image,
    region: Region,
    reader: &DigitReader,
) -> Option<Vec<CharacterView>> {
    let read = reader.reads(region, image.size());
    // The page marks each character on the region itself.
    let around = region.x().saturating_sub(read.x());
    let characters = character::of(&image.crop(read)?, reader);
    Some(
        characters
            .into_iter()
            .map(|character| CharacterView {
                left: character.left.saturating_sub(around),
                right: character.right.saturating_sub(around),
                ..character
            })
            .collect(),
    )
}

/// `region` of `image` with the pixels around it, one on each side `image`
/// has one, and where `region` is in them; `None` when it reaches outside
/// `image`. Those are the pixels a reference that finds its characters by
/// contrast is given of the region, the most any is, so each reader is
/// given of the region in them what it is given of it in `image`.
fn with_pixels_around(image: &Image, region: Region) -> Option<(Image, Region)> {
    let read = InkRule::Contrast { tolerance: 0 }.reads(region, image.size());
    let size = region.size();
    let within = Region::new(
        region.x().saturating_sub(read.x()),
        region.y().saturating_sub(read.y()),
        size.width,
        size.height,
    )?;
    Some((image.crop(read)?, within))
}

impl Found {
    fn view(&self) -> FoundView {
        FoundView {
            id: self.id.number(),
            screenshot: self.screenshot,
            finds: self
                .items
                .iter()
                .map(|item| FindView {
                    region: RegionDef::from(item.region),
                    text: item.text.clone(),
                    other_size: item.other_size,
                    ink: item.read_with.0,
                    tolerance: item.read_with.1,
                    characters: item.characters.clone(),
                })
                .collect(),
            more: self.more,
            passed: self.passed,
        }
    }
}

impl<H: Host> Owner<H> {
    /// Finds text like what `region` of screenshot `screenshot` shows, as
    /// a reference with the text color `ink`, if any, and `tolerance`
    /// tells it from the region's background, at any size,
    /// elsewhere on that screenshot and away from `added`, the finds added
    /// already, and from `passed`, the finds an earlier search offered that
    /// the user left out, in place of any earlier search, and sends each
    /// find to the text reader.
    pub(crate) fn find_like(
        &mut self,
        screenshot: usize,
        region: RegionDef,
        added: &[RegionDef],
        passed: &[RegionDef],
        ink: Option<ColorDef>,
        tolerance: u8,
    ) -> Result<FoundView, String> {
        self.forget_found();
        let image = self
            .capture
            .screenshots()
            .get(screenshot)
            .ok_or_else(|| "Take a screenshot first.".to_owned())?;
        let region = Region::try_from(region)
            .map_err(|_empty| "Draw the box around some characters first.".to_owned())?;
        if !region.fits(image.size()) {
            return Err(
                "The box reaches past the edge of the window. Draw it inside the window."
                    .to_owned(),
            );
        }
        let reader = DigitReader::new(check_spec::ink_rule(ink, tolerance));
        let regions = |specs: &[RegionDef]| -> Vec<Region> {
            specs
                .iter()
                .filter_map(|&spec| Region::try_from(spec).ok())
                .collect()
        };
        let search = find::like(image, region, &regions(added), &regions(passed), &reader)
            .ok_or_else(|| {
                "The box shows no characters. Draw it around some characters first.".to_owned()
            })?;
        let id = self.tickets.next();
        let mut items = Vec::with_capacity(search.finds.len());
        for found in search.finds {
            let Some((around, within)) = with_pixels_around(image, found.region) else {
                continue;
            };
            // Its place among the finds kept, which the reader's guess names.
            let text = self.read(id, items.len(), found.line);
            items.push(Item {
                region: found.region,
                characters: characters_in(&around, within, &reader).unwrap_or_default(),
                around,
                within,
                text,
                other_size: found.other_size,
                read_with: (ink, tolerance),
            });
        }
        let found = Found {
            id,
            screenshot,
            items,
            more: search.more,
            passed: search.passed,
        };
        let view = found.view();
        if let Some(picked) = self.capture.picked_mut() {
            picked.found = Some(found);
        }
        Ok(view)
    }

    /// Sends `line`, the ink of find `index` of search `search`, to the text
    /// reader, and answers what the find's text is until the reader does.
    fn read(&self, search: Ticket, index: usize, line: Option<Line>) -> Text {
        let Some(line) = line else {
            return Text::Failed("Type what this shows.".to_owned());
        };
        let job = ocr::Job {
            source: ocr::Source::Search(search),
            index,
            line,
        };
        if self.ocr.send(job).is_ok() {
            Text::Pending
        } else {
            Text::Failed("The text reader stopped. Type what this shows.".to_owned())
        }
    }

    /// Has the text reader read each of finds `indexes` of search `search`
    /// again, as a reference with the text color `ink`, if any, and
    /// `tolerance` tells their characters from the background now, and
    /// answers the search; `None` when that search was replaced or
    /// forgotten. A find the reader has yet to answer for is
    /// left waiting, so one read at a time is out for a find and the
    /// reader's next answer for it is for its newest read. An index named
    /// twice is read once.
    pub(crate) fn reread_found(
        &mut self,
        search: u64,
        indexes: &[usize],
        ink: Option<ColorDef>,
        tolerance: u8,
    ) -> Option<FoundView> {
        let reader = DigitReader::new(check_spec::ink_rule(ink, tolerance));
        let found = self.search(search)?;
        let id = found.id;
        let lines: Vec<(usize, Option<Line>)> = {
            let named: BTreeSet<usize> = indexes.iter().copied().collect();
            named
                .into_iter()
                .filter_map(|index| {
                    let item = found.items.get(index)?;
                    (item.text != Text::Pending)
                        .then(|| (index, find::ink_line(&item.around, item.within, &reader)))
                })
                .collect()
        };
        for (index, line) in lines {
            let text = self.read(id, index, line);
            if let Some(item) = self
                .found_mut()
                .and_then(|found| found.items.get_mut(index))
            {
                item.text = text;
                item.read_with = (ink, tolerance);
                item.characters =
                    characters_in(&item.around, item.within, &reader).unwrap_or_default();
            }
        }
        self.found_state()
    }

    /// Keeps the text reader's guess for find `index` of the search.
    pub(super) fn found_text(&mut self, index: usize, text: Text) {
        if let Some(item) = self
            .found_mut()
            .and_then(|found| found.items.get_mut(index))
        {
            item.text = text;
        }
    }

    /// The search, if there is one.
    pub(super) fn found(&self) -> Option<&Found> {
        self.capture
            .picked()
            .and_then(|picked| picked.found.as_ref())
    }

    /// The search, to change it.
    fn found_mut(&mut self) -> Option<&mut Found> {
        self.capture
            .picked_mut()
            .and_then(|picked| picked.found.as_mut())
    }

    /// The search the UI knows by the number `search`; `None` when that
    /// search was replaced or forgotten.
    fn search(&self, search: u64) -> Option<&Found> {
        self.found().filter(|found| found.id.number() == search)
    }

    /// The search as the UI shows it, or `None` without one.
    pub(crate) fn found_state(&self) -> Option<FoundView> {
        self.found().map(Found::view)
    }

    /// Find `index`'s pixels of search `search`, at most `max_width` wide;
    /// `None` when that search was replaced or forgotten.
    pub(crate) fn found_pixels(
        &self,
        search: u64,
        index: usize,
        max_width: u32,
    ) -> Option<Vec<u8>> {
        self.search(search)
            .and_then(|found| found.items.get(index))
            .and_then(|item| item.around.crop(item.within))
            .map(|image| pixels::rgba(&image, max_width))
    }

    /// Forgets the search.
    pub(crate) fn forget_found(&mut self) {
        if let Some(picked) = self.capture.picked_mut() {
            picked.found = None;
        }
    }

    /// The characters the box `region` of screenshot `screenshot` shows, as
    /// a reference with the text color `ink`, if any, and `tolerance`
    /// splits them, each where it is in the box and with the name a find's
    /// or a recorded reading's character of the same shape has; none for
    /// an unknown screenshot or a box that does not fit it.
    pub(crate) fn box_characters(
        &self,
        screenshot: usize,
        region: RegionDef,
        ink: Option<ColorDef>,
        tolerance: u8,
    ) -> Vec<CharacterView> {
        let reader = DigitReader::new(check_spec::ink_rule(ink, tolerance));
        self.capture
            .screenshots()
            .get(screenshot)
            .zip(Region::try_from(region).ok())
            .and_then(|(image, region)| characters_in(image, region, &reader))
            .unwrap_or_default()
    }

    /// For each of `found`, why the reference `spec` teaches cannot learn
    /// it, or `None` when it can; an error when `spec`'s own labels cannot
    /// be learned.
    pub(crate) fn found_problems(
        &mut self,
        spec: &CharactersSpec,
        found: &[Label],
    ) -> Result<Vec<Option<String>>, String> {
        self.keep_reference(spec.reference.as_ref());
        self.with_session(|session| spec.label_problems(session, found))
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use appytappy_sources::screen::{FrameHandler, Size};

    use crate::owner::test_support::{FakeHost, Harness, found_text, harness};

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

    fn region(x: u32, y: u32, width: u32, height: u32) -> RegionDef {
        serde_json::from_str(&format!(
            r#"{{"x":{x},"y":{y},"width":{width},"height":{height}}}"#
        ))
        .expect("valid JSON")
    }

    fn red() -> ColorDef {
        serde_json::from_str(r#"{"red":255,"green":0,"blue":0}"#).expect("valid JSON")
    }

    /// A harness with screenshot 0 of [`marks`].
    /// The numbers of the first and second searches of a [`searchable`]
    /// harness, whose capture took the ticket before them.
    const FIRST: u64 = 2;
    const SECOND: u64 = 3;

    fn first() -> ocr::Source {
        ocr::Source::Search(Ticket::numbered(FIRST))
    }

    fn searchable() -> (Harness, FrameHandler) {
        searchable_on(&marks())
    }

    /// A harness with screenshot 0 of `frame`.
    fn searchable_on(frame: &Image) -> (Harness, FrameHandler) {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(frame));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        (harness, handler)
    }

    /// A light 60 by 60 window with two dark panels, each with two white
    /// strokes 3 wide and 10 tall: one panel 28 by 18 at (2, 2), with
    /// strokes at x 8 and 14, and one 20 by 14 at (34, 30), with strokes
    /// at x 36 and 49, two pixels inside it.
    fn panels() -> Image {
        let inside = |x: u32, y: u32, (left, top, width, height): (u32, u32, u32, u32)| {
            (left..left.saturating_add(width)).contains(&x)
                && (top..top.saturating_add(height)).contains(&y)
        };
        let strokes = [
            (8, 6, 3, 10),
            (14, 6, 3, 10),
            (36, 32, 3, 10),
            (49, 32, 3, 10),
        ];
        let panels = [(2, 2, 28, 18), (34, 30, 20, 14)];
        let pixels = (0..60_u32)
            .flat_map(|y| (0..60_u32).map(move |x| (x, y)))
            .flat_map(|(x, y)| {
                if strokes.iter().any(|&stroke| inside(x, y, stroke)) {
                    [255, 255, 255, 0]
                } else if panels.iter().any(|&panel| inside(x, y, panel)) {
                    [20, 20, 20, 0]
                } else {
                    [200, 200, 200, 0]
                }
            })
            .collect();
        Image::new(
            Size {
                width: 60,
                height: 60,
            },
            pixels,
        )
        .expect("matching length")
    }

    fn find_like(
        region: RegionDef,
    ) -> impl FnOnce(&mut Owner<FakeHost>) -> Result<FoundView, String> {
        move |owner: &mut Owner<FakeHost>| owner.find_like(0, region, &[], &[], Some(red()), 0)
    }

    #[test]
    fn a_search_leaves_out_the_finds_passed_over() {
        let (mut harness, _handler) = searchable();
        let found = harness
            .ask(|owner| {
                owner.find_like(
                    0,
                    region(0, 0, 4, 5),
                    &[],
                    &[region(7, 0, 4, 5)],
                    Some(red()),
                    0,
                )
            })
            .expect("found");
        let regions: Vec<RegionDef> = found.finds.iter().map(|find| find.region).collect();
        assert_eq!(regions, [region(0, 6, 4, 5)]);
    }

    #[test]
    fn a_search_finds_the_other_marks_and_has_each_read() {
        let (mut harness, _handler) = searchable();
        let found = harness.ask(find_like(region(0, 0, 4, 5))).expect("found");
        assert_eq!(found.id, FIRST);
        assert_eq!(found.screenshot, 0);
        let regions: Vec<RegionDef> = found.finds.iter().map(|find| find.region).collect();
        assert_eq!(
            regions,
            [region(7, 0, 4, 5), region(0, 6, 4, 5)],
            "top to bottom then left to right, each with a margin"
        );
        assert!(found.finds.iter().all(|find| find.text == Text::Pending));
        // Each find is one mark, two columns wide after its margin of one,
        // and both marks have one shape.
        let characters: Vec<&[CharacterView]> = found
            .finds
            .iter()
            .map(|find| find.characters.as_slice())
            .collect();
        assert_eq!(characters[0], characters[1]);
        assert_eq!(
            characters[0]
                .iter()
                .map(|character| (character.left, character.right))
                .collect::<Vec<_>>(),
            [(1, 3)]
        );
        let jobs: Vec<(ocr::Source, usize)> = harness
            .jobs
            .try_iter()
            .map(|job| (job.source, job.index))
            .collect();
        assert_eq!(jobs, [(first(), 0), (first(), 1)]);
        // Text under another ticket than the search's, here its
        // capture's, is dropped.
        for (search, text) in [(FIRST - 1, "stale"), (FIRST, "7")] {
            assert!(
                harness
                    .owner
                    .handle(found_text(search, 1, Text::Read(text.to_owned())))
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
        assert_eq!(
            harness.ask(pixels_of(FIRST, 1)),
            Some(pixels::rgba(&crop, 20))
        );
        assert_eq!(harness.ask(pixels_of(FIRST, 2)), None);
        assert_eq!(
            harness.ask(pixels_of(FIRST - 1, 1)),
            None,
            "an earlier search's"
        );
    }

    #[test]
    fn a_search_leaves_out_the_finds_added_already() {
        let (mut harness, _handler) = searchable();
        let found = harness
            .ask(move |owner: &mut Owner<FakeHost>| {
                owner.find_like(
                    0,
                    region(0, 0, 4, 5),
                    &[region(7, 0, 4, 5)],
                    &[],
                    Some(red()),
                    0,
                )
            })
            .expect("found");
        let regions: Vec<RegionDef> = found.finds.iter().map(|find| find.region).collect();
        assert_eq!(regions, [region(0, 6, 4, 5)]);
        let indexes: Vec<usize> = harness.jobs.try_iter().map(|job| job.index).collect();
        assert_eq!(indexes, [0]);
    }

    #[test]
    fn a_new_search_replaces_the_old_and_moving_on_forgets_it() {
        let (mut harness, _handler) = searchable();
        harness.ask(find_like(region(0, 0, 4, 5))).expect("found");
        let again = harness.ask(find_like(region(0, 0, 4, 5))).expect("found");
        assert_eq!(again.id, SECOND);
        assert!(
            harness
                .owner
                .handle(found_text(FIRST, 0, Text::Read("1".to_owned())))
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
        let next = harness.pick();
        assert_eq!(
            harness.ask(|owner| owner.found_state()),
            None,
            "another window drops it"
        );
        let mut handler = next;
        harness.deliver(&mut handler, Ok(&marks()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        let found = harness.ask(find_like(region(0, 0, 4, 5))).expect("found");
        harness.ask(Owner::stop);
        assert_eq!(
            harness.ask(|owner| owner.found_state()),
            None,
            "stopping the capture, as Change window does, drops it"
        );
        assert_eq!(
            harness.ask(move |owner: &mut Owner<FakeHost>| owner.found_pixels(found.id, 0, 20)),
            None,
            "and the finds' pixels with it"
        );
    }

    fn reread(
        search: u64,
        indexes: &'static [usize],
        ink: ColorDef,
    ) -> impl FnOnce(&mut Owner<FakeHost>) -> Option<FoundView> {
        move |owner: &mut Owner<FakeHost>| owner.reread_found(search, indexes, Some(ink), 0)
    }

    /// A harness with search 1 of [`marks`], whose two finds the reader
    /// answered: the first failed and the second read as 7.
    fn answered() -> (Harness, FrameHandler) {
        let (mut harness, handler) = searchable();
        harness.ask(find_like(region(0, 0, 4, 5))).expect("found");
        assert_eq!(harness.jobs.try_iter().count(), 2);
        for (index, text) in [
            (0, Text::Failed("The text reader failed".to_owned())),
            (1, Text::Read("7".to_owned())),
        ] {
            assert!(
                harness
                    .owner
                    .handle(found_text(FIRST, index, text))
                    .is_continue()
            );
        }
        (harness, handler)
    }

    #[test]
    fn a_reread_sends_the_named_finds_again_and_leaves_the_rest() {
        let (mut harness, _handler) = answered();
        let view = harness.ask(reread(FIRST, &[0, 5], red())).expect("kept");
        assert_eq!(view.finds[0].text, Text::Pending);
        assert_eq!(
            view.finds[1].text,
            Text::Read("7".to_owned()),
            "a find that was not named keeps its text"
        );
        let jobs: Vec<(ocr::Source, usize)> = harness
            .jobs
            .try_iter()
            .map(|job| (job.source, job.index))
            .collect();
        assert_eq!(jobs, [(first(), 0)]);
        harness.ask(reread(FIRST, &[0, 1], red())).expect("kept");
        let indexes: Vec<usize> = harness.jobs.try_iter().map(|job| job.index).collect();
        assert_eq!(indexes, [1], "a find the reader has yet to answer waits");
        for index in 0..2 {
            assert!(
                harness
                    .owner
                    .handle(found_text(FIRST, index, Text::Read("7".to_owned())))
                    .is_continue()
            );
        }
        harness.ask(reread(FIRST, &[1, 0, 1], red())).expect("kept");
        let indexes: Vec<usize> = harness.jobs.try_iter().map(|job| job.index).collect();
        assert_eq!(indexes, [0, 1], "a find named twice is read once");
    }

    #[test]
    fn a_find_says_the_color_and_tolerance_it_was_last_read_with() {
        let (mut harness, _handler) = answered();
        let green: ColorDef =
            serde_json::from_str(r#"{"red":0,"green":255,"blue":0}"#).expect("valid JSON");
        let view = harness
            .ask(move |owner: &mut Owner<FakeHost>| owner.reread_found(FIRST, &[1], Some(green), 9))
            .expect("kept");
        assert_eq!(view.finds[0].characters.len(), 1);
        assert_eq!(
            view.finds[1].characters,
            [],
            "split again with the color it was read with, which shows none"
        );
        let read_with: Vec<(Option<ColorDef>, u8)> = view
            .finds
            .iter()
            .map(|find| (find.ink, find.tolerance))
            .collect();
        assert_eq!(
            read_with,
            [(Some(red()), 0), (Some(green), 9)],
            "only the find read again takes the new color and tolerance"
        );
    }

    #[test]
    fn a_reread_of_an_earlier_search_is_dropped() {
        let (mut harness, _handler) = answered();
        harness.ask(find_like(region(0, 0, 4, 5))).expect("found");
        assert_eq!(harness.jobs.try_iter().count(), 2);
        for index in 0..2 {
            assert!(
                harness
                    .owner
                    .handle(found_text(SECOND, index, Text::Read("3".to_owned())))
                    .is_continue()
            );
        }
        let before = harness.ask(|owner| owner.found_state());
        assert_eq!(harness.ask(reread(FIRST, &[0, 1], red())), None);
        assert_eq!(harness.jobs.try_iter().count(), 0, "nothing is read");
        assert_eq!(harness.ask(|owner| owner.found_state()), before);
        harness.ask(Owner::forget_found);
        assert_eq!(
            harness.ask(reread(SECOND, &[0], red())),
            None,
            "a forgotten one"
        );
    }

    #[test]
    fn a_reread_takes_the_ink_as_the_color_tells_it_now() {
        let (mut harness, _handler) = answered();
        let green: ColorDef =
            serde_json::from_str(r#"{"red":0,"green":255,"blue":0}"#).expect("valid JSON");
        let view = harness.ask(reread(FIRST, &[1], green)).expect("kept");
        assert_eq!(
            view.finds[1].text,
            Text::Failed("Type what this shows.".to_owned()),
            "a color the find shows none of leaves no ink to read"
        );
        assert_eq!(harness.jobs.try_iter().count(), 0);
        let (closed, _) = std::sync::mpsc::channel();
        harness.owner.ocr = closed;
        let view = harness.ask(reread(FIRST, &[1], red())).expect("kept");
        assert_eq!(
            view.finds[1].text,
            Text::Failed("The text reader stopped. Type what this shows.".to_owned())
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
            Err("Draw the box around some characters first.".to_owned())
        );
        assert_eq!(
            harness.ask(find_like(region(18, 0, 4, 5))),
            Err(
                "The box reaches past the edge of the window. Draw it inside the window."
                    .to_owned()
            )
        );
        assert_eq!(
            harness.ask(find_like(region(14, 0, 4, 5))),
            Err("The box shows no characters. Draw it around some characters first.".to_owned())
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
    fn a_box_shows_the_characters_a_find_of_the_same_text_does() {
        let (mut harness, _handler) = searchable();
        let found = harness.ask(find_like(region(0, 0, 4, 5))).expect("found");
        let characters = |screenshot: usize, region: RegionDef| {
            move |owner: &mut Owner<FakeHost>| {
                owner.box_characters(screenshot, region, Some(red()), 0)
            }
        };
        let drawn = harness.ask(characters(0, region(0, 0, 4, 5)));
        assert_eq!(drawn.len(), 1);
        assert_eq!(drawn, found.finds[0].characters);
        assert_eq!(harness.ask(characters(1, region(0, 0, 4, 5))), []);
        assert_eq!(harness.ask(characters(0, region(18, 0, 4, 5))), []);
        assert_eq!(harness.ask(characters(0, region(0, 0, 0, 5))), []);
        // A reference without a text color is given the pixels around a
        // box as tight as its mark, and the mark is still where it is in
        // the box.
        let tight = harness.ask(|owner: &mut Owner<FakeHost>| {
            owner.box_characters(0, region(8, 1, 2, 3), None, 0)
        });
        assert_eq!(
            tight
                .iter()
                .map(|character| (character.left, character.right))
                .collect::<Vec<_>>(),
            [(0, 2)]
        );
    }

    #[test]
    fn a_find_shows_the_characters_and_ink_its_region_does_as_a_box() {
        // By contrast, a find whose margin ends where its panel does is
        // read with the window around the panel as its background, as a
        // box there is, so the whole panel is one character.
        let (mut harness, _handler) = searchable_on(&panels());
        let found = harness
            .ask(|owner| owner.find_like(0, region(6, 4, 13, 14), &[], &[], None, 30))
            .expect("found");
        let panel = region(34, 30, 20, 14);
        let regions: Vec<RegionDef> = found.finds.iter().map(|find| find.region).collect();
        assert_eq!(regions, [panel]);
        let drawn = harness
            .ask(move |owner: &mut Owner<FakeHost>| owner.box_characters(0, panel, None, 30));
        assert_eq!(
            drawn
                .iter()
                .map(|character| (character.left, character.right))
                .collect::<Vec<_>>(),
            [(0, 20)]
        );
        assert_eq!(found.finds[0].characters, drawn);
        // The text reader is sent the panel's 20 columns too, with its
        // margin of 2 on each side, not the 16 the strokes span.
        let sent = |harness: &Harness| -> Vec<u32> {
            harness.jobs.try_iter().map(|job| job.line.width).collect()
        };
        assert_eq!(sent(&harness), [24]);
        assert!(
            harness
                .owner
                .handle(found_text(FIRST, 0, Text::Failed("no".to_owned())))
                .is_continue()
        );
        let again = harness
            .ask(|owner| owner.reread_found(FIRST, &[0], None, 30))
            .expect("kept");
        assert_eq!(again.finds[0].characters, drawn, "a reread splits it so");
        assert_eq!(sent(&harness), [24], "and sends the same ink");
        let crop = panels()
            .crop(Region::new(34, 30, 20, 14).expect("not empty"))
            .expect("inside");
        assert_eq!(
            harness.ask(|owner| owner.found_pixels(FIRST, 0, 60)),
            Some(pixels::rgba(&crop, 60)),
            "the picture is the find's region alone"
        );
    }

    #[test]
    fn found_problems_judge_each_find_on_the_ones_before_it() {
        let (mut harness, _handler) = searchable();
        let spec: CharactersSpec = serde_json::from_str(
            r#"{"region":{"x":0,"y":0,"width":4,"height":5},"screenshot":0,
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
            Some("It shows 1 character but the text has 2. Characters that touch count as one.")
        );
        assert_eq!(problems[1], None);
        assert_eq!(
            problems[2].as_deref(),
            Some("The region reaches past the edge of the window. Draw it inside the window.")
        );
        assert_eq!(problems[3], None);
    }
}
