//! Edits: a saved value changed in place. It keeps its ID, so the
//! automations that watch it stay linked, and the window size it was drawn
//! at, so its region means what it did. An edit that would break an
//! automation's condition is refused, and one that leaves the value's fixes
//! unusable removes them only with the user's word for it.

use appytappy_config::app_file::{CheckDef, ScreenDef, SizeDef, ValueDef};
use appytappy_sources::screen::Image;

use crate::automations;
use crate::check_spec::{self, Pictures, RegionCheck};
use crate::stores::SavedApp;

use super::fixes::free_picture_number;
use super::host::Host;
use super::messages::{EditDrawing, EditPreview, EditSpec, SavedValue};
use super::references::and_list;
use super::values::{named_apart, read_back};
use super::{Changed, Edit, Owner};

/// A saved value as an edit leaves it, before anything is saved.
#[derive(Debug)]
struct Edited {
    /// Where the value is among its application's values.
    position: usize,
    /// The value, under the ID it has.
    value: ValueDef,
    /// The new pictures its check names.
    pictures: Pictures,
    /// The window sizes of the fixes it had that no longer suit its check,
    /// which it leaves out.
    dropped: Vec<SizeDef>,
    /// Which automations suit the saved value but would not suit this one,
    /// as a sentence, if any.
    automations: Result<(), String>,
}

impl<H: Host> Owner<H> {
    /// Makes the value as `spec` edits it the check the live frame is read
    /// with, where and how the runtime would read it at the window's size
    /// now, and answers what saving the edit would do. Nothing is saved.
    ///
    /// # Errors
    ///
    /// Returns what to fix when the edit cannot be saved as it is, other
    /// than for the fixes and automations the answer names.
    pub(crate) fn try_edit(&mut self, spec: &EditSpec) -> Result<EditPreview, String> {
        let found = self.stores()?.app(&spec.app)?;
        let edited = self.edited(spec, &found)?;
        let value = read_back(&edited.value.screen, &found, &edited.pictures)?;
        let size = self
            .latest
            .as_ref()
            .map_or(value.drawn.window_size, Image::size);
        let unread = match value.at(size) {
            Ok(at) => {
                self.check = Some(RegionCheck {
                    region: at.region,
                    check: value.check(&at).clone(),
                });
                None
            },
            Err(unread) => {
                self.check = None;
                Some(format!(
                    "Not read at {} × {}, the window's size now: {unread}.",
                    size.width, size.height
                ))
            },
        };
        Ok(EditPreview {
            unread,
            dropped_fixes: edited.dropped,
            refused: edited.automations.err(),
        })
    }

    /// Saves the value as `spec` edits it over the saved one, under the same
    /// ID, and answers where it was saved, or what to fix. With
    /// `drop_fixes`, the user agreed to lose the fixes the edit leaves
    /// unusable; without it, such an edit is refused. An edit that changes
    /// nothing saves nothing and tells no one.
    pub(crate) fn edit_value(
        &mut self,
        spec: &EditSpec,
        drop_fixes: bool,
    ) -> Result<SavedValue, String> {
        self.update_app(&spec.app, Changed::Values, |owner, found| {
            let edited = owner.edited(spec, found)?;
            edited.automations?;
            if !drop_fixes && !edited.dropped.is_empty() {
                return Err(unconfirmed(&edited.value.name, &edited.dropped));
            }
            read_back(&edited.value.screen, found, &edited.pictures)?;
            let answer = SavedValue {
                app: spec.app.clone(),
                value: edited.value.id.clone(),
            };
            let values = &mut found.file.values;
            if values.get(edited.position) == Some(&edited.value) {
                return Ok(Edit::Keep(answer));
            }
            if let Some(saved) = values.get_mut(edited.position) {
                *saved = edited.value;
            }
            Ok(Edit::Save(answer, edited.pictures))
        })
        .map(|saved| saved.answer)
    }

    /// The value `spec` names in `found` as the edit leaves it.
    fn edited(&self, spec: &EditSpec, found: &SavedApp) -> Result<Edited, String> {
        let name = spec.value_name.trim();
        if name.is_empty() {
            return Err("Name the value first.".to_owned());
        }
        let file = &found.file;
        let (position, saved) = file
            .values
            .iter()
            .enumerate()
            .find(|(_, saved)| saved.id == spec.value)
            .ok_or_else(|| format!("{} no longer has that value.", file.name))?;
        named_apart(file, Some(&saved.id), name)?;
        let drawn = saved.screen.window_size;
        let (region, check, pictures) = match &spec.drawing {
            EditDrawing::Kept { check } => {
                keeps_pictures(&saved.screen.check, check)?;
                (saved.screen.region, check.clone(), Vec::new())
            },
            EditDrawing::Drawn { check, screenshot } => {
                if self.screenshots.get(*screenshot).is_none() {
                    return Err("Take a screenshot first.".to_owned());
                }
                let size = self
                    .with_session(|session| check_spec::drawn_size(check, session, *screenshot))
                    .map_err(|error| error.to_string())?;
                if (size.width, size.height) != (drawn.width, drawn.height) {
                    return Err(format!(
                        "{} was drawn at {} × {}, and this screenshot is {} × {}. Resize the \
                         window to {} × {} to move its region, or fix it at this size instead.",
                        saved.name,
                        drawn.width,
                        drawn.height,
                        size.width,
                        size.height,
                        drawn.width,
                        drawn.height
                    ));
                }
                // New pictures never take the name of one still saved, so a
                // save that fails partway leaves the saved value readable.
                let number = free_picture_number(&saved.id, saved.screen.images());
                let built = self
                    .with_session(|session| {
                        check_spec::to_saved_fix(check, session, &spec.app, &saved.id, Some(number))
                    })
                    .map_err(|error| error.to_string())?;
                (built.region, built.check, built.pictures)
            },
        };
        let (fixes, dropped) = if fixes_suit(&saved.screen.check, &check) {
            (saved.screen.fixes.clone(), Vec::new())
        } else {
            let dropped = saved
                .screen
                .fixes
                .iter()
                .map(|fix| fix.window_size)
                .collect();
            (Vec::new(), dropped)
        };
        let value = ValueDef {
            id: saved.id.clone(),
            name: name.to_owned(),
            screen: ScreenDef {
                window_size: drawn,
                region,
                check,
                pin: Some(spec.pin),
                fixes,
            },
        };
        let automations = automations::check_still_suits(self.stores()?, &spec.app, saved, &value);
        Ok(Edited {
            position,
            value,
            pictures,
            dropped,
            automations,
        })
    }
}

/// Checks that `check`, which an edit keeps the region for, names no picture
/// but the ones the saved check `saved` has: a state check keeps each saved
/// state's picture, in the saved order.
fn keeps_pictures(saved: &CheckDef, check: &CheckDef) -> Result<(), String> {
    let images = |check: &CheckDef| -> Vec<String> {
        check
            .states()
            .iter()
            .map(|state| state.image.clone())
            .collect()
    };
    if matches!(check, CheckDef::State { .. }) && images(check) != images(saved) {
        return Err(
            "A state keeps its saved picture only while the value keeps the same states in the \
             same order. Take each state again from a screenshot."
                .to_owned(),
        );
    }
    Ok(())
}

/// Whether the fixes of a value whose first drawing's check was `saved`
/// still suit it as `edited`: the same kind of check, and for a state check
/// as many states, each with the picture or the name it had, so each fix's
/// pictures still line up with the states they show.
fn fixes_suit(saved: &CheckDef, edited: &CheckDef) -> bool {
    match (saved, edited) {
        (CheckDef::State { states: had, .. }, CheckDef::State { states, .. }) => {
            had.len() == states.len()
                && had
                    .iter()
                    .zip(states)
                    .all(|(had, state)| had.image == state.image || had.name == state.name)
        },
        (CheckDef::Number { .. }, CheckDef::Number { .. })
        | (CheckDef::BarFill { .. }, CheckDef::BarFill { .. })
        | (CheckDef::Color { .. }, CheckDef::Color { .. }) => true,
        _ => false,
    }
}

/// What to say when an edit leaves the fixes of the value `name` at the
/// window sizes `dropped` unusable and the user has not agreed to lose them.
fn unconfirmed(name: &str, dropped: &[SizeDef]) -> String {
    let sizes: Vec<String> = dropped
        .iter()
        .map(|size| format!("{} × {}", size.width, size.height))
        .collect();
    let sizes: Vec<&str> = sizes.iter().map(String::as_str).collect();
    let (fixes, they) = if sizes.len() == 1 {
        ("fix", "it")
    } else {
        ("fixes", "them")
    };
    format!(
        "This change leaves the {fixes} of {name} at {} unusable, so saving removes {they}. \
         Agree to that in the editor to save.",
        and_list(&sizes)
    )
}

#[cfg(test)]
mod tests;
