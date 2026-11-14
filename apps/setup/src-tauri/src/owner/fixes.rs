//! Fixes: a saved value drawn again at another window size, which it reads
//! there exactly instead of scaled from its first drawing, and where the
//! fix flow's region starts.

use appytappy_config::app_file::{CheckDef, FixDef, ScreenDef, SizeDef, ValueDef};
use appytappy_sources::screen::Region;
use appytappy_sources::screen::saved::Readers;
use appytappy_sources::screen::scale::scaled;
use serde::Deserialize;

use appytappy_config::app_file::RegionDef;

use crate::check_spec::{self, CheckSpec};

use super::host::Host;
use super::values::{SavedValue, read_back};
use super::{Changed, Edit, Owner};

/// A saved value drawn again at another window size, as the UI describes
/// it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct FixSpec {
    /// The saved application.
    pub(crate) app: String,
    /// The value's ID.
    pub(crate) value: String,
    /// The check at this size: of the value's kind, with each of its
    /// states in order, or for a number, the reference to read with.
    pub(crate) check: CheckSpec,
    /// The screenshot the region was drawn on, whose size the fix is for.
    pub(crate) screenshot: usize,
}

impl<H: Host> Owner<H> {
    /// Where the value `value` of the application `app` starts on a window
    /// of `size` in the fix flow: its fix at that size, or its first drawing
    /// scaled to it, or `None` when that scales past the window.
    pub(crate) fn place_value(
        &self,
        app: &str,
        value: &str,
        size: SizeDef,
    ) -> Result<Option<RegionDef>, String> {
        let file = self.stores()?.app(app)?.file;
        let saved = find(&file.values, value, &file.name)?;
        Ok(start_region(&saved.screen, size))
    }

    /// Saves `spec` as the fix of its value at its screenshot's size,
    /// replacing any fix it had at that size, and answers where it was
    /// saved, or what to fix. The pictures of the fix it replaced go.
    pub(crate) fn save_fix(&mut self, spec: &FixSpec) -> Result<SavedValue, String> {
        // Without the stores, nothing else the user could fix matters.
        self.stores()?;
        if self.capture.screenshots().get(spec.screenshot).is_none() {
            return Err("Take a screenshot first.".to_owned());
        }
        let size = check_spec::drawn_size(&spec.check, self.capture.screenshots(), spec.screenshot)
            .map_err(|error| error.to_string())?;
        self.update_app(&spec.app, Changed::Values, |owner, found| {
            let file = &found.file;
            let position = file
                .values
                .iter()
                .position(|saved| saved.id == spec.value)
                .ok_or_else(|| format!("{} no longer has that value.", file.name))?;
            let value = file
                .values
                .get(position)
                .ok_or_else(|| format!("{} no longer has that value.", file.name))?;
            let window_size = SizeDef::from(size);
            if window_size == value.screen.window_size {
                return Err(format!(
                    "{} was drawn at this size, so it needs no fix here. Resize the window first.",
                    value.name
                ));
            }
            same_kind(&value.screen.check, &spec.check)?;
            let number = free_picture_number(&value.id, value.screen.images());
            let saved = check_spec::to_saved_fix(
                &spec.check,
                owner.capture.screenshots(),
                &spec.app,
                &value.id,
                Some(number),
            )
            .map_err(|error| error.to_string())?;
            let reference = match (&value.screen.check, &saved.check) {
                (CheckDef::Number { reference, .. }, CheckDef::Number { reference: own, .. })
                    if own != reference =>
                {
                    Some(own.clone())
                },
                _ => None,
            };
            let fix = FixDef {
                window_size,
                region: saved.region,
                images: saved
                    .check
                    .states()
                    .iter()
                    .map(|state| state.image.clone())
                    .collect(),
                reference,
            };
            let mut screen = value.screen.clone();
            match screen
                .fixes
                .iter_mut()
                .find(|fix| fix.window_size == window_size)
            {
                Some(replaced) => *replaced = fix,
                None => screen.fixes.push(fix),
            }
            read_back(
                &screen,
                found,
                Readers::new(&found.file.references),
                &saved.pictures,
            )?;
            let id = value.id.clone();
            if let Some(value) = found.file.values.get_mut(position) {
                value.screen = screen;
            }
            Ok(Edit::Save(
                SavedValue {
                    app: spec.app.clone(),
                    value: id,
                },
                saved.pictures,
            ))
        })
        .map(|saved| saved.answer)
    }

    /// Removes the fix of the value `value` of the application `app` at the
    /// window size `size`, so the value reads there as its application's
    /// size changes say.
    pub(crate) fn remove_fix(
        &mut self,
        app: &str,
        value: &str,
        size: SizeDef,
    ) -> Result<(), String> {
        self.update_app(app, Changed::Values, |_owner, found| {
            let file = &mut found.file;
            let name = file.name.clone();
            let saved = file
                .values
                .iter_mut()
                .find(|saved| saved.id == value)
                .ok_or_else(|| format!("{name} no longer has that value."))?;
            let before = saved.screen.fixes.len();
            saved.screen.fixes.retain(|fix| fix.window_size != size);
            if saved.screen.fixes.len() == before {
                return Err(format!(
                    "{} has no fix at {} × {} any more.",
                    saved.name, size.width, size.height
                ));
            }
            Ok(Edit::Save((), Vec::new()))
        })
        .map(|saved| saved.answer)
    }
}

/// The saved value `id` of `values`, or what to say when the application
/// `name` no longer has it.
fn find<'file>(values: &'file [ValueDef], id: &str, name: &str) -> Result<&'file ValueDef, String> {
    values
        .iter()
        .find(|saved| saved.id == id)
        .ok_or_else(|| format!("{name} no longer has that value."))
}

/// Where `screen` starts on a window of `size`: its fix at that size, or
/// its first drawing scaled to it, as the runtime would read it while the
/// window changes size often.
fn start_region(screen: &ScreenDef, size: SizeDef) -> Option<RegionDef> {
    if let Some(fix) = screen.fixes.iter().find(|fix| fix.window_size == size) {
        return Some(fix.region);
    }
    scaled(
        Region::try_from(screen.region).ok()?,
        screen.window_size.into(),
        size.into(),
        screen.pin(),
    )
    .map(RegionDef::from)
}

/// Checks that `spec` reads a value whose first drawing's check is `check`
/// the same way: the same kind, and for a state check, its states in the
/// same order.
fn same_kind(check: &CheckDef, spec: &CheckSpec) -> Result<(), String> {
    match (check, spec) {
        (CheckDef::State { states, .. }, CheckSpec::State { states: taken, .. }) => {
            let same = states.len() == taken.len()
                && states
                    .iter()
                    .zip(taken)
                    .all(|(state, taken)| state.name == taken.name);
            if same {
                Ok(())
            } else {
                let names: Vec<&str> = states.iter().map(|state| state.name.as_str()).collect();
                Err(format!(
                    "Take a picture of each of the value's states, in order: {}.",
                    names.join(", ")
                ))
            }
        },
        (CheckDef::Number { .. }, CheckSpec::Number { .. })
        | (CheckDef::BarFill { .. }, CheckSpec::BarFill { .. })
        | (CheckDef::Color { .. }, CheckSpec::Color { .. }) => Ok(()),
        _ => Err("A fix reads the value the way it was first drawn. Pick that kind.".to_owned()),
    }
}

/// The smallest number from 1 that none of `images`, the value `id`'s
/// pictures, is named with, so the pictures of a new fix, or of a drawing
/// whose states are taken again, never overwrite those still saved, the
/// ones they replace included.
pub(super) fn free_picture_number<'file>(
    id: &str,
    images: impl Iterator<Item = &'file str>,
) -> usize {
    let prefix = format!("{id}-");
    let taken: Vec<usize> = images
        .filter_map(|image| {
            let (number, _) = image.strip_prefix(&prefix)?.split_once('-')?;
            number.parse().ok()
        })
        .collect();
    // One more number is counted than are taken, so one is free.
    let last = taken.len().saturating_add(1);
    (1..=last)
        .find(|number| !taken.contains(number))
        .unwrap_or(last)
}

#[cfg(test)]
mod tests;
