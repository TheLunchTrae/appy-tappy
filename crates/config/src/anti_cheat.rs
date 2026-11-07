//! The anti-cheat policy: protection classes and the permit that every path
//! opening an app's process requires.

use std::error::Error;
use std::fmt;

/// How an app protects itself from other programs.
///
/// Only [`AntiCheatClass::None`] permits memory sources and anything else that
/// opens the app's process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AntiCheatClass {
    /// The app has no anti-cheat.
    None,
    /// An anti-cheat runs inside the app or beside it in user mode.
    UserMode,
    /// An anti-cheat runs a kernel driver.
    KernelLevel,
    /// Nobody has established the class yet; treated as protected.
    Unknown,
}

impl fmt::Display for AntiCheatClass {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::None => "none",
            Self::UserMode => "user-mode",
            Self::KernelLevel => "kernel-level",
            Self::Unknown => "unknown",
        })
    }
}

/// A Windows process ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProcessId(u32);

impl ProcessId {
    /// Wraps a raw process ID.
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// The raw process ID.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl fmt::Display for ProcessId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Proof that the anti-cheat policy allows opening one process.
///
/// [`ProcessAccessPermit::issue`] is the only constructor, and it refuses every
/// class but [`AntiCheatClass::None`]. Code that opens a process takes a permit
/// for that process, so opening one always names a class at a visible call
/// site. The type does not prove where the class came from: the caller that
/// classifies the process, such as the MCP server's attach after the user's
/// answer, is what the policy rests on.
#[derive(Debug, PartialEq, Eq)]
pub struct ProcessAccessPermit {
    process: ProcessId,
}

impl ProcessAccessPermit {
    /// Issues a permit for `process` when its app's class is
    /// [`AntiCheatClass::None`].
    ///
    /// # Errors
    ///
    /// Returns [`AccessBlocked`] for every other class.
    pub fn issue(process: ProcessId, class: AntiCheatClass) -> Result<Self, AccessBlocked> {
        match class {
            AntiCheatClass::None => Ok(Self { process }),
            AntiCheatClass::UserMode | AntiCheatClass::KernelLevel | AntiCheatClass::Unknown => {
                Err(AccessBlocked { process, class })
            },
        }
    }

    /// The process this permit allows opening.
    #[must_use]
    pub const fn process(&self) -> ProcessId {
        self.process
    }
}

/// The policy refused to open a process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessBlocked {
    process: ProcessId,
    class: AntiCheatClass,
}

impl AccessBlocked {
    /// The class that blocked access.
    #[must_use]
    pub const fn class(&self) -> AntiCheatClass {
        self.class
    }
}

impl fmt::Display for AccessBlocked {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "process {} has anti-cheat class {}, so AppyTappy does not open it",
            self.process, self.class
        )
    }
}

impl Error for AccessBlocked {}

/// Whether the policy lets AppyTappy send input, such as a key press, for an
/// app of `class`.
///
/// Only [`AntiCheatClass::None`] allows it, because anti-cheat can see input
/// that a program sends and treat it as cheating.
#[must_use]
pub const fn allows_input(class: AntiCheatClass) -> bool {
    matches!(class, AntiCheatClass::None)
}

/// Executable names of anti-cheat services and clients, with the product and
/// its class.
///
/// The list is not exhaustive. A match proves an anti-cheat is present; no
/// match proves nothing, so an unlisted app stays [`AntiCheatClass::Unknown`]
/// until the user says otherwise.
const KNOWN_ANTI_CHEAT: [(&str, &str, AntiCheatClass); 12] = [
    (
        "easyanticheat.exe",
        "Easy Anti-Cheat",
        AntiCheatClass::KernelLevel,
    ),
    (
        "easyanticheat_eos.exe",
        "Easy Anti-Cheat",
        AntiCheatClass::KernelLevel,
    ),
    ("beservice.exe", "BattlEye", AntiCheatClass::KernelLevel),
    ("beservice_x64.exe", "BattlEye", AntiCheatClass::KernelLevel),
    ("vgc.exe", "Riot Vanguard", AntiCheatClass::KernelLevel),
    ("vgtray.exe", "Riot Vanguard", AntiCheatClass::KernelLevel),
    (
        "eaanticheat.gameservice.exe",
        "EA Javelin",
        AntiCheatClass::KernelLevel,
    ),
    (
        "eaanticheat.gameservicelauncher.exe",
        "EA Javelin",
        AntiCheatClass::KernelLevel,
    ),
    (
        "gamemon.exe",
        "nProtect GameGuard",
        AntiCheatClass::KernelLevel,
    ),
    (
        "gamemon64.exe",
        "nProtect GameGuard",
        AntiCheatClass::KernelLevel,
    ),
    ("pnkbstra.exe", "PunkBuster", AntiCheatClass::UserMode),
    ("pnkbstrb.exe", "PunkBuster", AntiCheatClass::UserMode),
];

/// A known anti-cheat product, identified from a process's executable name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownAntiCheat {
    /// The product's name, such as `BattlEye`.
    pub product: &'static str,
    /// The class the product implies for an app it protects.
    pub class: AntiCheatClass,
}

/// Identifies an anti-cheat service or client from its executable name,
/// ignoring ASCII case.
#[must_use]
pub fn known_anti_cheat(executable: &str) -> Option<KnownAntiCheat> {
    KNOWN_ANTI_CHEAT
        .iter()
        .find(|(name, _, _)| name.eq_ignore_ascii_case(executable))
        .map(|&(_, product, class)| KnownAntiCheat { product, class })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issues_a_permit_only_for_class_none() {
        let process = ProcessId::new(42);
        let permit = ProcessAccessPermit::issue(process, AntiCheatClass::None)
            .expect("class none permits access");
        assert_eq!(permit.process(), process);

        for class in [
            AntiCheatClass::UserMode,
            AntiCheatClass::KernelLevel,
            AntiCheatClass::Unknown,
        ] {
            let blocked = ProcessAccessPermit::issue(process, class)
                .expect_err("every class but none blocks access");
            assert_eq!(blocked.class(), class);
        }
    }

    #[test]
    fn only_class_none_allows_input() {
        assert!(allows_input(AntiCheatClass::None));
        for class in [
            AntiCheatClass::UserMode,
            AntiCheatClass::KernelLevel,
            AntiCheatClass::Unknown,
        ] {
            assert!(!allows_input(class), "{class}");
        }
    }

    #[test]
    fn identifies_anti_cheat_executables_ignoring_case() {
        assert_eq!(
            known_anti_cheat("BEService.exe"),
            Some(KnownAntiCheat {
                product: "BattlEye",
                class: AntiCheatClass::KernelLevel,
            })
        );
        assert_eq!(known_anti_cheat("Example.exe"), None);
    }

    #[test]
    fn blocked_access_names_the_process_and_class() {
        let blocked = ProcessAccessPermit::issue(ProcessId::new(7), AntiCheatClass::Unknown)
            .expect_err("unknown blocks access");
        assert_eq!(
            blocked.to_string(),
            "process 7 has anti-cheat class unknown, so AppyTappy does not open it"
        );
    }
}
