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

/// AppyTappy's folders on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locations {
    local: PathBuf,
}

impl Locations {
    /// Resolves the folders from the `LOCALAPPDATA` environment variable.
    ///
    /// # Errors
    ///
    /// Returns [`LocationError`] when the variable is unset or not an
    /// absolute path.
    pub fn from_env() -> Result<Self, LocationError> {
        Self::from_local_app_data(env::var_os("LOCALAPPDATA"))
    }

    /// Resolves the folders from the value of `LOCALAPPDATA`.
    ///
    /// # Errors
    ///
    /// Returns [`LocationError`] when `value` is `None` or not an absolute
    /// path. A relative path would put the files wherever the program
    /// happened to start.
    pub fn from_local_app_data(value: Option<OsString>) -> Result<Self, LocationError> {
        let base = PathBuf::from(value.ok_or(LocationError)?);
        if base.is_absolute() {
            Ok(Self {
                local: base.join(PRODUCT_FOLDER),
            })
        } else {
            Err(LocationError)
        }
    }

    /// The folder for the runtime's log files.
    #[must_use]
    pub fn logs(&self) -> PathBuf {
        self.local.join("logs")
    }
}

/// The user's local app data folder is unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocationError;

impl fmt::Display for LocationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("LOCALAPPDATA is not set to an absolute path")
    }
}

impl Error for LocationError {}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn logs_live_in_the_product_folder_under_local_app_data() {
        let base = env::temp_dir();
        let locations = Locations::from_local_app_data(Some(base.clone().into_os_string()))
            .expect("an absolute path");
        assert_eq!(locations.logs(), base.join("AppyTappy").join("logs"));
    }

    #[test]
    fn a_missing_or_relative_value_is_refused() {
        assert_eq!(Locations::from_local_app_data(None), Err(LocationError));
        assert_eq!(
            Locations::from_local_app_data(Some(Path::new("relative").into())),
            Err(LocationError)
        );
        assert_eq!(
            LocationError.to_string(),
            "LOCALAPPDATA is not set to an absolute path"
        );
    }
}
