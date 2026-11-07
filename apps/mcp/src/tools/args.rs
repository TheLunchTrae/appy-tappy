//! The arguments each tool takes, parsed from the agent's JSON, and the values
//! and tests they turn into.

use appytappy_sources::memory::{Number, ValueTest, ValueType};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

pub(super) fn parse<T: DeserializeOwned>(arguments: Value) -> Result<T, String> {
    serde_json::from_value(arguments).map_err(|error| format!("invalid arguments: {error}"))
}

pub(super) fn value_type(name: &str) -> Result<ValueType, String> {
    ValueType::from_name(name).ok_or_else(|| format!("unknown value type: {name}"))
}

fn number(value: &serde_json::Number) -> Number {
    if let Some(int) = value.as_i64() {
        Number::Int(int.into())
    } else if let Some(int) = value.as_u64() {
        Number::Int(int.into())
    } else {
        Number::Float(value.as_f64().unwrap_or(f64::NAN))
    }
}

pub(super) fn exact(value: Option<serde_json::Number>) -> Result<ValueTest, String> {
    let value = value.ok_or("mode exact needs value")?;
    Ok(ValueTest::Exact(number(&value)))
}

pub(super) fn range(
    min: Option<serde_json::Number>,
    max: Option<serde_json::Number>,
) -> Result<ValueTest, String> {
    match (min, max) {
        (Some(min), Some(max)) => Ok(ValueTest::Range {
            min: number(&min),
            max: number(&max),
        }),
        _ => Err("mode range needs min and max".to_owned()),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NoArgs {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProcessListArgs {
    pub(super) name_contains: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WindowListArgs {
    pub(super) pid: Option<u32>,
    #[serde(default)]
    pub(super) include_hidden: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AttachArgs {
    pub(super) pid: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum FirstMode {
    Exact,
    Range,
    Unknown,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum NextMode {
    Exact,
    Range,
    Changed,
    Unchanged,
    Increased,
    Decreased,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ScanFirstArgs {
    pub(super) value_type: String,
    pub(super) mode: FirstMode,
    pub(super) value: Option<serde_json::Number>,
    pub(super) min: Option<serde_json::Number>,
    pub(super) max: Option<serde_json::Number>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ScanNextArgs {
    pub(super) mode: NextMode,
    pub(super) value: Option<serde_json::Number>,
    pub(super) min: Option<serde_json::Number>,
    pub(super) max: Option<serde_json::Number>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ScanResultsArgs {
    #[serde(default)]
    pub(super) offset: usize,
    pub(super) limit: Option<usize>,
}

/// An address as a number or a hex (`0x…`) or decimal string.
#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum AddressArg {
    Number(u64),
    Text(String),
}

impl AddressArg {
    pub(super) fn parse(&self) -> Result<u64, String> {
        match self {
            Self::Number(value) => Ok(*value),
            Self::Text(text) => {
                parse_unsigned(text).ok_or_else(|| format!("not an address: {text}"))
            },
        }
    }
}

/// A pointer offset as a number or a hex or decimal string, optionally
/// negative.
#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum OffsetArg {
    Number(i64),
    Text(String),
}

impl OffsetArg {
    pub(super) fn parse(&self) -> Result<i64, String> {
        match self {
            Self::Number(value) => Ok(*value),
            Self::Text(text) => {
                let (negative, digits) = text
                    .strip_prefix('-')
                    .map_or((false, text.as_str()), |rest| (true, rest));
                let magnitude = parse_unsigned(digits).and_then(|value| i64::try_from(value).ok());
                magnitude
                    .and_then(|value| {
                        if negative {
                            value.checked_neg()
                        } else {
                            Some(value)
                        }
                    })
                    .ok_or_else(|| format!("not an offset: {text}"))
            },
        }
    }
}

fn parse_unsigned(text: &str) -> Option<u64> {
    let text = text.trim();
    match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Some(digits) => u64::from_str_radix(digits, 16).ok(),
        None => text.parse().ok(),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReadArgs {
    pub(super) address: AddressArg,
    pub(super) value_type: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReadBytesArgs {
    pub(super) address: AddressArg,
    pub(super) length: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FollowPointerArgs {
    pub(super) module: Option<String>,
    pub(super) base: AddressArg,
    #[serde(default)]
    pub(super) offsets: Vec<OffsetArg>,
    pub(super) value_type: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ScreenSampleArgs {
    pub(super) points: Vec<Point>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Point {
    pub(super) x: i32,
    pub(super) y: i32,
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn parses_offsets_and_numbers() {
        let offset = |text: &str| OffsetArg::Text(text.to_owned()).parse();
        assert_eq!(offset("-0x10"), Ok(-16));
        assert_eq!(offset("0X1f"), Ok(31));
        assert_eq!(offset(" 12 "), Ok(12));
        assert!(offset("-x").is_err());
        assert!(offset("0x8000000000000000").is_err());

        assert_eq!(
            number(&serde_json::Number::from(u64::MAX)),
            Number::Int(u64::MAX.into())
        );
        assert_eq!(
            number(&serde_json::Number::from_f64(0.5).unwrap()),
            Number::Float(0.5)
        );
    }
}
