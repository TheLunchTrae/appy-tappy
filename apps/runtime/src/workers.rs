//! The action and reader threads the owner starts, one module each, and the
//! helpers that start and join every thread the owner starts.

pub(crate) mod act;
pub(crate) mod reader;

use std::io;
use std::panic::{self, AssertUnwindSafe};
use std::sync::mpsc::Sender;
use std::thread::{self, JoinHandle};

/// Starts a named thread.
pub(crate) fn spawn(
    name: &str,
    work: impl FnOnce() + Send + 'static,
) -> io::Result<JoinHandle<()>> {
    thread::Builder::new().name(name.to_owned()).spawn(work)
}

/// Runs `work`, then sends `ended` to `events` however `work` ends, so the
/// owner hears of a thread that panicked as soon as it does. The panic still
/// ends the thread, so its join says why.
pub(crate) fn tell_end<E>(work: impl FnOnce(), events: &Sender<E>, ended: E) {
    // Nothing reads what `work` held after a panic, so its state cannot be
    // seen half changed.
    let ran = panic::catch_unwind(AssertUnwindSafe(work));
    #[expect(
        clippy::let_underscore_must_use,
        reason = "a failed send means the owner already stopped, which is what this tells it"
    )]
    let _ = events.send(ended);
    if let Err(payload) = ran {
        panic::resume_unwind(payload);
    }
}

/// Waits for a thread and turns its panic into an error.
pub(crate) fn join(thread: JoinHandle<()>) -> io::Result<()> {
    let name = thread.thread().name().unwrap_or("a").to_owned();
    thread.join().map_err(|payload| {
        io::Error::other(format!(
            "the {name} thread panicked: {}",
            appytappy_platform::panic_reason(payload.as_ref())
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_thread_that_panics_is_an_error_that_names_it() {
        join(spawn("calm", || {}).expect("spawn")).expect("no panic");
        let panicked = spawn("example", || panic!("broke")).expect("spawn");
        let error = join(panicked).expect_err("it panicked");
        assert_eq!(error.to_string(), "the example thread panicked: broke");
    }

    #[test]
    fn a_thread_that_panics_still_tells_the_owner_it_ended() {
        let (events, heard) = std::sync::mpsc::channel();
        let panicked = spawn("example", move || {
            tell_end(|| panic!("broke"), &events, "ended");
        })
        .expect("spawn");
        let error = join(panicked).expect_err("it panicked");
        assert_eq!(error.to_string(), "the example thread panicked: broke");
        assert_eq!(heard.try_recv(), Ok("ended"));
    }
}
