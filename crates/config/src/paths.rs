//! Where AppyTappy keeps its files on this machine.
//!
//! Both programs resolve every path through [`Locations`], so they agree on
//! where each file lives.

use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

/// The folder under the user's local app data that holds AppyTappy's
/// machine-specific files.
const PRODUCT_FOLDER: &str = "AppyTappy";

/// AppyTappy's folders for this user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locations {
    /// Under the local app data folder: what belongs to this machine.
    local: PathBuf,
    /// Under the roaming app data folder: the authoring files, which are
    /// small and may roam with the user's profile.
    roaming: PathBuf,
}

impl Locations {
    /// Resolves the folders from the `LOCALAPPDATA` and `APPDATA`
    /// environment variables.
    ///
    /// # Errors
    ///
    /// Returns [`LocationError`] when a variable is unset or not an
    /// absolute path.
    pub fn from_env() -> Result<Self, LocationError> {
        Self::from_app_data(env::var_os("LOCALAPPDATA"), env::var_os("APPDATA"))
    }

    /// Resolves the folders from the values of `LOCALAPPDATA` and
    /// `APPDATA`.
    ///
    /// # Errors
    ///
    /// Returns [`LocationError`] naming the first value that is `None` or
    /// not an absolute path. A relative path would put the files wherever
    /// the program happened to start.
    pub fn from_app_data(
        local: Option<OsString>,
        roaming: Option<OsString>,
    ) -> Result<Self, LocationError> {
        let product = |value: Option<OsString>, variable| {
            value
                .map(PathBuf::from)
                .filter(|base| base.is_absolute())
                .map(|base| base.join(PRODUCT_FOLDER))
                .ok_or(LocationError(variable))
        };
        Ok(Self {
            local: product(local, "LOCALAPPDATA")?,
            roaming: product(roaming, "APPDATA")?,
        })
    }

    /// The folder for the runtime's log files.
    #[must_use]
    pub fn logs(&self) -> PathBuf {
        self.local.join("logs")
    }

    /// The folder for the runtime's trace files.
    #[must_use]
    pub fn traces(&self) -> PathBuf {
        self.local.join("traces")
    }

    /// The file where the runtime keeps whether automations are enabled, as
    /// the tray last left them.
    #[must_use]
    pub fn enabled(&self) -> PathBuf {
        self.local.join("enabled.txt")
    }

    /// The folder that holds one folder per application, each with its
    /// authoring file and pictures.
    #[must_use]
    pub fn apps(&self) -> PathBuf {
        self.roaming.join("apps")
    }

    /// The authoring file that holds every automation.
    #[must_use]
    pub fn automations(&self) -> PathBuf {
        self.roaming.join("automations.json")
    }

    /// The file that holds AppyTappy's settings.
    #[must_use]
    pub fn settings(&self) -> PathBuf {
        self.roaming.join("settings.json")
    }
}

/// An app data folder is unknown; it names the environment variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocationError(&'static str);

impl fmt::Display for LocationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} is not set to an absolute path", self.0)
    }
}

impl Error for LocationError {}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn each_folder_lives_in_the_product_folder_under_its_app_data() {
        let local = env::temp_dir().join("local");
        let roaming = env::temp_dir().join("roaming");
        let locations = Locations::from_app_data(
            Some(local.clone().into_os_string()),
            Some(roaming.clone().into_os_string()),
        )
        .expect("absolute paths");
        assert_eq!(locations.logs(), local.join("AppyTappy").join("logs"));
        assert_eq!(locations.traces(), local.join("AppyTappy").join("traces"));
        assert_eq!(
            locations.enabled(),
            local.join("AppyTappy").join("enabled.txt")
        );
        assert_eq!(locations.apps(), roaming.join("AppyTappy").join("apps"));
        assert_eq!(
            locations.automations(),
            roaming.join("AppyTappy").join("automations.json")
        );
        assert_eq!(
            locations.settings(),
            roaming.join("AppyTappy").join("settings.json")
        );
    }

    #[test]
    fn a_missing_or_relative_value_is_refused_by_name() {
        let absolute = Some(env::temp_dir().into_os_string());
        let relative = Some(Path::new("relative").into());
        let cases = [
            (None, absolute.clone(), "LOCALAPPDATA"),
            (relative.clone(), absolute.clone(), "LOCALAPPDATA"),
            (absolute.clone(), None, "APPDATA"),
            (absolute, relative, "APPDATA"),
        ];
        for (local, roaming, variable) in cases {
            let error = Locations::from_app_data(local, roaming).expect_err(variable);
            assert_eq!(error, LocationError(variable));
            assert_eq!(
                error.to_string(),
                format!("{variable} is not set to an absolute path")
            );
        }
    }
}
