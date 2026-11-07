//! Which running window belongs to an app.
//!
//! An app is found by a [`WindowMatch`] rather than an exact executable
//! name, the way OBS finds a window: patterns on the window's title, class,
//! and executable, and a [`MatchPriority`] that says how far to fall back
//! when no window fits all three. The setup app fills one in from the window
//! the user picked, the user may loosen it with wildcards, and the runtime
//! finds the app's window with the same rules.

use std::fmt;

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

/// How far a [`WindowMatch`] falls back when no window fits all its
/// patterns, like OBS's window match priority. The executable always has to
/// match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchPriority {
    /// Only a window that fits the title, class, and executable.
    Title,
    /// Otherwise a window of the same class and executable.
    TitleElseClass,
    /// Otherwise any window of the same executable, for an app whose title
    /// changes as it runs.
    TitleElseExecutable,
}

/// How well a window fits a [`WindowMatch`]; the better fits sort first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Fit {
    /// The title, class, and executable all match.
    Full,
    /// The class and executable match.
    ClassAndExecutable,
    /// Only the executable matches.
    Executable,
}

/// The facts about a window that a [`WindowMatch`] reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowFacts<'window> {
    /// The window's title.
    pub title: &'window str,
    /// The window's class name.
    pub class: &'window str,
    /// The file name of the window's executable, such as `GeometryDash.exe`.
    pub executable: &'window str,
}

/// Which running window belongs to an app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowMatch {
    /// The window's title.
    pub title: Pattern,
    /// The window's class name.
    pub class: Pattern,
    /// The file name of the window's executable.
    pub executable: Pattern,
    /// How far to fall back when no window fits all three.
    pub priority: MatchPriority,
}

impl WindowMatch {
    /// A match for exactly `window`, falling back to its executable, which
    /// suits the many apps whose title changes as they run.
    #[must_use]
    pub fn exact(window: WindowFacts<'_>) -> Self {
        Self {
            title: Pattern::new(window.title),
            class: Pattern::new(window.class),
            executable: Pattern::new(window.executable),
            priority: MatchPriority::TitleElseExecutable,
        }
    }

    /// How well `window` fits, or `None` when the priority does not accept
    /// it.
    #[must_use]
    pub fn fit(&self, window: WindowFacts<'_>) -> Option<Fit> {
        if !self.executable.matches(window.executable) {
            return None;
        }
        let class = self.class.matches(window.class);
        let fit = if class && self.title.matches(window.title) {
            Fit::Full
        } else if class {
            Fit::ClassAndExecutable
        } else {
            Fit::Executable
        };
        let loosest = match self.priority {
            MatchPriority::Title => Fit::Full,
            MatchPriority::TitleElseClass => Fit::ClassAndExecutable,
            MatchPriority::TitleElseExecutable => Fit::Executable,
        };
        (fit <= loosest).then_some(fit)
    }

    /// The positions of the windows that fit, the best fits first and in
    /// their given order within a fit.
    #[must_use]
    pub fn rank<'window>(
        &self,
        windows: impl IntoIterator<Item = WindowFacts<'window>>,
    ) -> Vec<usize> {
        let mut fits: Vec<(Fit, usize)> = windows
            .into_iter()
            .enumerate()
            .filter_map(|(position, window)| Some((self.fit(window)?, position)))
            .collect();
        fits.sort_unstable();
        fits.into_iter().map(|(_, position)| position).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards_match_runs_and_single_characters() {
        let cases = [
            ("Geometry Dash", "Geometry Dash", true),
            ("geometry dash", "GEOMETRY DASH", true),
            ("Geometry*", "Geometry Dash", true),
            ("*Dash", "Geometry Dash", true),
            ("*met*", "Geometry Dash", true),
            ("G?ometry Dash", "Geometry Dash", true),
            ("*", "", true),
            ("", "", true),
            ("**a*", "banana", true),
            ("a*a*a", "banana", false),
            ("b*n*a", "banana", true),
            ("?", "", false),
            ("Geometry", "Geometry Dash", false),
            ("Dash", "Geometry Dash", false),
            ("*x", "Geometry Dash", false),
            ("", "x", false),
            ("Level ? of 3", "Level 12 of 3", false),
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
        for text in ["Level 3 * practice", "What? Me?", "Spotify Premium", ""] {
            let pattern = Pattern::new(text);
            assert_eq!(pattern.as_str(), text);
            assert_eq!(pattern.to_string(), text);
            assert!(pattern.matches(text), "{text:?}");
        }
    }

    const DASH: WindowFacts<'static> = WindowFacts {
        title: "Geometry Dash",
        class: "GLFW30",
        executable: "GeometryDash.exe",
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

    #[test]
    fn the_priority_sets_how_far_matching_falls_back() {
        let renamed = with(
            "Geometry Dash - Stereo Madness",
            "GLFW30",
            "GeometryDash.exe",
        );
        let dialog = with("Geometry Dash", "#32770", "GeometryDash.exe");
        let other = with("Geometry Dash", "GLFW30", "Other.exe");
        let mut matcher = WindowMatch::exact(DASH);
        assert_eq!(matcher.priority, MatchPriority::TitleElseExecutable);
        assert_eq!(matcher.fit(DASH), Some(Fit::Full));
        assert_eq!(matcher.fit(renamed), Some(Fit::ClassAndExecutable));
        assert_eq!(matcher.fit(dialog), Some(Fit::Executable));
        assert_eq!(matcher.fit(other), None);

        matcher.priority = MatchPriority::TitleElseClass;
        assert_eq!(matcher.fit(renamed), Some(Fit::ClassAndExecutable));
        assert_eq!(matcher.fit(dialog), None);

        matcher.priority = MatchPriority::Title;
        assert_eq!(matcher.fit(DASH), Some(Fit::Full));
        assert_eq!(matcher.fit(renamed), None);
        assert_eq!(matcher.fit(dialog), None);
    }

    #[test]
    fn a_loosened_title_fits_fully() {
        let mut matcher = WindowMatch::exact(DASH);
        matcher.title = Pattern::new("Geometry Dash*");
        matcher.priority = MatchPriority::Title;
        let renamed = with(
            "Geometry Dash - Stereo Madness",
            "GLFW30",
            "geometrydash.EXE",
        );
        assert_eq!(matcher.fit(renamed), Some(Fit::Full));
    }

    #[test]
    fn ranking_puts_the_best_fits_first() {
        let matcher = WindowMatch::exact(DASH);
        let windows = [
            with("Settings", "#32770", "GeometryDash.exe"),
            with("Spotify Premium", "Chrome_WidgetWin_0", "Spotify.exe"),
            with("Geometry Dash - menu", "GLFW30", "GeometryDash.exe"),
            DASH,
            with("Another", "#32770", "GeometryDash.exe"),
        ];
        assert_eq!(matcher.rank(windows), [3, 2, 0, 4]);
        assert_eq!(matcher.rank([]), [0_usize; 0]);
    }
}
