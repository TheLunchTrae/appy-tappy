//! The action and reader threads the owner starts, one module each, and the
//! helpers that start and join every thread the owner starts.

pub(crate) mod act;
pub(crate) mod reader;

use std::io;
use std::thread::{self, JoinHandle};

/// Starts a named thread.
pub(crate) fn spawn(
    name: &str,
    work: impl FnOnce() + Send + 'static,
) -> io::Result<JoinHandle<()>> {
    thread::Builder::new().name(name.to_owned()).spawn(work)
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
}
