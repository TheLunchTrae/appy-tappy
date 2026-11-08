//! The thing an image check looks for, as the user selects it.
//!
//! The UI sends the steps the user took on a screenshot: a rough box to snap
//! to the thing inside it, or a click to grow over the thing's colors, each
//! replacing the selection, adding to it, or taking from it. [`select`]
//! replays them, so the same steps give the same selection on the screenshot
//! the check is built from as on the one the UI showed.

use std::ops::Range;

use appytappy_sources::screen::select::{Selection, grow, snap};
use appytappy_sources::screen::{Image, Region};
use serde::{Deserialize, Serialize};

use crate::check_spec::RegionSpec;

/// How a step's pick combines with the selection before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Combine {
    /// The pick becomes the selection.
    #[default]
    Replace,
    /// The pick joins the selection.
    Add,
    /// The pick leaves the selection.
    Remove,
}

/// One thing the user did to select.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum SelectStep {
    /// Dragged a rough box, which snaps to the thing inside it.
    Snap {
        region: RegionSpec,
        #[serde(default)]
        combine: Combine,
    },
    /// Clicked a point, which grows over the colors like it.
    Click {
        x: u32,
        y: u32,
        #[serde(default)]
        combine: Combine,
    },
}

/// What the user selected, as the steps they took.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct SelectionSpec {
    /// How far a color may be from another and still count as alike.
    pub(crate) tolerance: u8,
    /// The steps, in the order the user took them.
    pub(crate) steps: Vec<SelectStep>,
}

/// The selection `spec`'s steps make on `image`, or `None` when they leave
/// nothing selected.
#[must_use]
pub(crate) fn select(image: &Image, spec: &SelectionSpec) -> Option<Selection> {
    spec.steps.iter().fold(None, |current, step| {
        let (picked, combine) = match *step {
            SelectStep::Snap { region, combine } => (
                Region::new(region.x, region.y, region.width, region.height)
                    .and_then(|rough| snap(image, rough, spec.tolerance)),
                combine,
            ),
            SelectStep::Click { x, y, combine } => (grow(image, x, y, spec.tolerance), combine),
        };
        match (combine, current, picked) {
            (Combine::Replace, _, picked) => picked,
            (Combine::Add, Some(current), Some(picked)) => Some(current.union(&picked)),
            (Combine::Add, current, picked) => current.or(picked),
            (Combine::Remove, Some(current), Some(picked)) => current.without(&picked),
            (Combine::Remove, current, None) => current,
            (Combine::Remove, None, Some(_picked)) => None,
        }
    })
}

/// A selection as the UI draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SelectionView {
    /// The box around the selected pixels.
    region: RegionSpec,
    /// The runs of selected pixels in the box, each as the index of its
    /// first pixel and of the pixel after its last, row by row from the top.
    spans: Vec<[usize; 2]>,
}

impl From<&Selection> for SelectionView {
    fn from(selection: &Selection) -> Self {
        let region = selection.region();
        let size = region.size();
        Self {
            region: RegionSpec {
                x: region.x(),
                y: region.y(),
                width: size.width,
                height: size.height,
            },
            spans: selection
                .mask()
                .spans()
                .into_iter()
                .map(|Range { start, end }| [start, end])
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use appytappy_sources::screen::Size;

    use super::*;

    /// A 6x4 picture on a black background, with a white 2x2 square at
    /// (1, 1) and a white pixel at (4, 3).
    fn picture() -> Image {
        let rows = ["......", ".##...", ".##...", "....#."];
        let mut bytes = Vec::new();
        for row in rows {
            for character in row.chars() {
                let value = if character == '#' { 255 } else { 0 };
                bytes.extend_from_slice(&[value, value, value, 0]);
            }
        }
        Image::new(
            Size {
                width: 6,
                height: 4,
            },
            bytes,
        )
        .unwrap()
    }

    fn region(x: u32, y: u32, width: u32, height: u32) -> RegionSpec {
        RegionSpec {
            x,
            y,
            width,
            height,
        }
    }

    fn view(steps: Vec<SelectStep>) -> Option<SelectionView> {
        select(
            &picture(),
            &SelectionSpec {
                tolerance: 10,
                steps,
            },
        )
        .as_ref()
        .map(SelectionView::from)
    }

    fn click(x: u32, y: u32, combine: Combine) -> SelectStep {
        SelectStep::Click { x, y, combine }
    }

    #[test]
    fn a_snap_or_a_click_selects_the_thing() {
        let square = Some(SelectionView {
            region: region(1, 1, 2, 2),
            spans: vec![[0, 4]],
        });
        assert_eq!(
            view(vec![SelectStep::Snap {
                region: region(0, 0, 4, 4),
                combine: Combine::Replace,
            }]),
            square
        );
        assert_eq!(view(vec![click(2, 2, Combine::Replace)]), square);
        assert_eq!(view(Vec::new()), None, "no step selects nothing");
    }

    #[test]
    fn later_steps_replace_add_to_or_take_from_the_selection() {
        assert_eq!(
            view(vec![
                click(1, 1, Combine::Replace),
                click(4, 3, Combine::Replace)
            ]),
            Some(SelectionView {
                region: region(4, 3, 1, 1),
                spans: vec![[0, 1]],
            })
        );
        assert_eq!(
            view(vec![
                click(1, 1, Combine::Replace),
                click(4, 3, Combine::Add)
            ]),
            Some(SelectionView {
                region: region(1, 1, 4, 3),
                spans: vec![[0, 2], [4, 6], [11, 12]],
            })
        );
        assert_eq!(
            view(vec![
                click(1, 1, Combine::Replace),
                click(4, 3, Combine::Add),
                click(2, 1, Combine::Remove),
            ]),
            Some(SelectionView {
                region: region(4, 3, 1, 1),
                spans: vec![[0, 1]],
            })
        );
        assert_eq!(
            view(vec![
                click(1, 1, Combine::Replace),
                click(2, 2, Combine::Remove)
            ]),
            None,
            "taking away all of it leaves nothing"
        );
    }

    #[test]
    fn a_step_that_picks_nothing_combines_as_nothing() {
        let off = click(9, 9, Combine::Replace);
        assert_eq!(view(vec![click(1, 1, Combine::Replace), off]), None);
        assert_eq!(
            view(vec![
                click(1, 1, Combine::Replace),
                click(9, 9, Combine::Add)
            ]),
            view(vec![click(1, 1, Combine::Replace)])
        );
        assert_eq!(
            view(vec![
                click(1, 1, Combine::Replace),
                click(9, 9, Combine::Remove)
            ]),
            view(vec![click(1, 1, Combine::Replace)])
        );
        assert_eq!(
            view(vec![click(9, 9, Combine::Add)]),
            None,
            "adding nothing to nothing"
        );
        assert_eq!(
            view(vec![click(2, 2, Combine::Remove)]),
            None,
            "taking from nothing"
        );
        assert_eq!(
            view(vec![
                click(9, 9, Combine::Replace),
                click(4, 3, Combine::Add)
            ]),
            Some(SelectionView {
                region: region(4, 3, 1, 1),
                spans: vec![[0, 1]],
            }),
            "adding to nothing is the pick"
        );
    }

    #[test]
    fn steps_read_from_the_ui() {
        let spec: SelectionSpec = serde_json::from_str(
            r#"{"tolerance":12,"steps":[
                {"kind":"snap","region":{"x":0,"y":0,"width":4,"height":4}},
                {"kind":"click","x":4,"y":3,"combine":"add"},
                {"kind":"click","x":1,"y":1,"combine":"remove"}]}"#,
        )
        .unwrap();
        assert_eq!(
            spec,
            SelectionSpec {
                tolerance: 12,
                steps: vec![
                    SelectStep::Snap {
                        region: region(0, 0, 4, 4),
                        combine: Combine::Replace,
                    },
                    click(4, 3, Combine::Add),
                    click(1, 1, Combine::Remove),
                ],
            }
        );
        assert_eq!(
            serde_json::to_string(&view(vec![click(4, 3, Combine::Replace)])).unwrap(),
            r#"{"region":{"x":4,"y":3,"width":1,"height":1},"spans":[[0,1]]}"#
        );
    }
}
