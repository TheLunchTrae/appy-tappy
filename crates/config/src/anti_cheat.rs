//! The anti-cheat policy: protection classes, the input they allow, and the
//! anti-cheat programs the code recognizes.

use std::fmt;

/// How an app protects itself from other programs.
///
/// Only [`AntiCheatClass::None`] permits input-injection actions: see
/// [`allows_input`].
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
    fn names_each_class() {
        let names = [
            AntiCheatClass::None,
            AntiCheatClass::UserMode,
            AntiCheatClass::KernelLevel,
            AntiCheatClass::Unknown,
        ]
        .map(|class| class.to_string());
        assert_eq!(names, ["none", "user-mode", "kernel-level", "unknown"]);
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
}
