//! How the tools word their results and errors for the agent.

use std::io;

use appytappy_sources::memory::{Module, Number, Scan};
use serde_json::{Value, json};

pub(super) fn os_error(operation: &'static str) -> impl FnOnce(io::Error) -> String {
    move |error| format!("cannot {operation}: {error}")
}

pub(super) fn not_attached() -> String {
    "no process is attached; call process_attach first".to_owned()
}

pub(super) fn no_scan() -> String {
    "no scan is running; call memory_scan_first first".to_owned()
}

pub(super) fn hex(value: u64) -> String {
    format!("{value:#x}")
}

/// Names the module an address lies in, as `module+0xoffset`, so the agent
/// can see which candidates are static and can start a pointer chain.
pub(super) fn module_offset(modules: &[Module], address: u64) -> Option<String> {
    modules.iter().find_map(|module| {
        let offset = address.checked_sub(module.base)?;
        (offset < module.size).then(|| format!("{}+{offset:#x}", module.name))
    })
}

pub(super) fn number_json(value: Number) -> Value {
    match value {
        Number::Int(int) => i64::try_from(int)
            .map(Value::from)
            .or_else(|_| u64::try_from(int).map(Value::from))
            .unwrap_or_else(|_| Value::from(int.to_string())),
        Number::Float(float) => Value::from(float),
    }
}

pub(super) fn scan_summary(scan: &Scan) -> Value {
    json!({
        "value_type": scan.value_type().name(),
        "candidates": scan.count(),
        "narrowed": scan.candidates().is_some(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_offset_names_only_addresses_inside_a_module() {
        let modules = [
            Module {
                name: "Example.exe".to_owned(),
                base: 0x1000,
                size: 0x100,
            },
            Module {
                name: "ntdll.dll".to_owned(),
                base: 0x8000,
                size: 0x10,
            },
        ];
        let named =
            [0xfff, 0x1000, 0x10ff, 0x1100, 0x800f].map(|address| module_offset(&modules, address));
        assert_eq!(
            named,
            [
                None,
                Some("Example.exe+0x0".to_owned()),
                Some("Example.exe+0xff".to_owned()),
                None,
                Some("ntdll.dll+0xf".to_owned()),
            ]
        );
    }

    #[test]
    fn formats_numbers() {
        assert_eq!(number_json(Number::Int(-5)), json!(-5));
        assert_eq!(number_json(Number::Int(u64::MAX.into())), json!(u64::MAX));
        assert_eq!(
            number_json(Number::Int(i128::MAX)),
            json!(i128::MAX.to_string())
        );
    }
}
