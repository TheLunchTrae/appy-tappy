//! How a check came to its reading, for the setup app's debug details.
//!
//! Working it out costs more than a read, such as scoring each character
//! against every learned glyph, so the runtime never asks for it. Each part
//! comes from the calculation its check decides with, so the workings and
//! the reading always agree.

use appytappy_config::signal::StateName;

use super::Image;
use super::check::{Check, CheckError, Difference};
use super::digits::CharacterScore;
use crate::system::Rgb;

/// How a check came to its reading of one region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Workings {
    /// Each character's closest glyphs, left to right.
    Number(Vec<CharacterScore>),
    /// The bar's pixels: `filled` of the `counted` that are near either
    /// color. The fill is `filled` out of `counted`, before rounding.
    BarFill {
        /// The pixels nearer the filled color.
        filled: u64,
        /// The pixels near either color.
        counted: u64,
    },
    /// The region's average color and the color it is compared with.
    Color {
        /// The region's average color.
        average: Rgb,
        /// The color the check looks for.
        color: Rgb,
        /// How far each of red, green, and blue may be from it.
        tolerance: u8,
    },
    /// Each state's distance from the region, in the check's order.
    State(Vec<StateScore>),
}

/// How far a region is from one state's picture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateScore {
    /// The state.
    pub name: StateName,
    /// The summed difference of the compared red, green, and blue channels.
    pub total: u64,
    /// How many channels were compared; the average difference is `total`
    /// over this.
    pub channels: u64,
    /// The largest average difference at which the region shows the state.
    pub tolerance: u8,
}

impl StateScore {
    /// Whether the region is within the state's tolerance, as a read
    /// judges it. A read takes the closest state that is.
    #[must_use]
    pub fn within(&self) -> bool {
        Difference {
            total: self.total,
            channels: self.channels,
        }
        .within(self.tolerance)
    }
}

impl Check {
    /// How this check reads `image`: the scores behind [`Check::read`].
    ///
    /// # Errors
    ///
    /// Returns the [`CheckError`] a read would, apart from a number with no
    /// digits or an unknown character, whose scores still show why, and a
    /// bar with no pixel near either color, which shows as none counted.
    pub fn explain(&self, image: &Image) -> Result<Workings, CheckError> {
        match self {
            Self::Number { reader, .. } => reader
                .explain(image)
                .map(Workings::Number)
                .map_err(CheckError::Number),
            Self::BarFill(bar) => {
                let (filled, counted) = bar.count(image);
                Ok(Workings::BarFill { filled, counted })
            },
            Self::Color(color) => Ok(Workings::Color {
                average: super::check::ColorMatch::average(image),
                color: color.color,
                tolerance: color.tolerance,
            }),
            Self::State(states) => states
                .states
                .iter()
                .map(|(name, picture)| {
                    let difference = picture.difference(image)?;
                    Ok(StateScore {
                        name: name.clone(),
                        total: difference.total,
                        channels: difference.channels,
                        tolerance: picture.tolerance,
                    })
                })
                .collect::<Result<_, _>>()
                .map(Workings::State),
        }
    }
}

#[cfg(test)]
mod tests {
    use appytappy_config::signal::Value;

    use super::*;
    use crate::screen::Size;
    use crate::screen::check::{BarFill, ColorMatch, ImageMatch, StateMatch};
    use crate::screen::digits::DigitReader;

    const WHITE: Rgb = Rgb {
        red: 255,
        green: 255,
        blue: 255,
    };
    const BLACK: Rgb = Rgb {
        red: 0,
        green: 0,
        blue: 0,
    };

    /// Draws `rows` of `#` (white) and `.` (black) as an image.
    fn draw(rows: &[&str]) -> Image {
        let mut bgra = Vec::new();
        for row in rows {
            for cell in row.chars() {
                let value = if cell == '#' { 255 } else { 0 };
                bgra.extend_from_slice(&[value, value, value, 255]);
            }
        }
        let size = Size {
            width: u32::try_from(rows.first().map_or(0, |row| row.len())).expect("fits"),
            height: u32::try_from(rows.len()).expect("fits"),
        };
        Image::new(size, bgra).expect("matching length")
    }

    /// "1" then "7", a pixel apart.
    const ONE_SEVEN: [&str; 5] = [
        "#.###", //
        "#...#", //
        "#...#", //
        "#...#", //
        "#...#", //
    ];

    #[test]
    fn a_number_scores_each_character_against_its_glyphs() {
        let mut reader = DigitReader::new(WHITE, 10);
        reader.learn(&draw(&ONE_SEVEN), "17").expect("learn");
        let check = Check::Number {
            reader,
            whole: false,
        };
        let image = draw(&ONE_SEVEN);
        assert_eq!(
            check.read(&image),
            Ok(Value::Number(
                appytappy_config::signal::Number::new(17.0).expect("a number")
            ))
        );
        let Ok(Workings::Number(characters)) = check.explain(&image) else {
            panic!("number workings");
        };
        let read: String = characters
            .iter()
            .filter_map(|score| score.best.filter(|best| best.matches()))
            .map(|best| best.character)
            .collect();
        assert_eq!(read, "17", "the best matches are what a read reads");
        let one = characters.first().expect("a 1");
        assert_eq!(one.best.map(|best| best.differing), Some(0));
        let runner_up = one.runner_up.expect("a 7 as runner-up");
        assert_eq!(runner_up.character, '7');
        assert!(runner_up.differing > 0, "{runner_up:?}");
        assert_eq!(
            check.explain(&draw(&["....."])),
            Err(CheckError::Number(super::super::digits::ReadError::NoInk))
        );
    }

    #[test]
    fn a_number_with_one_character_has_no_runner_up() {
        let mut reader = DigitReader::new(WHITE, 10);
        reader.learn(&draw(&["#", "#", "#"]), "1").expect("learn");
        let Ok(Workings::Number(characters)) = (Check::Number {
            reader,
            whole: false,
        })
        .explain(&draw(&["#", "#", "#"])) else {
            panic!("number workings");
        };
        assert_eq!(characters.len(), 1);
        assert_eq!(characters.first().and_then(|score| score.runner_up), None);
    }

    #[test]
    fn a_bar_counts_its_filled_and_counted_pixels() {
        let check = Check::BarFill(BarFill::new(WHITE, BLACK).expect("two colors"));
        let image = draw(&["###."]);
        assert_eq!(
            check.explain(&image),
            Ok(Workings::BarFill {
                filled: 3,
                counted: 4
            })
        );
        assert_eq!(
            check.read(&image),
            Ok(Value::Number(
                appytappy_config::signal::Number::new(75.0).expect("a number")
            ))
        );
    }

    #[test]
    fn a_color_shows_the_average_it_compared() {
        let color = ColorMatch {
            color: WHITE,
            tolerance: 3,
        };
        let check = Check::Color(color);
        let image = draw(&["##.."]);
        let half = Rgb {
            red: 127,
            green: 127,
            blue: 127,
        };
        assert_eq!(
            check.explain(&image),
            Ok(Workings::Color {
                average: half,
                color: WHITE,
                tolerance: 3
            })
        );
        assert_eq!(check.read(&image), Ok(Value::Flag(false)));
    }

    #[test]
    fn a_state_scores_each_picture() {
        let on = StateName::new("On").expect("a name");
        let off = StateName::new("Off").expect("a name");
        let states = StateMatch::new(vec![
            (
                on.clone(),
                ImageMatch::new(draw(&["##"]), None, 10).expect("a picture"),
            ),
            (
                off.clone(),
                ImageMatch::new(draw(&[".."]), None, 10).expect("a picture"),
            ),
        ]);
        let check = Check::State(states);
        let image = draw(&["#."]);
        assert_eq!(
            check.explain(&image),
            Ok(Workings::State(vec![
                StateScore {
                    name: on,
                    total: 765,
                    channels: 6,
                    tolerance: 10
                },
                StateScore {
                    name: off,
                    total: 765,
                    channels: 6,
                    tolerance: 10
                },
            ]))
        );
        assert_eq!(check.read(&image), Ok(Value::State(StateName::none())));
        let Ok(Workings::State(scores)) = check.explain(&draw(&["##"])) else {
            panic!("state scores");
        };
        assert_eq!(
            scores.iter().map(StateScore::within).collect::<Vec<_>>(),
            [true, false],
            "within as the read judges it"
        );
        assert_eq!(
            check.read(&draw(&["##"])),
            Ok(Value::State(StateName::new("On").expect("a name")))
        );
        assert!(
            check.explain(&draw(&["###"])).is_err(),
            "another size cannot be compared"
        );
    }
}
