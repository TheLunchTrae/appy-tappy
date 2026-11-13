//! The live check: the value or the reference the capture screen is
//! setting up, read on the picked window's newest frame the way the
//! runtime will read it once it is saved.
//!
//! The owner builds a check's saved form and reads it back with
//! `saved::build`, exactly as a save does, so a check that reads here is
//! one the runtime can read. It then reads the value where
//! `SavedValue::at` places it at the window's size, as the runtime does:
//! the drawing made at that size, the first drawing scaled to it while the
//! application's window changes size often, or nothing, with the reason.
//! Each frame means to it what it means to the runtime ([`FrameMeaning`]): at
//! another size nothing is read until the size settles, and the value is
//! then placed there once. The capture copies the whole window, so it
//! goes on at every size.

use std::time::Instant;

use appytappy_config::app_file::{CheckDef, PinDef, ScreenDef};
use appytappy_config::signal::Value;
use appytappy_sources::screen::check::Check;
use appytappy_sources::screen::saved::{AtSize, Readers, SavedValue, Unread};
use appytappy_sources::screen::settle::Settle;
use appytappy_sources::screen::{FrameMeaning, Image, Size};
use serde::Deserialize;

use crate::check_spec::{self, CharactersSpec, CheckSpec};
use crate::reading::at_size;

use super::Owner;
use super::host::Host;
use super::values::read_back;

/// A value's check to try on the live window before it is saved, as the UI
/// describes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct TrialSpec {
    /// The saved application the value is for, whose references it reads
    /// with and whose window's size changes say where it reads.
    pub(crate) app: String,
    /// The check, with the session's screenshots it names.
    pub(crate) check: CheckSpec,
    /// The screenshot the region was drawn on, whose size the value would
    /// keep.
    pub(crate) screenshot: usize,
    /// The side of the window the region stays at as the window changes
    /// size, or `None` before one is picked, for the side it is nearest.
    pub(crate) pin: Option<PinDef>,
}

/// A reference being taught to try on the live window before it is saved,
/// as the UI describes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReferenceTrial {
    /// The saved application the reference is for, whose window's size
    /// changes say where its region reads.
    pub(crate) app: String,
    /// The characters it is taught.
    pub(crate) characters: CharactersSpec,
    /// The characters the number it is made for reads of it, or `None`
    /// for every character it is taught, as a saved number check has them.
    pub(crate) reads: Option<String>,
    /// Whether that number ignores digits after the decimal point.
    pub(crate) whole: bool,
    /// The side of the window that number's region stays at, or `None`
    /// for the side the region is nearest.
    pub(crate) pin: Option<PinDef>,
}

/// The ID a value is tried under before it is saved under its own. It
/// names only pictures that are never written.
const TRIED: &str = "tried";

/// What a reference that is being taught is called where its live check
/// says what to fix, since it may have no name yet.
const TAUGHT: &str = "The reference";

/// A value as it would be saved, placed at the window size its frames
/// settled at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LiveCheck {
    value: SavedValue,
    /// The window size the value is placed at, and where and how it reads
    /// there or why it does not.
    placed: (Size, Result<AtSize, Unread>),
    /// The size frames last settled at where the value does not read, and
    /// why, while it is placed at a size it reads at. It stays placed
    /// there, as the runtime's capture stays at the size it reads, so a
    /// frame that shows that size again is read at once.
    unread: Option<(Size, Unread)>,
    /// The window size frames show, settled.
    settle: Settle,
}

impl LiveCheck {
    /// `value`, placed at `size`.
    pub(super) fn new(value: SavedValue, size: Size) -> Self {
        let placed = (size, value.at(size));
        Self {
            value,
            placed,
            unread: None,
            settle: Settle::new(size),
        }
    }

    /// Notes that a frame at `now` showed the window at `size`.
    pub(super) fn saw(&mut self, size: Size, now: Instant) {
        self.settle.saw(size, now);
        self.settled(now);
    }

    /// Places the value at the size its frames settled at by `now`, once
    /// they have, unless it is placed there already. Placing redraws a
    /// scaled state check's pictures, so a drag across many sizes places
    /// it once.
    pub(super) fn settled(&mut self, now: Instant) {
        let Some(size) = self
            .settle
            .settled(now)
            .filter(|size| *size != self.placed.0)
        else {
            return;
        };
        match (self.value.at(size), &self.placed.1) {
            (Err(unread), Ok(_)) => self.unread = Some((size, unread)),
            (placed, _) => {
                self.placed = (size, placed);
                self.unread = None;
            },
        }
    }

    /// Why the value is not read at the size it is placed at, as a
    /// sentence, or `None` when it reads there.
    pub(super) fn unread(&self) -> Option<String> {
        let (size, placed) = &self.placed;
        placed.as_ref().err().map(|unread| not_read(*size, *unread))
    }

    /// The check that reads the value where it is placed and its region's
    /// pixels on `frame`, a frame of the whole window; `None` while the
    /// frame shows a size that has yet to settle; or why the value is not
    /// read, for the user.
    pub(super) fn region_of(&self, frame: &Image) -> Result<Option<(&Check, Image)>, String> {
        let (size, placed) = &self.placed;
        // What a capture of the value's region would copy of the frame.
        let copied = match placed {
            Ok(at) => frame.crop(at.region).map(|region| Ok((at, region))),
            Err(unread) => Some(Err(*unread)),
        };
        match FrameMeaning::of(*size, frame.size(), copied) {
            FrameMeaning::Readable(Ok((at, region))) => Ok(Some((self.value.check(at), region))),
            FrameMeaning::Readable(Err(unread)) => Err(not_read(*size, unread)),
            FrameMeaning::Resized(other) => self
                .unread
                .filter(|(unread_at, _)| *unread_at == other)
                .map_or(Ok(None), |(_, unread)| Err(not_read(other, unread))),
            FrameMeaning::Unreadable(outside) => Err(on_window(&outside)),
        }
    }

    /// What the value reads on `frame`, `None` while the frame's size has
    /// yet to settle, or why it cannot be read there, for the user.
    pub(super) fn read(&self, frame: &Image) -> Result<Option<Value>, String> {
        let Some((check, region)) = self.region_of(frame)? else {
            return Ok(None);
        };
        check
            .read(&region)
            .map(Some)
            .map_err(|error| on_window(&error))
    }
}

/// That a value is not read on a window of `size`, and why, as a sentence.
fn not_read(size: Size, unread: Unread) -> String {
    format!("Not read {}.", at_size(size, unread))
}

/// That the newest frame cannot be read, and why, as a sentence.
fn on_window(error: &dyn std::fmt::Display) -> String {
    format!("Not read on the live window now: {error}.")
}

impl<H: Host> Owner<H> {
    /// Makes `trial` the check the live frame is read with: the value it
    /// would save, read back as the runtime reads a saved value and placed
    /// at the window's size now.
    pub(crate) fn set_check(&mut self, trial: &TrialSpec) -> Result<(), String> {
        let found = self.stores()?.app(&trial.app)?;
        let size =
            check_spec::drawn_size(&trial.check, self.capture.screenshots(), trial.screenshot)
                .map_err(|error| error.to_string())?;
        let saved =
            check_spec::to_saved(&trial.check, self.capture.screenshots(), &trial.app, TRIED)
                .map_err(|error| error.to_string())?;
        let screen = ScreenDef {
            window_size: size.into(),
            region: saved.region,
            check: saved.check,
            pin: trial.pin,
            reading_area: false,
            fixes: Vec::new(),
        };
        let value = read_back(
            &screen,
            &found,
            Readers::new(&found.file.references),
            &saved.pictures,
        )?;
        self.go_live(value);
        Ok(())
    }

    /// Makes the reference `trial` teaches the check the live frame is
    /// read with: a number read with it in the region the characters were
    /// drawn in, read back as [`Self::set_check`] reads a value's check
    /// back. The number is the one the reference is made for, where it is
    /// made for one: it reads only that number's characters, whole when that
    /// number is, and stays at its side. The saved reference the characters
    /// add to is kept for the asks that follow.
    pub(crate) fn set_characters(&mut self, trial: &ReferenceTrial) -> Result<(), String> {
        self.keep_reference(trial.characters.reference.as_ref());
        let mut found = self.stores()?.app(&trial.app)?;
        let taught = self
            .with_session(|session| trial.characters.teach(session, None, TAUGHT, ""))
            .map_err(|error| error.to_string())?;
        let id = taught.reference.id.clone();
        let screen = ScreenDef {
            window_size: taught.window_size.into(),
            region: taught.region.into(),
            check: CheckDef::Number {
                reference: id.clone(),
                whole: trial.whole,
                characters: trial.reads.clone(),
            },
            pin: trial.pin,
            reading_area: false,
            fixes: Vec::new(),
        };
        // The number reads with the reference as it is taught here, not as
        // the application's file has it, and with the reader teaching
        // learned, which reading back would only learn again from the same
        // pictures.
        found.file.references = vec![taught.reference];
        let readers = Readers::new(&found.file.references).learned(&id, taught.reader);
        let value = read_back(&screen, &found, readers, &Vec::new())?;
        self.go_live(value);
        Ok(())
    }

    /// Reads the live frame of the picked window with `value` from now on,
    /// placed at the newest frame's size, or at the size it was drawn at
    /// before a frame arrived, and answers why it is not read at that size,
    /// if it is not. With no window picked there is no frame to read, so
    /// the check is not kept.
    pub(super) fn go_live(&mut self, value: SavedValue) -> Option<String> {
        let Some(picked) = self.capture.picked_mut() else {
            let size = value.drawn.window_size;
            return LiveCheck::new(value, size).unread();
        };
        let size = picked
            .latest
            .as_ref()
            .map_or(value.drawn.window_size, |(frame, _arrived)| frame.size());
        picked.check.insert(LiveCheck::new(value, size)).unread()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::Duration;

    use appytappy_config::app_file::{CheckDef, SizeChangesDef, ValueDef};
    use appytappy_sources::screen::saved::{self, SavedCheck};
    use appytappy_sources::screen::settle::SETTLE;

    use crate::check_spec::{CharactersSpec, CheckSpec};
    use crate::debug::workings::WorkingsView;
    use crate::owner::ReferenceSaveSpec;
    use crate::owner::test_support::{
        BAR, Harness, add_example_app, capturing, characters, harness_with, scratch_store,
        teaching, trial,
    };
    use crate::reading::Reading;
    use crate::stores::SavedApp;

    use super::*;

    #[test]
    fn the_check_reads_the_live_frame() {
        let (mut harness, _handler, root) = capturing("live-reads");
        assert_eq!(harness.ask(Owner::reading), Ok(None));
        assert_eq!(harness.ask(|owner| owner.explain_reading()), Ok(None));
        harness
            .ask(|owner| owner.set_check(&trial(BAR)))
            .expect("valid check");
        assert_eq!(harness.ask(Owner::reading), Ok(Some(Reading::Number(50.0))));
        harness.wait(Duration::from_millis(30));
        let details = harness
            .ask(|owner| owner.explain_reading())
            .expect("explains")
            .expect("a frame arrived");
        assert_eq!(details.frame_age_ms, 30, "by the owner's clock");
        assert!(
            matches!(
                details.workings,
                Some(WorkingsView::BarFill { filled, counted }) if filled * 2 == counted
            ),
            "{details:?}"
        );
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_check_that_cannot_be_saved_or_read_back_says_what_to_fix() {
        let (mut harness, _handler, root) = capturing("live-refused");
        let mut refused = |check: &str| harness.ask(|owner| owner.set_check(&trial(check)));
        assert_eq!(
            refused(
                r#"{"kind":"color","region":{"x":0,"y":0,"width":0,"height":2},
                    "color":{"red":255,"green":0,"blue":0},"tolerance":0}"#
            ),
            Err("Draw a region on the screenshot first.".to_owned())
        );
        // What only `saved::build` judges is refused before the save, too.
        assert_eq!(
            refused(
                r#"{"kind":"barFill","region":{"x":0,"y":0,"width":4,"height":2},
                    "filled":{"red":9,"green":9,"blue":9},"empty":{"red":9,"green":9,"blue":9}}"#
            ),
            Err("Pick a filled color and an empty color that differ.".to_owned())
        );
        assert_eq!(
            refused(
                r#"{"kind":"color","region":{"x":3,"y":0,"width":2,"height":2},
                    "color":{"red":255,"green":0,"blue":0},"tolerance":0}"#
            ),
            Err(
                "The region reaches past the edge of the window. Draw it inside the window."
                    .to_owned()
            )
        );
        assert_eq!(
            refused(
                r#"{"kind":"number","region":{"x":0,"y":0,"width":2,"height":2},
                    "reference":{"app":"example","id":"gone"},"whole":false}"#
            ),
            Err("Example App no longer has that reference.".to_owned())
        );
        assert_eq!(harness.ask(Owner::reading), Ok(None));
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_live_frame_the_check_cannot_read_says_which_stage_failed() {
        let (mut harness, mut handler, root) = capturing("live-stage");
        // A reference taught that the screenshot's red half is a 1.
        let spec = characters(
            r#"{"region":{"x":0,"y":0,"width":4,"height":2},"screenshot":0,
                "ink":{"red":255,"green":0,"blue":0},"tolerance":0,
                "labels":[{"screenshot":0,"text":"1"}]}"#,
        );
        harness
            .ask(|owner| owner.set_characters(&teaching(spec.clone())))
            .expect("valid check");
        assert_eq!(harness.ask(Owner::reading), Ok(Some(Reading::Number(1.0))));
        let gray = Image::new(
            Size {
                width: 4,
                height: 2,
            },
            [40, 40, 40, 0].repeat(8),
        )
        .expect("matching length");
        harness.deliver(&mut handler, Ok(&gray));
        assert_eq!(
            harness.ask(Owner::reading),
            Err(
                "Not read on the live window now: no pixel in the region has the characters' \
                 color."
                    .to_owned()
            )
        );
        fs::remove_dir_all(root).expect("clean up");
    }

    /// A font of 3 by 5 pixels.
    const GLYPHS: [(char, [&str; 5]); 5] = [
        ('3', ["###", "..#", "###", "..#", "###"]),
        ('7', ["###", "..#", "..#", "..#", "..#"]),
        ('9', ["###", "#.#", "###", "..#", "###"]),
        ('%', ["#.#", "..#", ".#.", "#..", "#.#"]),
        ('.', ["...", "...", "...", "...", ".#."]),
    ];

    /// The window's size on the screenshot the percentage is drawn on.
    const DRAWN: Size = Size {
        width: 24,
        height: 12,
    };

    /// A frame of a window `scale` times [`DRAWN`] that shows `text` in
    /// white on black, 8 pixels across and 4 down at scale 1, each cell of
    /// the font `scale` pixels wide and tall, a blank cell between
    /// characters.
    fn showing(text: &str, scale: u32) -> Image {
        let lit = |x: u32, y: u32| {
            let cell = |pixel: u32| pixel.checked_div(scale).expect("a scale above zero");
            let (Some(column), Some(row)) = (cell(x).checked_sub(8), cell(y).checked_sub(4)) else {
                return false;
            };
            let across = usize::try_from(column % 4).expect("small");
            usize::try_from(column / 4)
                .ok()
                .and_then(|index| text.chars().nth(index))
                .and_then(|character| {
                    GLYPHS
                        .iter()
                        .find_map(|(glyph, rows)| (*glyph == character).then_some(rows))
                })
                .and_then(|rows| rows.get(usize::try_from(row).expect("small")))
                .is_some_and(|line| line.as_bytes().get(across) == Some(&b'#'))
        };
        let size = Size {
            width: DRAWN.width.checked_mul(scale).expect("a small window"),
            height: DRAWN.height.checked_mul(scale).expect("a small window"),
        };
        let bgra = (0..size.height)
            .flat_map(|y| (0..size.width).map(move |x| (x, y)))
            .flat_map(|(x, y)| {
                if lit(x, y) {
                    [255, 255, 255, 0]
                } else {
                    [0; 4]
                }
            })
            .collect();
        Image::new(size, bgra).expect("a pixel for each of the window's")
    }

    /// The reference `percent`, taught `379%` on screenshot 0, and the
    /// number `Charge` read with it in a region around the text.
    const PERCENT: &str = r#"{"region":{"x":6,"y":2,"width":18,"height":9},"screenshot":0,
        "ink":{"red":255,"green":255,"blue":255},"tolerance":0,
        "labels":[{"screenshot":0,"text":"379%"}]}"#;
    const CHARGE: &str = r#"{"kind":"number","region":{"x":6,"y":2,"width":18,"height":9},
        "reference":{"app":"example","id":"percent"},"whole":false}"#;

    /// What the live box says of `379%` read without the `7`.
    const NO_SEVEN: &str = "Not read on the live window now: the value leaves out a character \
        its number holds: it read `3 7 9 %`.";

    /// A harness whose application `example` changes size as `size_changes`
    /// say, with a window that shows `379%` at [`DRAWN`] on screenshot 0 and
    /// the reference `percent` saved from it.
    fn with_percent(
        name: &str,
        size_changes: SizeChangesDef,
    ) -> (
        Harness,
        appytappy_sources::screen::FrameHandler,
        std::path::PathBuf,
    ) {
        let (stores, root) = scratch_store(name);
        let mut harness = harness_with(false, Ok(stores));
        add_example_app(&mut harness);
        harness
            .ask(|owner| {
                owner.update_app("example", crate::owner::Changed::Values, |_owner, found| {
                    found.file.size_changes = size_changes;
                    Ok(crate::owner::Edit::Save((), Vec::new()))
                })
            })
            .expect("saves");
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&showing("379%", 1)));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        harness
            .ask(|owner| {
                owner.save_reference(&ReferenceSaveSpec {
                    app: "example".to_owned(),
                    reference_name: "Percent".to_owned(),
                    expects: String::new(),
                    characters: characters(PERCENT),
                })
            })
            .expect("saves the reference");
        (harness, handler, root)
    }

    /// What the runtime reads for `value` of `app` on `frame`, a frame of
    /// the whole window: the value built from the saved files, placed at
    /// the frame's size, and read from its region there.
    fn as_the_runtime_reads(app: &SavedApp, value: &ValueDef, frame: &Image) -> Option<Value> {
        let file = &app.file;
        let built = saved::build(
            &value.screen,
            &mut saved::Readers::new(&file.references),
            file.size_changes,
            |name| app.picture(name),
        )
        .expect("the runtime reads the saved value");
        let at = built.at(frame.size()).ok()?;
        built.check(&at).read(&frame.crop(at.region)?).ok()
    }

    #[test]
    fn a_percentage_tried_on_the_live_window_reads_as_the_runtime_reads_it_once_saved() {
        let (mut harness, mut handler, root) = with_percent("live-percent", SizeChangesDef::Often);
        harness
            .ask(|owner| owner.set_check(&trial(CHARGE)))
            .expect("valid check");
        // A frame of another size is read once its size settled.
        let tried_at = |harness: &mut Harness, handler: &mut _, frame: &Image| {
            harness.deliver(handler, Ok(frame));
            harness.wait(SETTLE);
            harness.ask(Owner::reading)
        };
        let same_size = showing("37%", 1);
        // The window at twice the size it was drawn at: the unscaled region
        // holds only the top left corner of the text.
        let doubled = showing("93%", 2);
        let tried = [
            tried_at(&mut harness, &mut handler, &same_size),
            tried_at(&mut harness, &mut handler, &doubled),
        ];
        assert_eq!(
            tried,
            [
                Ok(Some(Reading::Number(37.0))),
                Ok(Some(Reading::Number(93.0)))
            ]
        );

        let saved = harness
            .ask(|owner| {
                owner.save(&crate::owner::test_support::spec(
                    "example", "Charge", CHARGE,
                ))
            })
            .expect("saves");
        let app = harness
            .ask(|owner| owner.stores().and_then(|stores| stores.app("example")))
            .expect("loads");
        let value = app
            .file
            .values
            .iter()
            .find(|value| value.id == saved.value)
            .expect("saved");
        assert!(matches!(value.screen.check, CheckDef::Number { .. }));
        let runtime = [&same_size, &doubled]
            .map(|frame| Ok(as_the_runtime_reads(&app, value, frame).map(Reading::from)));
        assert_eq!(tried, runtime, "setup previews what the runtime reads");
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_number_tried_with_characters_its_reference_knows_none_of_says_so() {
        let (mut harness, _handler, root) = with_percent("live-subset", SizeChangesDef::Often);
        let reading = |characters: &str| {
            let mut tried = trial(CHARGE);
            let CheckSpec::Number {
                characters: reads, ..
            } = &mut tried.check
            else {
                panic!("a number");
            };
            *reads = Some(characters.to_owned());
            tried
        };
        harness
            .ask(|owner| owner.set_check(&reading("0123456789%")))
            .expect("the reference knows four of them");
        assert_eq!(
            harness.ask(Owner::reading),
            Ok(Some(Reading::Number(379.0)))
        );
        // Read with the digits alone, the percent sign ends the number, as
        // it does for the runtime.
        harness
            .ask(|owner| owner.set_check(&reading("0123456789")))
            .expect("the reference knows three of them");
        assert_eq!(
            harness.ask(Owner::reading),
            Ok(Some(Reading::Number(379.0)))
        );
        // A digit the value leaves out is not read around.
        harness
            .ask(|owner| owner.set_check(&reading("39%")))
            .expect("the reference knows them");
        assert_eq!(harness.ask(Owner::reading), Err(NO_SEVEN.to_owned()));
        assert_eq!(
            harness.ask(|owner| owner.set_check(&reading("2"))),
            Err(
                "Percent knows none of the characters this value reads, \"2\". Teach it one of \
                 them, or change which characters the value reads."
                    .to_owned()
            )
        );
        assert_eq!(
            harness.ask(|owner| owner.set_check(&reading(""))),
            Err("Choose at least one character for the value to read.".to_owned())
        );
        fs::remove_dir_all(root).expect("clean up");
    }

    /// `frame`, of a window of [`DRAWN`], with its last column once more.
    fn a_column_wider(frame: &Image) -> Image {
        Image::new(
            Size {
                width: DRAWN.width + 1,
                height: DRAWN.height,
            },
            frame
                .pixels()
                .chunks(24)
                .flat_map(|row| row.iter().chain(row.last()).flatten().copied())
                .collect(),
        )
        .expect("a column more")
    }

    /// What the live box says at a window a column wider than [`DRAWN`]
    /// when the application's window changes size rarely.
    const A_COLUMN_WIDER: &str = "Not read at 25 × 12, the window's size now: it is read only \
        at the window sizes it was drawn at.";

    #[test]
    fn a_reference_made_for_a_number_reads_as_that_number_will() {
        let (mut harness, mut handler, root) =
            with_percent("live-made-for", SizeChangesDef::Rarely);
        harness.deliver(&mut handler, Ok(&showing("3.7%", 1)));
        assert_eq!(harness.ask(Owner::screenshot), Ok(1));
        let taught = CharactersSpec {
            screenshot: 1,
            labels: vec![serde_json::from_str(r#"{"screenshot":1,"text":"3.7%"}"#).expect("valid")],
            ..characters(PERCENT)
        };
        harness
            .ask(|owner| owner.set_characters(&teaching(taught.clone())))
            .expect("learns");
        assert_eq!(harness.ask(Owner::reading), Ok(Some(Reading::Number(3.7))));
        // Made for a whole number, it reads as one.
        let whole = ReferenceTrial {
            whole: true,
            ..teaching(taught)
        };
        harness
            .ask(|owner| owner.set_characters(&whole))
            .expect("learns");
        assert_eq!(harness.ask(Owner::reading), Ok(Some(Reading::Number(3.0))));
        // Its application's window changes size rarely, so at another size
        // it reads nothing, as the number will.
        harness.deliver(&mut handler, Ok(&a_column_wider(&showing("3.7%", 1))));
        harness.wait(SETTLE);
        assert_eq!(harness.ask(Owner::reading), Err(A_COLUMN_WIDER.to_owned()));
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_value_tried_at_a_size_the_runtime_does_not_read_it_at_says_so() {
        let (mut harness, mut handler, root) = with_percent("live-rarely", SizeChangesDef::Rarely);
        harness
            .ask(|owner| owner.set_check(&trial(CHARGE)))
            .expect("valid check");
        assert_eq!(
            harness.ask(Owner::reading),
            Ok(Some(Reading::Number(379.0)))
        );
        // A frame of another size reads nothing until the size settles, as
        // the runtime reads nothing: a drag passes many sizes, and placing
        // the value at each would redraw a state check's pictures for each.
        let wider = a_column_wider(&showing("37%", 1));
        harness.deliver(&mut handler, Ok(&wider));
        assert_eq!(harness.ask(Owner::reading), Ok(None));
        assert_eq!(harness.ask(|owner| owner.explain_reading()), Ok(None));
        harness.wait(SETTLE / 2);
        harness.deliver(&mut handler, Ok(&showing("37%", 2)));
        harness.deliver(&mut handler, Ok(&wider));
        harness.wait(SETTLE / 2);
        assert_eq!(
            harness.ask(Owner::reading),
            Ok(None),
            "another size started the wait again"
        );
        // The poll places the value once the size settled, with no frame.
        // Its text still shows inside the region as drawn, which is not
        // read all the same.
        harness.wait(SETTLE / 2);
        let unread = "Not read at 25 × 12, the window's size now: it is read only at the window \
                      sizes it was drawn at.";
        assert_eq!(harness.ask(Owner::reading), Err(unread.to_owned()));
        assert_eq!(
            harness.ask(|owner| owner.explain_reading()),
            Err(unread.to_owned())
        );
        // The value stays placed at the size it reads at, so a frame that
        // shows it again is read at once, as the runtime reads it, and a
        // frame of the size it does not read at says why at once.
        harness.deliver(&mut handler, Ok(&showing("37%", 1)));
        assert_eq!(harness.ask(Owner::reading), Ok(Some(Reading::Number(37.0))));
        harness.deliver(&mut handler, Ok(&wider));
        assert_eq!(harness.ask(Owner::reading), Err(unread.to_owned()));
        // A third size, not settled, says nothing of the second.
        harness.deliver(&mut handler, Ok(&showing("37%", 2)));
        assert_eq!(harness.ask(Owner::reading), Ok(None));
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn settling_where_a_fix_reads_forgets_the_size_it_was_not_read_at() {
        use appytappy_sources::screen::check::ColorMatch;
        use appytappy_sources::screen::saved::SavedFix;
        use appytappy_sources::screen::{Region, Rgb};

        let size = |width| Size { width, height: 4 };
        let (drawn, fixed, unread) = (size(8), size(10), size(12));
        let region = Region::new(0, 0, 2, 2).expect("not empty");
        let value = SavedValue {
            drawn: SavedCheck {
                window_size: drawn,
                region,
                check: Check::Color(ColorMatch {
                    color: Rgb {
                        red: 0,
                        green: 0,
                        blue: 0,
                    },
                    tolerance: 0,
                }),
            },
            fixes: vec![SavedFix {
                window_size: fixed,
                region,
                check: None,
            }],
            pin: PinDef::TopLeft,
            size_changes: SizeChangesDef::Rarely,
        };
        let frame = |size: Size| {
            let bytes = size
                .width
                .checked_mul(size.height)
                .and_then(|pixels| pixels.checked_mul(4))
                .and_then(|bytes| usize::try_from(bytes).ok())
                .expect("small");
            Image::new(size, vec![0; bytes]).expect("matching length")
        };
        let start = Instant::now();
        let settled = start.checked_add(SETTLE).expect("in range");
        let later = settled.checked_add(SETTLE).expect("in range");
        let mut live = LiveCheck::new(value, drawn);
        live.saw(unread, start);
        live.saw(unread, settled);
        assert!(
            live.region_of(&frame(unread)).is_err(),
            "the size it is not read at says why"
        );
        // Settled where the fix reads, it is placed there, and the size it
        // was not read at is only another size again.
        live.saw(fixed, settled);
        live.saw(fixed, later);
        assert!(matches!(live.region_of(&frame(fixed)), Ok(Some(_))));
        assert_eq!(
            live.region_of(&frame(unread)).map(|read| read.is_some()),
            Ok(false)
        );
    }

    #[test]
    fn a_reference_being_taught_reads_where_its_region_scales_to() {
        let (mut harness, mut handler, root) = capturing("live-taught");
        harness.deliver(&mut handler, Ok(&showing("379%", 1)));
        assert_eq!(harness.ask(Owner::screenshot), Ok(1));
        let taught = CharactersSpec {
            screenshot: 1,
            labels: vec![serde_json::from_str(r#"{"screenshot":1,"text":"379%"}"#).expect("valid")],
            ..characters(PERCENT)
        };
        harness
            .ask(|owner| owner.set_characters(&teaching(taught.clone())))
            .expect("learns");
        assert_eq!(
            harness.ask(Owner::reading),
            Ok(Some(Reading::Number(379.0)))
        );
        let no_seven = ReferenceTrial {
            reads: Some("39%".to_owned()),
            ..teaching(taught.clone())
        };
        // Made for a value that reads only some characters, it reads as
        // that value will: not around a digit the value leaves out.
        harness
            .ask(|owner| owner.set_characters(&no_seven))
            .expect("learns them");
        assert_eq!(harness.ask(Owner::reading), Err(NO_SEVEN.to_owned()));
        assert_eq!(
            harness.ask(|owner| owner.set_characters(&ReferenceTrial {
                reads: Some("2".to_owned()),
                ..teaching(taught.clone())
            })),
            Err(
                "The reference knows none of the characters this value reads, \"2\". Teach it \
                 one of them, or change which characters the value reads."
                    .to_owned()
            )
        );
        // The file's own rule for a number holds here as it does at a save.
        assert_eq!(
            harness.ask(|owner| owner.set_characters(&ReferenceTrial {
                reads: Some(String::new()),
                ..teaching(taught.clone())
            })),
            Err("Choose at least one character for the value to read.".to_owned())
        );
        harness
            .ask(|owner| owner.set_characters(&teaching(taught.clone())))
            .expect("learns");
        harness.deliver(&mut handler, Ok(&showing("97%", 2)));
        harness.wait(SETTLE);
        assert_eq!(harness.ask(Owner::reading), Ok(Some(Reading::Number(97.0))));
        fs::remove_dir_all(root).expect("clean up");
    }
}
