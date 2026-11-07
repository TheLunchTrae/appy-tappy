//! The pointer-chain walker: follows a base address through a list of offsets
//! to the value's address.

use std::error::Error;
use std::fmt;
use std::io;

use super::{PointerWidth, ProcessMemory};

/// Reads one pointer at `address`.
///
/// # Errors
///
/// Returns the read's error.
pub fn read_pointer(memory: &impl ProcessMemory, address: u64) -> io::Result<u64> {
    let mut bytes = [0; 8];
    match memory.pointer_width() {
        PointerWidth::Bits32 => {
            let low = bytes
                .get_mut(..4)
                .ok_or_else(|| io::Error::other("pointer buffer too small"))?;
            memory.read(address, low)?;
        },
        PointerWidth::Bits64 => memory.read(address, &mut bytes)?,
    }
    Ok(u64::from_le_bytes(bytes))
}

/// One step of a pointer chain: the pointer read at `read_at`, plus the
/// step's offset, gives `address`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointerStep {
    /// Where the pointer was read.
    pub read_at: u64,
    /// The pointer's value.
    pub pointer: u64,
    /// The pointer plus the step's offset.
    pub address: u64,
}

/// Follows a pointer chain from `start`.
///
/// Each offset reads a pointer at the current address and adds the offset to
/// it. The last step's address is the chain's target, or `start` itself when
/// there are no offsets.
///
/// # Errors
///
/// Fails at the first pointer that is null, cannot be read, or overflows.
pub fn follow_pointer_chain(
    memory: &impl ProcessMemory,
    start: u64,
    offsets: &[i64],
) -> Result<Vec<PointerStep>, PointerError> {
    let mut steps = Vec::with_capacity(offsets.len());
    let mut current = start;
    for &offset in offsets {
        let pointer = read_pointer(memory, current).map_err(|source| PointerError::Read {
            read_at: current,
            source,
        })?;
        if pointer == 0 {
            return Err(PointerError::Null { read_at: current });
        }
        let address = pointer
            .checked_add_signed(offset)
            .ok_or(PointerError::Overflow { pointer, offset })?;
        steps.push(PointerStep {
            read_at: current,
            pointer,
            address,
        });
        current = address;
    }
    Ok(steps)
}

/// Why a pointer chain broke.
#[derive(Debug)]
pub enum PointerError {
    /// A pointer could not be read.
    Read {
        /// Where the read failed.
        read_at: u64,
        /// The read's error.
        source: io::Error,
    },
    /// A pointer was null.
    Null {
        /// Where the null pointer was read.
        read_at: u64,
    },
    /// Adding an offset to a pointer left the address space.
    Overflow {
        /// The pointer.
        pointer: u64,
        /// The offset.
        offset: i64,
    },
}

impl fmt::Display for PointerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { read_at, .. } => {
                write!(formatter, "cannot read a pointer at {read_at:#x}")
            },
            Self::Null { read_at } => write!(formatter, "the pointer at {read_at:#x} is null"),
            Self::Overflow { pointer, offset } => {
                write!(
                    formatter,
                    "pointer {pointer:#x} plus offset {offset} overflows"
                )
            },
        }
    }
}

impl Error for PointerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::Null { .. } | Self::Overflow { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::memory::fake::FakeMemory;

    use super::*;

    #[test]
    fn follows_a_pointer_chain_and_reports_each_step() {
        let mut data = vec![0_u8; 0x40];
        data[0x00..0x08].copy_from_slice(&0x1010_u64.to_le_bytes());
        data[0x18..0x20].copy_from_slice(&0x1030_u64.to_le_bytes());
        let memory = FakeMemory::new(vec![(0x1000, data)]);
        let steps = follow_pointer_chain(&memory, 0x1000, &[0x8, 0x4]).unwrap();
        assert_eq!(
            steps,
            [
                PointerStep {
                    read_at: 0x1000,
                    pointer: 0x1010,
                    address: 0x1018,
                },
                PointerStep {
                    read_at: 0x1018,
                    pointer: 0x1030,
                    address: 0x1034,
                },
            ]
        );
        assert_eq!(follow_pointer_chain(&memory, 0x1000, &[]).unwrap(), []);
    }

    #[test]
    fn pointer_chain_stops_at_null_unreadable_and_overflowing_pointers() {
        let mut data = vec![0_u8; 0x20];
        data[0x08..0x10].copy_from_slice(&u64::MAX.to_le_bytes());
        let memory = FakeMemory::new(vec![(0x1000, data)]);
        assert!(matches!(
            follow_pointer_chain(&memory, 0x1000, &[0]),
            Err(PointerError::Null { read_at: 0x1000 })
        ));
        assert!(matches!(
            follow_pointer_chain(&memory, 0x1008, &[1]),
            Err(PointerError::Overflow {
                pointer: u64::MAX,
                offset: 1
            })
        ));
        assert!(matches!(
            follow_pointer_chain(&memory, 0x5000, &[0]),
            Err(PointerError::Read {
                read_at: 0x5000,
                ..
            })
        ));
        assert_eq!(
            follow_pointer_chain(&memory, 0x1008, &[-1]).unwrap()[0].address,
            0xffff_ffff_ffff_fffe
        );
    }

    #[test]
    fn reads_four_byte_pointers_from_32_bit_processes() {
        let mut memory = FakeMemory::new(vec![(
            0x1000,
            vec![0x10, 0x20, 0, 0, 0xff, 0xff, 0xff, 0xff],
        )]);
        memory.width = PointerWidth::Bits32;
        assert_eq!(read_pointer(&memory, 0x1000).unwrap(), 0x2010);
    }

    #[test]
    fn pointer_errors_explain_themselves_and_keep_their_source() {
        let read = PointerError::Read {
            read_at: 0x10,
            source: io::Error::other("unreadable"),
        };
        assert_eq!(read.to_string(), "cannot read a pointer at 0x10");
        assert_eq!(read.source().unwrap().to_string(), "unreadable");
        let null = PointerError::Null { read_at: 0x20 };
        assert_eq!(null.to_string(), "the pointer at 0x20 is null");
        assert!(null.source().is_none());
        let overflow = PointerError::Overflow {
            pointer: 0xff,
            offset: -1,
        };
        assert_eq!(
            overflow.to_string(),
            "pointer 0xff plus offset -1 overflows"
        );
    }
}
