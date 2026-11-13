//! A file the page sends with a command.
//!
//! The file is the command's raw body, and what the page says about it is
//! JSON in the `about` header. JSON arguments would carry the file as a
//! list of numbers, several times its size, which Tauri parses whole before
//! the command sees it.

use serde::de::DeserializeOwned;
use tauri::Runtime;
use tauri::http::HeaderMap;
use tauri::ipc::{CommandArg, CommandItem, InvokeBody, InvokeError};

use crate::sharing::MAX_SHARED_BYTES;

/// The header that holds what the page says about the file, as JSON in
/// ASCII, which is all a header holds: `aboutHeader` in the page's
/// `src/lib/upload.ts` writes it.
const ABOUT: &str = "about";

/// The message for a file that did not arrive as the page sends one, which
/// only a fault in the page causes.
const UNREAD: &str = "AppyTappy Setup could not read the file it was sent.";

/// A file the page sent as a command's only argument, and what the page
/// says about it. The command owns both, so it can hand them to the owner
/// thread.
#[derive(Debug)]
pub(crate) struct Upload<T> {
    /// The file's bytes, [`MAX_SHARED_BYTES`] at most.
    pub(crate) bytes: Vec<u8>,
    /// What the page says about the file.
    pub(crate) about: T,
}

impl<'de, R: Runtime, T: DeserializeOwned> CommandArg<'de, R> for Upload<T> {
    fn from_command(command: CommandItem<'de, R>) -> Result<Self, InvokeError> {
        let (bytes, about) = read(command.message.payload(), command.message.headers())?;
        Ok(Self { bytes, about })
    }
}

/// The file in `body`, and what `headers` say about it.
///
/// A file larger than [`MAX_SHARED_BYTES`] is refused before it is copied
/// for the owner thread. The page refuses one first, with the file's name.
///
/// # Errors
///
/// Returns the message for the user when the file is too large, or when the
/// body is not raw bytes or the header is not JSON for a `T`.
fn read<T: DeserializeOwned>(
    body: &InvokeBody,
    headers: &HeaderMap,
) -> Result<(Vec<u8>, T), String> {
    let InvokeBody::Raw(bytes) = body else {
        return Err(UNREAD.to_owned());
    };
    if bytes.len() > MAX_SHARED_BYTES {
        return Err(format!(
            "The file holds more than {} MB.",
            MAX_SHARED_BYTES / (1024 * 1024)
        ));
    }
    let about = headers
        .get(ABOUT)
        .and_then(|about| about.to_str().ok())
        .and_then(|about| serde_json::from_str(about).ok())
        .ok_or_else(|| UNREAD.to_owned())?;
    Ok((bytes.clone(), about))
}

#[cfg(test)]
mod tests {
    use tauri::http::HeaderValue;

    use super::*;

    fn headers(about: &'static str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(ABOUT, HeaderValue::from_static(about));
        headers
    }

    #[test]
    fn a_file_comes_with_what_the_page_says_about_it() {
        let body = InvokeBody::Raw(vec![80, 75]);
        // As `aboutHeader` writes a name outside ASCII.
        assert_eq!(
            read::<String>(&body, &headers(r#""caf\u00e9 \ud83c\udf89.zip""#)),
            Ok((vec![80, 75], "café 🎉.zip".to_owned()))
        );
        assert_eq!(
            read::<String>(&body, &HeaderMap::new()),
            Err(UNREAD.to_owned()),
            "nothing about it"
        );
        assert_eq!(
            read::<String>(&body, &headers("7")),
            Err(UNREAD.to_owned()),
            "not what the command takes"
        );
        assert_eq!(
            read::<String>(&InvokeBody::Json("PK".into()), &headers(r#""a.zip""#)),
            Err(UNREAD.to_owned()),
            "the file as JSON"
        );
    }

    #[test]
    fn a_file_larger_than_an_import_reads_is_refused_as_it_arrives() {
        let most = InvokeBody::Raw(vec![0; MAX_SHARED_BYTES]);
        assert!(read::<String>(&most, &headers(r#""a.zip""#)).is_ok());
        let over = InvokeBody::Raw(vec![0; MAX_SHARED_BYTES.checked_add(1).expect("fits")]);
        assert_eq!(
            read::<String>(&over, &headers(r#""a.zip""#)),
            Err("The file holds more than 16 MB.".to_owned())
        );
    }
}
