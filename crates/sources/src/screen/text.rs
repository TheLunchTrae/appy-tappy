//! The text reader: a model that reads a line of text in any font.
//!
//! The number reader knows the shapes a reference learned, and nothing of
//! a shape it did not. The text reader guesses at those. The setup app
//! reads recorded readings with it, so the user only corrects them, and
//! the runtime asks it what an unlearned character is for a reference that
//! guesses. It is [`ocrs`](https://github.com/robertknight/ocrs), a neural
//! text reader in pure Rust, with its recognition model alone: a line is
//! cut to its text here ([`Line`]), so nothing has to find the text first.
//!
//! A read takes tens of milliseconds, and the model takes memory while it
//! is loaded, so whoever owns a [`TextReader`] runs it on a thread of its
//! own and loads it only when there is a line to read.

mod line;

use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

use ocrs::{ImageSource, OcrEngine, OcrEngineParams};
use rten::Model;
use rten_imageproc::{Point, Rect, RotatedRect};

pub use line::Line;

/// The text reader's model file, in the `models` folder beside the
/// programs.
pub const MODEL_FILE: &str = "text-recognition.onnx";

/// Where the model file is for the program at `program`: in the `models`
/// folder beside it, where the installer puts it.
#[must_use]
pub fn model_path(program: &Path) -> PathBuf {
    program
        .parent()
        .unwrap_or(program)
        .join("models")
        .join(MODEL_FILE)
}

/// What an operation of the text reader failed with.
type Cause = Box<dyn Error + Send + Sync>;

/// The text reader could not load, or could not read a line.
#[derive(Debug)]
pub enum TextError {
    /// The model file could not be loaded.
    Model {
        /// The file.
        path: PathBuf,
        /// Why not.
        source: Cause,
    },
    /// The model loaded, and the reader could not start with it.
    Start(Cause),
    /// A line could not be read.
    Read(Cause),
}

impl fmt::Display for TextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Model { path, .. } => write!(
                formatter,
                "the text reader's model, {}, could not be loaded",
                path.display()
            ),
            Self::Start(_) => formatter.write_str("the text reader could not start"),
            Self::Read(_) => formatter.write_str("the text reader could not read the line"),
        }
    }
}

impl Error for TextError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Model { source, .. } | Self::Start(source) | Self::Read(source) => {
                Some(source.as_ref())
            },
        }
    }
}

/// The text reader with its model loaded.
pub struct TextReader(OcrEngine);

impl fmt::Debug for TextReader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TextReader")
    }
}

impl TextReader {
    /// The text reader with the model in the file `model`.
    ///
    /// # Errors
    ///
    /// Returns [`TextError::Model`] when the file is missing or is no
    /// model, and [`TextError::Start`] when the reader cannot take it.
    pub fn load(model: &Path) -> Result<Self, TextError> {
        let loaded = Model::load_file(model).map_err(|source| TextError::Model {
            path: model.to_owned(),
            source: source.into(),
        })?;
        OcrEngine::new(OcrEngineParams {
            recognition_model: Some(loaded),
            ..OcrEngineParams::default()
        })
        .map(Self)
        .map_err(|source| TextError::Start(source.into()))
    }

    /// The text `line` shows, as the model reads it, without the space
    /// around it.
    ///
    /// # Errors
    ///
    /// Returns [`TextError::Read`] when the model cannot take the line.
    pub fn read(&self, line: &Line) -> Result<String, TextError> {
        self.recognize(line).map_err(TextError::Read)
    }

    fn recognize(&self, line: &Line) -> Result<String, Cause> {
        let source = ImageSource::from_bytes(&line.pixels, (line.width, line.height))?;
        let input = self.0.prepare_input(source)?;
        // The whole line: it is already cut to its text.
        let whole = Rect::new(
            Point::from_yx(0.0, 0.0),
            Point::from_yx(line.height as f32, line.width as f32),
        );
        let lines = self
            .0
            .recognize_text(&input, &[vec![RotatedRect::from_rect(whole)]])?;
        Ok(lines
            .into_iter()
            .flatten()
            .map(|text| text.to_string().trim().to_owned())
            .collect::<Vec<_>>()
            .join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_model_is_beside_the_program() {
        let program = Path::new("install").join("appytappy-setup.exe");
        assert_eq!(
            model_path(&program),
            Path::new("install").join("models").join(MODEL_FILE)
        );
    }
}
