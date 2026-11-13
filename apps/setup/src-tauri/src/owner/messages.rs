//! The messages the owner's thread receives: the work commands and overlay
//! windows send it, and what its captures and worker threads tell it.

use std::fmt;
use std::time::Instant;

use appytappy_sources::screen::Image;
use tokio::sync::oneshot;

use crate::overlay::{FrameRead, Purpose};
use crate::recording::{Text, look};
use crate::ticket::Ticket;

use super::capture::CaptureFailure;
use super::watch::Seen;
use super::{Desktop, Host, Owner};

/// Where a request's answer goes.
pub(crate) type Reply<T> = oneshot::Sender<T>;

/// Work for the owner thread, from a command or an overlay window, which it
/// runs with the state and which sends its own answer, as
/// [`ask`](crate::commands) builds it.
pub(crate) struct Run<H: Host>(Work<H>);

/// Work for the owner thread, which answers on its own if it answers.
type Work<H> = Box<dyn FnOnce(&mut Owner<H>) + Send>;

impl<H: Host> Run<H> {
    /// Work that runs `work` on the owner and sends what it returns to
    /// `reply`.
    pub(crate) fn new<T: Send + 'static>(
        reply: Reply<T>,
        work: impl FnOnce(&mut Owner<H>) -> T + Send + 'static,
    ) -> Self {
        Self(Box::new(move |owner| {
            // A dropped command, such as one whose window closed while it
            // waited, needs no answer.
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a dropped command needs no answer"
            )]
            let _ = reply.send(work(owner));
        }))
    }

    /// Work that runs `work` on the owner, for a sender that waits for no
    /// answer.
    pub(crate) fn unanswered(work: impl FnOnce(&mut Owner<H>) + Send + 'static) -> Self {
        Self(Box::new(work))
    }

    /// Runs the work on `owner`.
    pub(super) fn run(self, owner: &mut Owner<H>) {
        (self.0)(owner);
    }
}

// The work is a closure, which cannot be printed, so a `Run` prints only its
// name.
impl<H: Host> fmt::Debug for Run<H> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Run")
    }
}

/// What reaches the owner thread. Its host is the operating system's,
/// except in the tests.
#[derive(Debug)]
pub(crate) enum Message<H: Host = Desktop> {
    /// Work from a command or an overlay window.
    Run(Run<H>),
    /// A capture or a worker thread sent `event` about the work `ticket`
    /// names. The owner drops it unless that work still holds the ticket.
    Sent {
        /// The ticket of the capture, recording, or search it is about.
        ticket: Ticket,
        /// What happened.
        event: Event,
    },
    /// The app is closing.
    Shutdown,
}

/// What a capture or a worker thread tells the owner about its work.
#[derive(Debug)]
pub(crate) enum Event {
    /// The picked window's capture sent a frame.
    Frame {
        /// The window's whole frame.
        image: Image,
        /// When it arrived.
        received: Instant,
    },
    /// The picked window's capture failed, such as when the window closed.
    CaptureFailed(CaptureFailure),
    /// The watch's capture saw a frame, or stopped and why.
    Watch(Result<Seen, String>),
    /// The text reader read reading `index` of a recording.
    Recognized {
        /// The reading's index.
        index: usize,
        /// What it read.
        text: Text,
    },
    /// The frame that arrived at `at` was looked over for a recording.
    Looked {
        /// When the frame it looked over arrived.
        at: Instant,
        /// Each run of text it found, or `None` when the look failed.
        seen: Option<Vec<look::Seen>>,
    },
    /// The text reader read find `index` of a search.
    FoundText {
        /// The find's index.
        index: usize,
        /// What it read.
        text: Text,
    },
    /// An application's overlay, or the Values page's reading of its
    /// values, read them, or stopped and why.
    Overlay {
        /// Who the capture reads for.
        purpose: Purpose,
        /// The application's ID.
        app: String,
        /// What the frame read, or why the capture stopped.
        read: Result<FrameRead, String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_runs_on_the_owner_and_answers() {
        let mut harness = crate::owner::test_support::harness(false);
        let (reply, mut answer) = oneshot::channel();
        let run = Run::new(reply, Owner::screenshot);
        assert!(harness.owner.handle(Message::Run(run)).is_continue());
        assert_eq!(
            answer.try_recv(),
            Ok(Err("No frame has arrived from the window yet.".to_owned()))
        );

        let (sender, told) = std::sync::mpsc::channel();
        let run = Run::unanswered(move |owner| {
            owner.stop();
            sender.send(()).expect("test listens");
        });
        assert_eq!(format!("{run:?}"), "Run");
        assert!(harness.owner.handle(Message::Run(run)).is_continue());
        assert_eq!(told.try_recv(), Ok(()));
    }
}
