//! Tickets: how the owner thread tells whether a message that arrives late
//! is still about work it has.
//!
//! A capture hands its frames over from its own thread, and the text reader
//! and the window look answer from theirs, so a message can arrive after
//! the owner stopped or replaced the work it is about. Each piece of such
//! work takes a [`Ticket`] from the owner's one counter when it starts, and
//! every message about it carries that ticket. The owner applies a message
//! only while the work it is for still holds the ticket.

use std::io;

use appytappy_sources::screen::CaptureSession;

/// Names one piece of work whose messages reach the owner later: a capture,
/// a recording, or a search. No two pieces of work share one, and a later
/// one's is greater.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Ticket(u64);

impl Ticket {
    /// The ticket's number, which names its work to the UI.
    pub(crate) const fn number(self) -> u64 {
        self.0
    }

    /// The ticket with `number`, where a test stands in for the counter.
    #[cfg(test)]
    pub(crate) const fn numbered(number: u64) -> Self {
        Self(number)
    }
}

/// The owner's counter, which hands out every [`Ticket`].
#[derive(Debug, Default)]
pub(crate) struct Tickets(u64);

impl Tickets {
    /// A ticket no work has had.
    pub(crate) const fn next(&mut self) -> Ticket {
        self.0 = self.0.wrapping_add(1);
        Ticket(self.0)
    }
}

/// A capture that runs, with the ticket its messages carry.
#[derive(Debug)]
pub(crate) struct Running<S> {
    /// The ticket its handler sends with each frame.
    pub(crate) ticket: Ticket,
    /// The capture.
    pub(crate) session: S,
}

impl<S: CaptureSession> Running<S> {
    /// Stops the capture.
    ///
    /// # Errors
    ///
    /// Returns the error the capture's thread ended with.
    pub(crate) fn close(self) -> io::Result<()> {
        self.session.close()
    }
}
