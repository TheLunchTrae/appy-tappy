//! The owner's part in an update: it keeps the downloaded installer until the
//! user installs it, and closes what is open before the installer ends the
//! process, as the app's own end would.

use super::{Host, Owner};

impl<H: Host> Owner<H> {
    /// Saves where each panel is, closes the overlays, and stops the capture,
    /// since the installer ends the process before the app's own end runs.
    pub(super) fn close_for_update(&mut self) {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the installer runs either way, and the process ends the capture with it"
        )]
        let _ = self.shut_down();
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::owner::messages::Request;
    use crate::owner::test_support::{frame_image, harness};
    use crate::updates::Downloaded;

    fn downloaded(version: &str) -> Downloaded {
        Downloaded {
            version: version.to_owned(),
            manifest: json!({ "version": version }),
            installer: vec![1, 2, 3],
        }
    }

    #[test]
    fn keeps_the_newest_download_until_it_is_taken() {
        let mut harness = harness(false);
        assert!(harness.ask(Request::TakeUpdate).is_none());
        harness.ask(|reply| Request::KeepUpdate {
            update: downloaded("0.2.0"),
            reply,
        });
        harness.ask(|reply| Request::KeepUpdate {
            update: downloaded("0.3.0"),
            reply,
        });
        let taken = harness.ask(Request::TakeUpdate).expect("a kept download");
        assert_eq!(taken.version, "0.3.0");
        assert_eq!(taken.installer, [1, 2, 3]);
        assert!(harness.ask(Request::TakeUpdate).is_none(), "taken once");
    }

    #[test]
    fn closes_the_capture_before_the_installer_runs() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        harness.ask(Request::CloseForUpdate);
        assert_eq!(harness.closed.try_recv(), Ok(7));
    }
}
