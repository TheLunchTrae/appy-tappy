//! Actions: the side effects a rule runs when it fires.
//!
//! These are validated types; `wiki/developer/Actions.md` explains the v1 set.

use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

/// The longest app name, in bytes.
const MAX_APP_NAME_LEN: usize = 256;

/// An app as Windows reports it: a media session's app ID, such as
/// `Player.exe`, or a process's executable name.
///
/// A name is 1 to 256 bytes with no control characters. It matches a
/// reported name regardless of ASCII case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppName(Box<str>);

impl AppName {
    /// Validates `name`.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidAppName`] when `name` is empty, too long, or holds a
    /// control character.
    pub fn new(name: &str) -> Result<Self, InvalidAppName> {
        if name.is_empty() || name.len() > MAX_APP_NAME_LEN || name.chars().any(char::is_control) {
            return Err(InvalidAppName);
        }
        Ok(Self(name.into()))
    }

    /// The name as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether `reported`, a name Windows reports, names this app.
    #[must_use]
    pub fn matches(&self, reported: &str) -> bool {
        self.0.eq_ignore_ascii_case(reported)
    }
}

/// An app name was empty, longer than 256 bytes, or held a control character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidAppName;

impl fmt::Display for InvalidAppName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "an app name needs 1 to {MAX_APP_NAME_LEN} bytes and no control characters"
        )
    }
}

impl Error for InvalidAppName {}

/// A volume level from silent (0) to full (1).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct VolumeLevel(f32);

impl VolumeLevel {
    /// The level `percent` of full volume, or `None` above 100.
    #[must_use]
    pub fn from_percent(percent: u8) -> Option<Self> {
        (percent <= 100).then(|| Self(f32::from(percent) / 100.0))
    }

    /// The level `scalar`, as Windows reports it, or `None` outside 0 to 1.
    #[must_use]
    pub fn from_scalar(scalar: f32) -> Option<Self> {
        (0.0..=1.0).contains(&scalar).then_some(Self(scalar))
    }

    /// The level from 0 to 1.
    #[must_use]
    pub const fn scalar(self) -> f32 {
        self.0
    }
}

/// Which media session a media action controls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaTarget {
    /// The session Windows considers current, which its media overlay shows.
    Current,
    /// The session of one app, such as `Player.exe`.
    App(AppName),
}

/// What a media action does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaCommand {
    /// Start or resume playback.
    Play,
    /// Pause playback.
    Pause,
    /// Skip to the next track.
    Next,
    /// Go back to the previous track.
    Previous,
}

/// Whose volume a volume action sets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VolumeTarget {
    /// The default playback device.
    System,
    /// Every audio session of one executable, such as `Player.exe`, on the
    /// default playback device.
    App(AppName),
}

/// A key that a key press presses after its modifiers.
///
/// A [`KeyChord`] holds only keys that [`KeyChord::parse`] read, so its
/// letter is always `A` to `Z`, its digit `0` to `9`, and its function key
/// `F1` to `F24`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A letter key, `A` to `Z`.
    Letter(char),
    /// A digit key on the main keyboard, `0` to `9`.
    Digit(char),
    /// A function key, `F1` to `F24`.
    Function(u8),
    /// The space bar.
    Space,
    /// Enter.
    Enter,
    /// Tab.
    Tab,
    /// Escape.
    Escape,
    /// Backspace.
    Backspace,
    /// Insert.
    Insert,
    /// Delete.
    Delete,
    /// Home.
    Home,
    /// End.
    End,
    /// Page up.
    PageUp,
    /// Page down.
    PageDown,
    /// The up arrow.
    Up,
    /// The down arrow.
    Down,
    /// The left arrow.
    Left,
    /// The right arrow.
    Right,
}

/// The named keys, as a key press spells them.
const NAMED_KEYS: [(&str, Key); 15] = [
    ("Space", Key::Space),
    ("Enter", Key::Enter),
    ("Tab", Key::Tab),
    ("Escape", Key::Escape),
    ("Backspace", Key::Backspace),
    ("Insert", Key::Insert),
    ("Delete", Key::Delete),
    ("Home", Key::Home),
    ("End", Key::End),
    ("PageUp", Key::PageUp),
    ("PageDown", Key::PageDown),
    ("Up", Key::Up),
    ("Down", Key::Down),
    ("Left", Key::Left),
    ("Right", Key::Right),
];

impl Key {
    /// Reads one key's name, ignoring ASCII case.
    fn parse(name: &str) -> Option<Self> {
        let mut chars = name.chars();
        if let (Some(only), None) = (chars.next(), chars.next()) {
            let upper = only.to_ascii_uppercase();
            return match upper {
                'A'..='Z' => Some(Self::Letter(upper)),
                '0'..='9' => Some(Self::Digit(upper)),
                _ => None,
            };
        }
        if let Some(number) = name.strip_prefix(['F', 'f']) {
            // `parse` would also take `+1` or `01`, which are not key names.
            if number.starts_with(|digit: char| ('1'..='9').contains(&digit)) {
                return number
                    .parse()
                    .ok()
                    .filter(|number| (1..=24).contains(number))
                    .map(Self::Function);
            }
        }
        NAMED_KEYS
            .iter()
            .find(|(named, _)| named.eq_ignore_ascii_case(name))
            .map(|&(_, key)| key)
    }
}

impl fmt::Display for Key {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Letter(key) | Self::Digit(key) => write!(formatter, "{key}"),
            Self::Function(number) => write!(formatter, "F{number}"),
            named => formatter.write_str(
                NAMED_KEYS
                    .iter()
                    .find(|(_, key)| key == named)
                    .map_or("", |&(name, _)| name),
            ),
        }
    }
}

/// The keys of one key press: any of Ctrl, Alt, Shift and Win, held while
/// one other key is pressed, such as `Ctrl+Shift+M`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyChord {
    /// Whether each of [`Modifier::ALL`] is held.
    held: [bool; 4],
    key: Key,
}

impl KeyChord {
    /// Reads keys such as `Ctrl+Shift+M` or `F9`, ignoring ASCII case and
    /// spaces around each name.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidKeys`] for a name it does not know, a modifier named
    /// twice, or anything but exactly one key that is not a modifier.
    pub fn parse(text: &str) -> Result<Self, InvalidKeys> {
        let mut held = [false; 4];
        let mut key = None;
        for name in text.split('+').map(str::trim) {
            let modifier = Modifier::ALL
                .iter()
                .position(|modifier| modifier.to_string().eq_ignore_ascii_case(name));
            match (modifier, key) {
                (Some(position), None) => {
                    let held = held.get_mut(position).ok_or(InvalidKeys)?;
                    if *held {
                        return Err(InvalidKeys);
                    }
                    *held = true;
                },
                (None, None) => key = Some(Key::parse(name).ok_or(InvalidKeys)?),
                // The key comes last, after its modifiers.
                (_, Some(_)) => return Err(InvalidKeys),
            }
        }
        Ok(Self {
            held,
            key: key.ok_or(InvalidKeys)?,
        })
    }

    /// The modifiers held, in the order they go down: Ctrl, Alt, Shift and
    /// Win, each only when held.
    #[must_use]
    pub fn modifiers(&self) -> Vec<Modifier> {
        self.held
            .into_iter()
            .zip(Modifier::ALL)
            .filter_map(|(held, modifier)| held.then_some(modifier))
            .collect()
    }

    /// The key pressed while the modifiers are held.
    #[must_use]
    pub const fn key(&self) -> Key {
        self.key
    }
}

impl fmt::Display for KeyChord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for modifier in self.modifiers() {
            write!(formatter, "{modifier}+")?;
        }
        write!(formatter, "{}", self.key)
    }
}

/// A key held while another is pressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modifier {
    /// Ctrl.
    Ctrl,
    /// Alt.
    Alt,
    /// Shift.
    Shift,
    /// The Windows key.
    Win,
}

impl Modifier {
    /// Every modifier, in the order a key press holds them down.
    pub const ALL: [Self; 4] = [Self::Ctrl, Self::Alt, Self::Shift, Self::Win];
}

impl fmt::Display for Modifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Ctrl => "Ctrl",
            Self::Alt => "Alt",
            Self::Shift => "Shift",
            Self::Win => "Win",
        })
    }
}

/// Keys did not read as a key press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidKeys;

impl fmt::Display for InvalidKeys {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            "keys are any of Ctrl, Alt, Shift and Win, then one key such as M, 5, F9, Space or \
             Enter, joined by +, such as Ctrl+Shift+M",
        )
    }
}

impl Error for InvalidKeys {}

/// The longest target an open action or a sound action names, in bytes.
const MAX_TARGET_LEN: usize = 2048;

/// Whether `text` is a full Windows path on a drive: a drive letter, a
/// colon and a backslash or slash, such as `C:\Tools\Timer.exe`.
///
/// A relative path would depend on the runtime's working folder, which the
/// user never sees. A network path, which starts with two backslashes, is
/// refused: reaching one sends the user's sign-in to that server, and a file
/// there can hand a link of any type to its program, so an automation from
/// someone else must not name one.
fn is_full_windows_path(text: &str) -> bool {
    matches!(
        text.as_bytes(),
        [letter, b':', b'\\' | b'/', ..] if letter.is_ascii_alphabetic()
    )
}

/// Whether `text` fits in a target: not empty, at most [`MAX_TARGET_LEN`]
/// bytes, and no control characters.
fn fits_target(text: &str) -> bool {
    !text.is_empty() && text.len() <= MAX_TARGET_LEN && !text.chars().any(char::is_control)
}

/// What an open action opens: a program or a file by its full path, which
/// Windows opens with the program the file type is set to, or a web link.
///
/// A link is `http` or `https` only. Other link types hand their text to
/// whichever program registered them, and some of those have run commands
/// from a crafted link, so an automation from someone else must not reach
/// them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenTarget {
    text: Box<str>,
    /// Whether `text` is a web link rather than a path.
    link: bool,
}

impl OpenTarget {
    /// Validates `target`, ignoring spaces around it.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidOpenTarget`] for anything but a full path or an
    /// `http` or `https` link of 1 to 2048 bytes without control
    /// characters, or a link with a space in it.
    pub fn parse(target: &str) -> Result<Self, InvalidOpenTarget> {
        let target = target.trim();
        if !fits_target(target) {
            return Err(InvalidOpenTarget);
        }
        let link = ["http://", "https://"].into_iter().find_map(|scheme| {
            target
                .get(..scheme.len())
                .filter(|start| start.eq_ignore_ascii_case(scheme))
                .map(|_| target.get(scheme.len()..).unwrap_or_default())
        });
        let valid = match link {
            Some(rest) => !rest.is_empty() && !rest.contains(char::is_whitespace),
            None => is_full_windows_path(target),
        };
        if valid {
            Ok(Self {
                text: target.into(),
                link: link.is_some(),
            })
        } else {
            Err(InvalidOpenTarget)
        }
    }

    /// The target as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The folder that holds the program or file, which a program starts
    /// in, as it does when opened from File Explorer; `None` for a web link.
    /// It is found from the text, by the last backslash or slash, so it reads
    /// the same on every system.
    #[must_use]
    pub fn folder(&self) -> Option<&str> {
        if self.link {
            return None;
        }
        let last = self.text.rfind(['\\', '/'])?;
        // A file at a drive's top, such as C:\Timer.exe, is in C:\, not C:.
        let end = if last == 2 { 3 } else { last };
        self.text.get(..end)
    }
}

/// An open action's target was not a full path or a web link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidOpenTarget;

impl fmt::Display for InvalidOpenTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            "open a program or file by its full path, such as C:\\Tools\\Timer.exe, or a web \
             link that starts with http:// or https://",
        )
    }
}

impl Error for InvalidOpenTarget {}

/// The WAV file a sound action plays, by its full path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundFile(PathBuf);

impl SoundFile {
    /// Validates `path`, ignoring spaces around it.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidSoundFile`] for anything but a full path of 1 to 2048
    /// bytes without control characters that ends in `.wav`, ignoring case.
    pub fn parse(path: &str) -> Result<Self, InvalidSoundFile> {
        let path = path.trim();
        let wav = path
            .len()
            .checked_sub(".wav".len())
            .and_then(|start| path.get(start..))
            .is_some_and(|end| end.eq_ignore_ascii_case(".wav"));
        if fits_target(path) && is_full_windows_path(path) && wav {
            Ok(Self(PathBuf::from(path)))
        } else {
            Err(InvalidSoundFile)
        }
    }

    /// The file's path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.0
    }
}

/// A sound action's file was not a full path to a WAV file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidSoundFile;

impl fmt::Display for InvalidSoundFile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("play a WAV file by its full path, such as C:\\Sounds\\Ding.wav")
    }
}

impl Error for InvalidSoundFile {}

/// A side effect from the runtime's fixed set.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Controls a media session.
    Media {
        /// The session to control.
        target: MediaTarget,
        /// What to do.
        command: MediaCommand,
    },
    /// Sets a volume.
    SetVolume {
        /// Whose volume to set.
        target: VolumeTarget,
        /// The level to set.
        level: VolumeLevel,
    },
    /// Presses keys, which go to whichever window has the keyboard focus.
    PressKeys(KeyChord),
    /// Opens a program, a file, or a web link, as opening it from File
    /// Explorer or the Run box would, with nothing passed to it.
    Open(OpenTarget),
    /// Starts playing a WAV file, without waiting for it to end.
    PlaySound(SoundFile),
}

impl Action {
    /// Whether a rule can undo this action on release: pausing or playing
    /// can be reversed and a volume can be set back, but a skipped track
    /// cannot be unskipped, a pressed key cannot be unpressed, and what was
    /// opened or played has nothing to put back.
    #[must_use]
    pub const fn restorable(&self) -> bool {
        !matches!(
            self,
            Self::Media {
                command: MediaCommand::Next | MediaCommand::Previous,
                ..
            } | Self::PressKeys(_)
                | Self::Open(_)
                | Self::PlaySound(_)
        )
    }

    /// Whether the action sends input as if the user typed it, which
    /// anti-cheat can see, so the policy allows it only for an application
    /// whose anti-cheat class is none.
    #[must_use]
    pub const fn injects_input(&self) -> bool {
        matches!(self, Self::PressKeys(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_names_are_bounded_and_match_ignoring_case() {
        let longest = "a".repeat(MAX_APP_NAME_LEN);
        let too_long = "a".repeat(MAX_APP_NAME_LEN.checked_add(1).expect("small"));
        for valid in [
            "Player.exe",
            "Contoso.MusicPlayer_8wekyb3d8bbwe!App",
            longest.as_str(),
        ] {
            assert_eq!(
                AppName::new(valid).map(|name| name.as_str().to_owned()),
                Ok(valid.to_owned())
            );
        }
        for invalid in ["", "Player\n.exe", "\u{7}", too_long.as_str()] {
            assert_eq!(AppName::new(invalid), Err(InvalidAppName), "{invalid:?}");
        }
        let player = AppName::new("Player.exe").expect("valid");
        assert!(player.matches("player.EXE"));
        assert!(!player.matches("Player"));
        assert!(InvalidAppName.to_string().contains("256"));
    }

    #[test]
    fn volume_levels_run_from_zero_to_one() {
        assert_eq!(
            VolumeLevel::from_percent(0).map(VolumeLevel::scalar),
            Some(0.0)
        );
        assert_eq!(
            VolumeLevel::from_percent(25).map(VolumeLevel::scalar),
            Some(0.25)
        );
        assert_eq!(
            VolumeLevel::from_percent(100).map(VolumeLevel::scalar),
            Some(1.0)
        );
        assert_eq!(VolumeLevel::from_percent(101), None);
        assert_eq!(
            VolumeLevel::from_scalar(0.0).map(VolumeLevel::scalar),
            Some(0.0)
        );
        assert_eq!(
            VolumeLevel::from_scalar(1.0).map(VolumeLevel::scalar),
            Some(1.0)
        );
        for scalar in [-0.01, 1.01, f32::NAN] {
            assert_eq!(VolumeLevel::from_scalar(scalar), None);
        }
    }

    fn chord(text: &str) -> String {
        KeyChord::parse(text).map_or_else(|_invalid| "invalid".to_owned(), |keys| keys.to_string())
    }

    #[test]
    fn keys_read_modifiers_then_one_key_in_any_case() {
        for (text, read) in [
            ("Ctrl+Shift+M", "Ctrl+Shift+M"),
            ("shift + ctrl + m", "Ctrl+Shift+M"),
            ("Win+Alt+5", "Alt+Win+5"),
            ("F9", "F9"),
            ("f1", "F1"),
            ("F24", "F24"),
            ("ctrl+pagedown", "Ctrl+PageDown"),
            ("Space", "Space"),
            ("a", "A"),
        ] {
            assert_eq!(chord(text), read, "{text:?}");
        }
        for text in [
            "",
            "Ctrl",
            "Ctrl+",
            "Ctrl+Ctrl+M",
            "M+Ctrl",
            "M+N",
            "F0",
            "F25",
            "F01",
            "F+1",
            "Ctrl+Shift+Alt+Win+Meta+M",
            "Ä",
            "PrintScreen",
        ] {
            assert_eq!(chord(text), "invalid", "{text:?}");
        }
        let keys = KeyChord::parse("Win+Ctrl+Up").expect("valid");
        assert_eq!(keys.modifiers(), [Modifier::Ctrl, Modifier::Win]);
        assert_eq!(keys.key(), Key::Up);
        assert!(InvalidKeys.to_string().contains("Ctrl+Shift+M"));
    }

    #[test]
    fn only_key_presses_inject_input() {
        let keys = Action::PressKeys(KeyChord::parse("F9").expect("valid"));
        assert!(keys.injects_input());
        assert!(!keys.restorable());
        assert!(
            !Action::Media {
                target: MediaTarget::Current,
                command: MediaCommand::Pause,
            }
            .injects_input()
        );
    }

    #[test]
    fn open_targets_are_full_paths_or_web_links() {
        let rest = MAX_TARGET_LEN
            .checked_sub("https://".len())
            .expect("long enough");
        let longest = format!("https://{}", "a".repeat(rest));
        for (text, read) in [
            (r"C:\Tools\Timer.exe", r"C:\Tools\Timer.exe"),
            ("  d:/notes/today.txt ", "d:/notes/today.txt"),
            ("https://example.com/a?b=c", "https://example.com/a?b=c"),
            ("HTTP://example.com", "HTTP://example.com"),
            (longest.as_str(), longest.as_str()),
        ] {
            assert_eq!(
                OpenTarget::parse(text).map(|target| target.as_str().to_owned()),
                Ok(read.to_owned()),
                "{text:?}"
            );
        }
        let too_long = format!("{longest}a");
        for text in [
            "",
            "   ",
            "Timer.exe",
            r"Tools\Timer.exe",
            "C:Timer.exe",
            "1:/x",
            "https://",
            "https://example.com/a b",
            "ftp://example.com",
            "file:///C:/Tools/Timer.exe",
            "search-ms:query=x",
            r"\\server\share\run.bat",
            r"\\?\UNC\server\share\run.bat",
            r"\\server@SSL\DavWWWRoot\x.url",
            "C:\\Tools\nTimer.exe",
            too_long.as_str(),
        ] {
            assert_eq!(OpenTarget::parse(text), Err(InvalidOpenTarget), "{text:?}");
        }
        assert!(InvalidOpenTarget.to_string().contains("https://"));
    }

    #[test]
    fn a_path_opens_in_its_own_folder_and_a_link_in_none() {
        let folder = |text| {
            OpenTarget::parse(text)
                .expect("valid")
                .folder()
                .map(str::to_owned)
        };
        assert_eq!(
            folder(r"C:\Tools\Timer\Timer.exe"),
            Some(r"C:\Tools\Timer".to_owned())
        );
        assert_eq!(folder("d:/notes/today.txt"), Some("d:/notes".to_owned()));
        assert_eq!(folder(r"C:\Timer.exe"), Some(r"C:\".to_owned()));
        assert_eq!(folder("https://example.com/a/b"), None);
        assert_eq!(folder("HTTP://example.com"), None);
    }

    #[test]
    fn sound_files_are_full_paths_to_wav_files() {
        for (text, read) in [
            (r"C:\Sounds\Ding.wav", r"C:\Sounds\Ding.wav"),
            (" c:/sounds/ding.WAV ", "c:/sounds/ding.WAV"),
        ] {
            assert_eq!(
                SoundFile::parse(text).map(|file| file.path().to_path_buf()),
                Ok(PathBuf::from(read)),
                "{text:?}"
            );
        }
        for text in [
            "",
            ".wav",
            "Ding.wav",
            r"C:\Sounds\Ding.mp3",
            r"C:\Sounds\Ding.wave",
            "https://example.com/ding.wav",
            r"\\server\sounds\ding.wav",
            "C:\\Sounds\u{7}.wav",
            "C:\\\u{e9}.wa",
        ] {
            assert_eq!(SoundFile::parse(text), Err(InvalidSoundFile), "{text:?}");
        }
        assert!(InvalidSoundFile.to_string().contains("WAV"));
    }

    #[test]
    fn opening_and_playing_neither_inject_input_nor_restore() {
        for action in [
            Action::Open(OpenTarget::parse("https://example.com").expect("valid")),
            Action::PlaySound(SoundFile::parse(r"C:\Ding.wav").expect("valid")),
        ] {
            assert!(!action.injects_input(), "{action:?}");
            assert!(!action.restorable(), "{action:?}");
        }
    }

    #[test]
    fn only_skips_cannot_be_restored() {
        let media = |command| Action::Media {
            target: MediaTarget::Current,
            command,
        };
        assert!(media(MediaCommand::Play).restorable());
        assert!(media(MediaCommand::Pause).restorable());
        assert!(!media(MediaCommand::Next).restorable());
        assert!(!media(MediaCommand::Previous).restorable());
        let level = VolumeLevel::from_percent(20).expect("valid");
        assert!(
            Action::SetVolume {
                target: VolumeTarget::System,
                level
            }
            .restorable()
        );
    }
}
