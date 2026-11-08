//! The window size frames show, settled.
//!
//! Dragging a window's corner shows it at one size after another, and under
//! scaling nearly every size reads. Acting on each would start a capture
//! again for each, and redraw state pictures for each, many times a second.
//! A new size counts only once frames have shown it, and no other, for
//! [`SETTLE`]. Until then a reader reads nothing at it.

use std::time::{Duration, Instant};

use super::Size;

/// How long frames must show the window at one new size before it counts.
pub const SETTLE: Duration = Duration::from_millis(250);

/// The window size a reader reads at, and the other size frames show, if
/// any, since when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settle {
    at: Size,
    pending: Option<(Size, Instant)>,
}

impl Settle {
    /// Settled at `at`, the size a reader reads at.
    #[must_use]
    pub const fn new(at: Size) -> Self {
        Self { at, pending: None }
    }

    /// The size it settled at last.
    #[must_use]
    pub const fn at(&self) -> Size {
        self.at
    }

    /// The other size frames show, before it settled.
    #[must_use]
    pub fn pending(&self) -> Option<Size> {
        self.pending.map(|(size, _)| size)
    }

    /// Notes that a frame at `now` showed the window at `size`. Back at the
    /// settled size, nothing is pending; at a size other than the pending
    /// one, the wait starts again from `now`.
    pub fn saw(&mut self, size: Size, now: Instant) {
        if size == self.at {
            self.pending = None;
        } else if self.pending() != Some(size) {
            self.pending = Some((size, now));
        }
    }

    /// How long from `now` until the pending size settles, or `None` when
    /// none is pending.
    #[must_use]
    pub fn left(&self, now: Instant) -> Option<Duration> {
        let (_, since) = self.pending?;
        Some(SETTLE.saturating_sub(now.saturating_duration_since(since)))
    }

    /// The pending size, once it has settled by `now`. It is then the
    /// settled size, so it is answered once.
    pub fn settled(&mut self, now: Instant) -> Option<Size> {
        if self.left(now)? > Duration::ZERO {
            return None;
        }
        let (size, _) = self.pending.take()?;
        self.at = size;
        Some(size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Size = Size {
        width: 800,
        height: 600,
    };

    fn size(width: u32) -> Size {
        Size { width, height: 600 }
    }

    fn after(start: Instant, millis: u64) -> Instant {
        start
            .checked_add(Duration::from_millis(millis))
            .expect("in range")
    }

    #[test]
    fn a_new_size_settles_once_frames_showed_it_alone_for_a_while() {
        let start = Instant::now();
        let mut settle = Settle::new(WINDOW);
        assert_eq!(settle.left(start), None, "nothing pending");
        assert_eq!(settle.settled(start), None);
        settle.saw(WINDOW, start);
        assert_eq!(settle.pending(), None, "its own size is no change");
        settle.saw(size(700), start);
        assert_eq!(
            settle.left(after(start, 100)),
            Some(Duration::from_millis(150))
        );
        // The same size again keeps its start.
        settle.saw(size(700), after(start, 200));
        assert_eq!(settle.settled(after(start, 249)), None);
        assert_eq!(settle.left(after(start, 400)), Some(Duration::ZERO));
        assert_eq!(settle.settled(after(start, 250)), Some(size(700)));
        assert_eq!(settle.at(), size(700));
        assert_eq!(settle.settled(after(start, 900)), None, "answered once");
        assert_eq!(settle.pending(), None);
    }

    #[test]
    fn each_other_size_starts_the_wait_again_and_the_settled_size_cancels_it() {
        let start = Instant::now();
        let mut settle = Settle::new(WINDOW);
        settle.saw(size(700), start);
        settle.saw(size(650), after(start, 200));
        assert_eq!(settle.pending(), Some(size(650)));
        assert_eq!(settle.settled(after(start, 300)), None, "650 is newer");
        assert_eq!(settle.settled(after(start, 450)), Some(size(650)));
        settle.saw(WINDOW, after(start, 500));
        assert_eq!(settle.pending(), Some(WINDOW));
        settle.saw(size(650), after(start, 600));
        assert_eq!(settle.pending(), None, "back at 650, where it settled");
        assert_eq!(settle.settled(after(start, 2000)), None);
        let mut back = Settle::new(WINDOW);
        back.saw(size(700), start);
        back.saw(WINDOW, after(start, 100));
        assert_eq!(back.pending(), None, "back where it settled");
        assert_eq!(back.settled(after(start, 1000)), None);
    }
}
