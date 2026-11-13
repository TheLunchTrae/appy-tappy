//! Which running window belongs to an app.
//!
//! An app is found by a [`WindowMatch`] rather than an exact executable
//! name: patterns on the window's title, class, and executable. The
//! executable always has to match, since the runtime watches an app's
//! process; the title and class may be required too, and a fallback says
//! whether any window of the executable will do when none matches everything
//! required. The setup app fills one in from the window the user picked, the
//! user may loosen it with wildcards, and the runtime finds the app's window
//! with the same rules.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A text pattern in which `*` matches any run of characters and `?` matches
/// one character. Case is ignored.
///
/// A pattern made from a text always matches that text, because a `*` or `?`
/// in the text also matches itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    /// The pattern as written.
    text: String,
    /// The pattern's characters, lowercased, as matching compares them.
    folded: Vec<char>,
}

impl Pattern {
    /// The pattern `text`.
    #[must_use]
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            folded: text.to_lowercase().chars().collect(),
        }
    }

    /// The pattern as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Whether `candidate` matches, ignoring case.
    #[must_use]
    pub fn matches(&self, candidate: &str) -> bool {
        let folded: Vec<char> = candidate.to_lowercase().chars().collect();
        glob(&self.folded, &folded)
    }
}

impl fmt::Display for Pattern {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.text)
    }
}

/// Whether `text` matches `pattern`. On a mismatch after a `*`, the `*` takes
/// one more character and matching resumes, so the work stays near the
/// product of the two lengths at worst.
fn glob(mut pattern: &[char], mut text: &[char]) -> bool {
    // The pattern after the last `*`, and where in the text that `*` stops.
    let mut star: Option<(&[char], &[char])> = None;
    loop {
        match (pattern.split_first(), text.split_first()) {
            (Some(('*', after)), _) => {
                star = Some((after, text));
                pattern = after;
            },
            (Some((&wanted, after)), Some((&found, rest))) if wanted == '?' || wanted == found => {
                pattern = after;
                text = rest;
            },
            (None, None) => return true,
            _ => {
                let Some((after, stop)) = star else {
                    return false;
                };
                let Some((_, later)) = stop.split_first() else {
                    return false;
                };
                star = Some((after, later));
                pattern = after;
                text = later;
            },
        }
    }
}

/// Which fields besides the executable a window has to match to qualify,
/// as an application's file spells it too.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MustMatch {
    /// The title has to match.
    pub title: bool,
    /// The class has to match.
    pub class: bool,
}

/// How well a window fits a [`WindowMatch`]; the better fits sort first.
///
/// A window that qualifies beats one taken only as the fallback. Then the
/// window matching more fields wins, and a tie goes to the one matching the
/// title.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Fit {
    fallback: bool,
    misses: usize,
    /// Whether the title and the class missed, in that order, so a tie goes
    /// to the window matching the title.
    missed: [bool; 2],
}

impl Fit {
    /// Whether the window was taken only because nothing qualified.
    #[must_use]
    pub const fn is_fallback(self) -> bool {
        self.fallback
    }
}

/// The facts about a window that a [`WindowMatch`] reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowFacts<'window> {
    /// The window's title.
    pub title: &'window str,
    /// The window's class name.
    pub class: &'window str,
    /// The file name of the window's executable, such as `Example.exe`.
    pub executable: &'window str,
}

/// Which running window belongs to an app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowMatch {
    /// The window's title.
    pub title: Pattern,
    /// The window's class name.
    pub class: Pattern,
    /// The file name of the window's executable, which always has to match.
    pub executable: Pattern,
    /// The fields besides the executable a window has to match to qualify.
    pub must_match: MustMatch,
    /// Whether any window of the executable will do when none qualifies;
    /// otherwise the app counts as closed.
    pub fallback: bool,
}

impl WindowMatch {
    /// A match for `window` that requires only its executable, which suits
    /// the many apps whose title changes as they run; the title and class
    /// still pick between the executable's windows.
    #[must_use]
    pub fn exact(window: WindowFacts<'_>) -> Self {
        Self {
            title: Pattern::new(window.title),
            class: Pattern::new(window.class),
            executable: Pattern::new(window.executable),
            must_match: MustMatch {
                title: false,
                class: false,
            },
            fallback: false,
        }
    }

    /// How well `window` fits, or `None` when its executable differs, or it
    /// neither qualifies nor may be taken as the fallback.
    #[must_use]
    pub fn fit(&self, window: WindowFacts<'_>) -> Option<Fit> {
        if !self.executable.matches(window.executable) {
            return None;
        }
        let title = self.title.matches(window.title);
        let class = self.class.matches(window.class);
        let qualifies = (title || !self.must_match.title) && (class || !self.must_match.class);
        (qualifies || self.fallback).then(|| {
            let missed = [!title, !class];
            Fit {
                fallback: !qualifies,
                misses: missed.iter().filter(|&&miss| miss).count(),
                missed,
            }
        })
    }

    /// The positions of the windows this takes, with their fits, the best
    /// fits first and in their given order within a fit. The fallback takes
    /// windows only when none qualifies.
    #[must_use]
    pub fn rank<'window>(
        &self,
        windows: impl IntoIterator<Item = WindowFacts<'window>>,
    ) -> Vec<(usize, Fit)> {
        let mut fits: Vec<(Fit, usize)> = windows
            .into_iter()
            .enumerate()
            .filter_map(|(position, window)| Some((self.fit(window)?, position)))
            .collect();
        if fits.iter().any(|(fit, _)| !fit.is_fallback()) {
            fits.retain(|(fit, _)| !fit.is_fallback());
        }
        fits.sort_unstable();
        fits.into_iter()
            .map(|(fit, position)| (position, fit))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards_match_runs_and_single_characters() {
        let cases = [
            ("Example App", "Example App", true),
            ("example app", "EXAMPLE APP", true),
            ("Example*", "Example App", true),
            ("*App", "Example App", true),
            ("*amp*", "Example App", true),
            ("E?ample App", "Example App", true),
            ("*", "", true),
            ("", "", true),
            ("**a*", "banana", true),
            ("a*a*a", "banana", false),
            ("b*n*a", "banana", true),
            ("?", "", false),
            ("Example", "Example App", false),
            ("App", "Example App", false),
            ("*x", "Example App", false),
            ("", "x", false),
            ("Page ? of 3", "Page 12 of 3", false),
        ];
        for (pattern, text, expected) in cases {
            assert_eq!(
                Pattern::new(pattern).matches(text),
                expected,
                "{pattern:?} on {text:?}"
            );
        }
    }

    #[test]
    fn a_pattern_keeps_its_text_and_matches_it() {
        for text in ["Page 3 * draft", "What? Me?", "Media Player", ""] {
            let pattern = Pattern::new(text);
            assert_eq!(pattern.as_str(), text);
            assert_eq!(pattern.to_string(), text);
            assert!(pattern.matches(text), "{text:?}");
        }
    }

    const WINDOW: WindowFacts<'static> = WindowFacts {
        title: "Example App",
        class: "ExampleWindow",
        executable: "Example.exe",
    };

    fn with(
        title: &'static str,
        class: &'static str,
        executable: &'static str,
    ) -> WindowFacts<'static> {
        WindowFacts {
            title,
            class,
            executable,
        }
    }

    fn requiring(title: bool, class: bool) -> WindowMatch {
        WindowMatch {
            must_match: MustMatch { title, class },
            ..WindowMatch::exact(WINDOW)
        }
    }

    fn positions(matcher: &WindowMatch, windows: &[WindowFacts<'static>]) -> Vec<usize> {
        matcher
            .rank(windows.iter().copied())
            .into_iter()
            .map(|(position, _)| position)
            .collect()
    }

    #[test]
    fn a_window_qualifies_only_by_matching_its_executable_and_every_required_field() {
        let renamed = with("Example App - Project 1", "ExampleWindow", "Example.exe");
        let dialog = with("Example App", "#32770", "Example.exe");
        let other = with("Example App", "ExampleWindow", "Other.exe");
        let cases = [
            ((false, false), [true, true, true, false]),
            ((false, true), [true, true, false, false]),
            ((true, false), [true, false, true, false]),
            ((true, true), [true, false, false, false]),
        ];
        for ((title, class), expected) in cases {
            let matcher = requiring(title, class);
            let found =
                [WINDOW, renamed, dialog, other].map(|window| matcher.fit(window).is_some());
            assert_eq!(found, expected, "requiring {title} {class}");
        }
    }

    #[test]
    fn a_new_match_requires_only_the_executable() {
        let matcher = WindowMatch::exact(WINDOW);
        assert_eq!(
            matcher.must_match,
            MustMatch {
                title: false,
                class: false
            }
        );
        assert!(!matcher.fallback);
    }

    #[test]
    fn the_fallback_takes_any_window_of_the_executable_only_when_none_qualifies() {
        let mut matcher = requiring(true, true);
        let other_class = with("Something Else", "Other", "Example.exe");
        let other_program = with("Example App", "ExampleWindow", "Other.exe");
        assert_eq!(matcher.fit(other_class), None);
        matcher.fallback = true;
        assert_eq!(matcher.fit(other_class).map(Fit::is_fallback), Some(true));
        assert_eq!(matcher.fit(other_program), None);
        assert_eq!(matcher.fit(WINDOW).map(Fit::is_fallback), Some(false));
        assert_eq!(positions(&matcher, &[other_class, WINDOW]), [1]);
        let alone = matcher.rank([other_class, other_program]);
        assert_eq!(alone.len(), 1);
        assert!(
            alone
                .iter()
                .all(|(position, fit)| *position == 0 && fit.is_fallback())
        );
    }

    #[test]
    fn a_loosened_title_fits_fully() {
        let mut matcher = requiring(true, true);
        matcher.title = Pattern::new("Example App*");
        let renamed = with("Example App - Project 1", "ExampleWindow", "example.EXE");
        assert_eq!(matcher.fit(renamed), matcher.fit(WINDOW));
    }

    #[test]
    fn ranking_prefers_more_fields_then_the_title() {
        let matcher = WindowMatch::exact(WINDOW);
        let windows = [
            with("Settings", "#32770", "Example.exe"),
            with("Media Player", "PlayerWindow", "Player.exe"),
            with("Example App - Settings", "ExampleWindow", "Example.exe"),
            WINDOW,
            with("Another", "#32770", "Example.exe"),
            with("Example App", "#32770", "Example.exe"),
            with("Settings", "#32770", "example.exe"),
        ];
        assert_eq!(positions(&matcher, &windows), [3, 5, 2, 0, 4, 6]);
        assert!(
            matcher
                .rank(windows)
                .iter()
                .all(|(_, fit)| !fit.is_fallback())
        );
        assert_eq!(matcher.rank([]), Vec::new());
    }
}
