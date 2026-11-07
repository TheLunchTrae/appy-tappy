//! The value types a scan reads, the numbers they hold, and the tests a scan
//! applies to them.

use std::cmp::Ordering;

/// How a scan or a read interprets memory. Every type is little-endian.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueType {
    /// A signed 32-bit integer.
    I32,
    /// An unsigned 32-bit integer.
    U32,
    /// A signed 64-bit integer.
    I64,
    /// An unsigned 64-bit integer.
    U64,
    /// A 32-bit float.
    F32,
    /// A 64-bit float.
    F64,
}

impl ValueType {
    /// Every value type.
    pub const ALL: [Self; 6] = [
        Self::I32,
        Self::U32,
        Self::I64,
        Self::U64,
        Self::F32,
        Self::F64,
    ];

    /// The type's name, such as `f32`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::I32 => "i32",
            Self::U32 => "u32",
            Self::I64 => "i64",
            Self::U64 => "u64",
            Self::F32 => "f32",
            Self::F64 => "f64",
        }
    }

    /// The type with this name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }

    /// The value's size in bytes. Scans look only at addresses that are a
    /// multiple of it.
    #[must_use]
    pub const fn size(self) -> usize {
        match self {
            Self::I32 | Self::U32 | Self::F32 => 4,
            Self::I64 | Self::U64 | Self::F64 => 8,
        }
    }

    /// Decodes a value from the first [`ValueType::size`] bytes, or `None`
    /// when there are fewer.
    #[must_use]
    pub fn decode(self, bytes: &[u8]) -> Option<Number> {
        Some(match self {
            Self::I32 => Number::Int(i32::from_le_bytes(take(bytes)?).into()),
            Self::U32 => Number::Int(u32::from_le_bytes(take(bytes)?).into()),
            Self::I64 => Number::Int(i64::from_le_bytes(take(bytes)?).into()),
            Self::U64 => Number::Int(u64::from_le_bytes(take(bytes)?).into()),
            Self::F32 => Number::Float(f32::from_le_bytes(take(bytes)?).into()),
            Self::F64 => Number::Float(f64::from_le_bytes(take(bytes)?)),
        })
    }

    /// The value as this type stores it: a target for an `f32` scan is
    /// rounded to the nearest `f32`, so that an exact test can match it.
    fn stored(self, number: Number) -> Number {
        match (self, number) {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "rounding to the nearest f32 is the conversion an f32 scan target needs"
            )]
            (Self::F32, Number::Float(value)) => Number::Float(f64::from(value as f32)),
            #[expect(
                clippy::cast_precision_loss,
                reason = "rounding to the nearest f32 is the conversion an f32 scan target needs"
            )]
            (Self::F32, Number::Int(value)) => Number::Float(f64::from(value as f32)),
            _ => number,
        }
    }
}

fn take<const N: usize>(bytes: &[u8]) -> Option<[u8; N]> {
    bytes.get(..N)?.try_into().ok()
}

/// A decoded value, or a value to compare against.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Number {
    /// An integer of any value type.
    Int(i128),
    /// A floating-point value.
    Float(f64),
}

impl Number {
    pub(super) fn compare(self, other: Self) -> Option<Ordering> {
        match (self, other) {
            (Self::Int(left), Self::Int(right)) => Some(left.cmp(&right)),
            _ => self.to_f64().partial_cmp(&other.to_f64()),
        }
    }

    fn to_f64(self) -> f64 {
        match self {
            Self::Int(value) => value as f64,
            Self::Float(value) => value,
        }
    }
}

/// A test on a value alone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ValueTest {
    /// The value equals this one.
    Exact(Number),
    /// The value lies in this inclusive range.
    Range {
        /// The smallest value that passes.
        min: Number,
        /// The largest value that passes.
        max: Number,
    },
}

impl ValueTest {
    pub(super) fn passes(self, value_type: ValueType, value: Number) -> bool {
        match self {
            Self::Exact(target) => {
                value.compare(value_type.stored(target)) == Some(Ordering::Equal)
            },
            Self::Range { min, max } => {
                matches!(
                    value.compare(min),
                    Some(Ordering::Greater | Ordering::Equal)
                ) && matches!(value.compare(max), Some(Ordering::Less | Ordering::Equal))
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f32_targets_given_as_integers_round_to_f32() {
        let stored = ValueType::F32.stored(Number::Int(16_777_217));
        assert_eq!(stored, Number::Float(16_777_216.0));
        assert_eq!(
            ValueType::F64.stored(Number::Int(16_777_217)),
            Number::Int(16_777_217)
        );
    }

    #[test]
    fn integers_compare_exactly_beyond_float_precision() {
        let above = Number::Int((1 << 53) + 1);
        let below = Number::Int(1 << 53);
        assert_eq!(above.compare(below), Some(Ordering::Greater));
        assert_eq!(
            Number::Float(1.5).compare(Number::Int(1)),
            Some(Ordering::Greater)
        );
    }

    #[test]
    fn value_types_round_trip_by_name() {
        for kind in ValueType::ALL {
            assert_eq!(ValueType::from_name(kind.name()), Some(kind));
        }
        assert_eq!(ValueType::from_name("f16"), None);
        assert_eq!(ValueType::U64.decode(&[1, 0, 0]), None);
        assert_eq!(
            ValueType::U64.decode(&u64::MAX.to_le_bytes()),
            Some(Number::Int(u64::MAX.into()))
        );
    }
}
