//! Reading another process's memory: the guided value scan and the
//! pointer-chain walker.
//!
//! A scan works like Cheat Engine's: a first scan finds every address whose
//! value passes a test, or snapshots all readable memory when the value is
//! unknown, and each next scan keeps the addresses whose value passes a new
//! test against its current and previous value.

use std::cmp::Ordering;
use std::error::Error;
use std::fmt;
use std::io;

/// One committed, readable region of a process's address space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    /// The region's first address.
    pub base: u64,
    /// The region's length in bytes.
    pub size: u64,
}

/// A module, such as the game's executable or a DLL, loaded in a process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    /// The module's file name, such as `GeometryDash.exe`.
    pub name: String,
    /// The address the module is loaded at.
    pub base: u64,
    /// The module's length in bytes.
    pub size: u64,
}

/// The pointer size of a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerWidth {
    /// A 32-bit process.
    Bits32,
    /// A 64-bit process.
    Bits64,
}

impl PointerWidth {
    /// The pointer size in bits.
    #[must_use]
    pub const fn bits(self) -> u32 {
        match self {
            Self::Bits32 => 32,
            Self::Bits64 => 64,
        }
    }
}

/// Read access to one opened process.
pub trait ProcessMemory {
    /// The committed, readable regions, in address order.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn regions(&self) -> io::Result<Vec<Region>>;

    /// Fills `buffer` with the memory at `address`.
    ///
    /// # Errors
    ///
    /// Fails when any byte of the range cannot be read.
    fn read(&self, address: u64, buffer: &mut [u8]) -> io::Result<()>;

    /// The loaded modules.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn modules(&self) -> io::Result<Vec<Module>>;

    /// The process's pointer size.
    fn pointer_width(&self) -> PointerWidth;
}

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
    fn compare(self, other: Self) -> Option<Ordering> {
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
    fn passes(self, value_type: ValueType, value: Number) -> bool {
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

/// What a first scan keeps.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FirstScan {
    /// Every address whose value passes the test.
    Value(ValueTest),
    /// A snapshot of all readable memory, for a value not known yet.
    Unknown,
}

/// What a next scan keeps, comparing each address's current value with its
/// value at the previous scan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NextScan {
    /// The current value passes the test.
    Value(ValueTest),
    /// The value's bytes changed.
    Changed,
    /// The value's bytes did not change.
    Unchanged,
    /// The value grew.
    Increased,
    /// The value shrank.
    Decreased,
}

impl NextScan {
    fn passes(self, value_type: ValueType, previous: &[u8], current: &[u8]) -> bool {
        let Some(now) = value_type.decode(current) else {
            return false;
        };
        let size = value_type.size();
        let before = || {
            value_type
                .decode(previous)
                .and_then(|then| now.compare(then))
        };
        match self {
            Self::Value(test) => test.passes(value_type, now),
            Self::Changed => previous.get(..size) != current.get(..size),
            Self::Unchanged => previous.get(..size) == current.get(..size),
            Self::Increased => before() == Some(Ordering::Greater),
            Self::Decreased => before() == Some(Ordering::Less),
        }
    }
}

/// The most memory an unknown-value first scan snapshots.
pub const SNAPSHOT_LIMIT: u64 = 2 << 30;

/// The most addresses a scan keeps.
pub const CANDIDATE_LIMIT: usize = 32 << 20;

/// Scans read memory in pieces of at most this many bytes, and skip a piece
/// that fails to read.
const PIECE: u64 = 1 << 20;

/// An address a scan kept, with its value at the last scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candidate {
    /// The address.
    pub address: u64,
    previous: [u8; 8],
}

impl Candidate {
    fn new(address: u64, bytes: &[u8]) -> Self {
        let mut previous = [0; 8];
        for (slot, byte) in previous.iter_mut().zip(bytes) {
            *slot = *byte;
        }
        Self { address, previous }
    }

    /// The value at the last scan.
    #[must_use]
    pub fn previous(&self, value_type: ValueType) -> Option<Number> {
        value_type.decode(&self.previous)
    }
}

#[derive(Debug)]
struct Piece {
    base: u64,
    bytes: Vec<u8>,
}

#[derive(Debug)]
enum State {
    Snapshot(Vec<Piece>),
    Candidates(Vec<Candidate>),
}

/// A guided value scan over one process.
#[derive(Debug)]
pub struct Scan {
    value_type: ValueType,
    state: State,
}

impl Scan {
    /// Runs a first scan.
    ///
    /// # Errors
    ///
    /// Fails when the regions cannot be listed or the scan would exceed
    /// [`SNAPSHOT_LIMIT`] or [`CANDIDATE_LIMIT`].
    pub fn first(
        memory: &impl ProcessMemory,
        value_type: ValueType,
        scan: FirstScan,
    ) -> Result<Self, ScanError> {
        let regions = memory.regions().map_err(ScanError::Regions)?;
        let state = match scan {
            FirstScan::Unknown => State::Snapshot(snapshot(memory, &regions, SNAPSHOT_LIMIT)?),
            FirstScan::Value(test) => {
                State::Candidates(find_values(memory, &regions, value_type, test)?)
            },
        };
        Ok(Self { value_type, state })
    }

    /// Runs a next scan, keeping the addresses that pass and dropping those
    /// that no longer read.
    ///
    /// # Errors
    ///
    /// Fails when the scan would exceed [`CANDIDATE_LIMIT`]; the scan keeps
    /// its previous state.
    pub fn next(&mut self, memory: &impl ProcessMemory, scan: NextScan) -> Result<(), ScanError> {
        let value_type = self.value_type;
        let mut kept = Vec::new();
        match &self.state {
            State::Snapshot(pieces) => {
                for old in pieces {
                    compare_piece(memory, old, value_type, scan, &mut kept)?;
                }
            },
            State::Candidates(candidates) => {
                let mut current = vec![0; value_type.size()];
                for candidate in candidates {
                    if memory.read(candidate.address, &mut current).is_ok()
                        && scan.passes(value_type, &candidate.previous, &current)
                    {
                        kept.push(Candidate::new(candidate.address, &current));
                    }
                }
            },
        }
        self.state = State::Candidates(kept);
        Ok(())
    }

    /// The value type this scan reads.
    #[must_use]
    pub const fn value_type(&self) -> ValueType {
        self.value_type
    }

    /// How many addresses the scan holds: every aligned address of a
    /// snapshot, or the kept candidates.
    #[must_use]
    pub fn count(&self) -> u64 {
        match &self.state {
            State::Snapshot(pieces) => pieces
                .iter()
                .map(|piece| {
                    u64::try_from(piece.values(self.value_type).count()).unwrap_or(u64::MAX)
                })
                .fold(0, u64::saturating_add),
            State::Candidates(candidates) => u64::try_from(candidates.len()).unwrap_or(u64::MAX),
        }
    }

    /// The kept addresses, or `None` while the scan is still an unknown-value
    /// snapshot that a next scan has not narrowed.
    #[must_use]
    pub fn candidates(&self) -> Option<&[Candidate]> {
        match &self.state {
            State::Snapshot(_) => None,
            State::Candidates(candidates) => Some(candidates),
        }
    }
}

impl Piece {
    fn len_u64(&self) -> u64 {
        u64::try_from(self.bytes.len()).unwrap_or(u64::MAX)
    }

    /// Each aligned value's address and bytes.
    fn values(&self, value_type: ValueType) -> impl Iterator<Item = (u64, &[u8])> {
        let size = value_type.size();
        let step = u64::try_from(size).unwrap_or(u64::MAX);
        self.bytes
            .chunks_exact(size)
            .zip(0_u64..)
            .filter_map(move |(bytes, index)| {
                let address = index.checked_mul(step)?.checked_add(self.base)?;
                Some((address, bytes))
            })
    }
}

/// Snapshots every readable piece of `regions`, failing once the snapshot
/// would hold more than `limit` bytes.
fn snapshot(
    memory: &impl ProcessMemory,
    regions: &[Region],
    limit: u64,
) -> Result<Vec<Piece>, ScanError> {
    let mut pieces = Vec::new();
    let mut total: u64 = 0;
    for piece in regions
        .iter()
        .flat_map(|region| read_pieces(memory, *region))
    {
        total = total.saturating_add(piece.len_u64());
        if total > limit {
            return Err(ScanError::SnapshotTooLarge);
        }
        pieces.push(piece);
    }
    Ok(pieces)
}

/// Finds every aligned value in `regions` that passes `test`.
fn find_values(
    memory: &impl ProcessMemory,
    regions: &[Region],
    value_type: ValueType,
    test: ValueTest,
) -> Result<Vec<Candidate>, ScanError> {
    let mut candidates = Vec::new();
    for piece in regions
        .iter()
        .flat_map(|region| read_pieces(memory, *region))
    {
        let passing = piece.values(value_type).filter(|(_, bytes)| {
            value_type
                .decode(bytes)
                .is_some_and(|value| test.passes(value_type, value))
        });
        for (address, bytes) in passing {
            keep(&mut candidates, Candidate::new(address, bytes))?;
        }
    }
    Ok(candidates)
}

/// Rereads a snapshot piece and keeps the values that pass `scan`; a piece
/// that no longer reads keeps nothing.
fn compare_piece(
    memory: &impl ProcessMemory,
    old: &Piece,
    value_type: ValueType,
    scan: NextScan,
    kept: &mut Vec<Candidate>,
) -> Result<(), ScanError> {
    let mut bytes = vec![0; old.bytes.len()];
    if memory.read(old.base, &mut bytes).is_err() {
        return Ok(());
    }
    let new = Piece {
        base: old.base,
        bytes,
    };
    let passing = new
        .values(value_type)
        .zip(old.values(value_type))
        .filter(|((_, current), (_, previous))| scan.passes(value_type, previous, current));
    for ((address, current), _) in passing {
        keep(kept, Candidate::new(address, current))?;
    }
    Ok(())
}

fn keep(candidates: &mut Vec<Candidate>, candidate: Candidate) -> Result<(), ScanError> {
    if candidates.len() >= CANDIDATE_LIMIT {
        return Err(ScanError::TooManyCandidates);
    }
    candidates.push(candidate);
    Ok(())
}

/// Reads a region in pieces of at most [`PIECE`] bytes, skipping pieces that
/// fail to read.
fn read_pieces(memory: &impl ProcessMemory, region: Region) -> impl Iterator<Item = Piece> {
    (0..region.size.div_ceil(PIECE)).filter_map(move |index| {
        let offset = index.checked_mul(PIECE)?;
        let base = region.base.checked_add(offset)?;
        let length = usize::try_from(PIECE.min(region.size.saturating_sub(offset))).ok()?;
        let mut bytes = vec![0; length];
        memory.read(base, &mut bytes).ok()?;
        Some(Piece { base, bytes })
    })
}

/// Why a scan failed.
#[derive(Debug)]
pub enum ScanError {
    /// The process's regions could not be listed.
    Regions(io::Error),
    /// An unknown-value first scan would snapshot more than
    /// [`SNAPSHOT_LIMIT`] bytes.
    SnapshotTooLarge,
    /// The scan would keep more than [`CANDIDATE_LIMIT`] addresses.
    TooManyCandidates,
}

impl fmt::Display for ScanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Regions(_) => formatter.write_str("cannot list the process's memory regions"),
            Self::SnapshotTooLarge => write!(
                formatter,
                "an unknown-value scan would snapshot more than {} MiB; scan for a value or range instead",
                SNAPSHOT_LIMIT >> 20
            ),
            Self::TooManyCandidates => write!(
                formatter,
                "the scan would keep more than {CANDIDATE_LIMIT} addresses; use a narrower test"
            ),
        }
    }
}

impl Error for ScanError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Regions(source) => Some(source),
            Self::SnapshotTooLarge | Self::TooManyCandidates => None,
        }
    }
}

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
    use super::*;

    /// Process memory made of regions at fixed addresses; `holes` are ranges
    /// that fail to read, as guard pages do.
    struct FakeMemory {
        regions: Vec<(u64, Vec<u8>)>,
        holes: Vec<(u64, u64)>,
        width: PointerWidth,
    }

    impl FakeMemory {
        fn new(regions: Vec<(u64, Vec<u8>)>) -> Self {
            Self {
                regions,
                holes: Vec::new(),
                width: PointerWidth::Bits64,
            }
        }

        fn write(&mut self, address: u64, bytes: &[u8]) {
            let (offset, data) = self
                .regions
                .iter_mut()
                .find_map(|(base, data)| {
                    let offset = usize::try_from(address.checked_sub(*base)?).ok()?;
                    (offset < data.len()).then_some((offset, data))
                })
                .expect("write inside a region");
            data[offset..]
                .iter_mut()
                .zip(bytes)
                .for_each(|(slot, byte)| *slot = *byte);
        }
    }

    impl ProcessMemory for FakeMemory {
        fn regions(&self) -> io::Result<Vec<Region>> {
            Ok(self
                .regions
                .iter()
                .map(|(base, data)| Region {
                    base: *base,
                    size: data.len() as u64,
                })
                .collect())
        }

        fn read(&self, address: u64, buffer: &mut [u8]) -> io::Result<()> {
            let end = address
                .checked_add(buffer.len() as u64)
                .ok_or_else(|| io::Error::other("overflow"))?;
            if self
                .holes
                .iter()
                .any(|&(start, stop)| address < stop && start < end)
            {
                return Err(io::Error::other("unreadable"));
            }
            let source = self
                .regions
                .iter()
                .find_map(|(base, data)| {
                    let offset = usize::try_from(address.checked_sub(*base)?).ok()?;
                    data.get(offset..offset.checked_add(buffer.len())?)
                })
                .ok_or_else(|| io::Error::other("unmapped"))?;
            buffer.copy_from_slice(source);
            Ok(())
        }

        fn modules(&self) -> io::Result<Vec<Module>> {
            Ok(Vec::new())
        }

        fn pointer_width(&self) -> PointerWidth {
            self.width
        }
    }

    fn f32_region(base: u64, values: &[f32]) -> (u64, Vec<u8>) {
        (base, values.iter().flat_map(|v| v.to_le_bytes()).collect())
    }

    fn addresses(scan: &Scan) -> Vec<u64> {
        scan.candidates()
            .expect("narrowed scan")
            .iter()
            .map(|candidate| candidate.address)
            .collect()
    }

    #[test]
    fn narrows_an_unknown_value_by_how_it_changes() {
        let mut memory = FakeMemory::new(vec![f32_region(0x1000, &[1.0, 2.0, 3.0, 4.0])]);
        let mut scan = Scan::first(&memory, ValueType::F32, FirstScan::Unknown).unwrap();
        assert_eq!(scan.count(), 4);
        assert_eq!(scan.candidates(), None);

        memory.write(0x1004, &2.5_f32.to_le_bytes());
        memory.write(0x1008, &1.0_f32.to_le_bytes());
        scan.next(&memory, NextScan::Changed).unwrap();
        assert_eq!(addresses(&scan), [0x1004, 0x1008]);

        memory.write(0x1004, &9.0_f32.to_le_bytes());
        memory.write(0x1008, &0.5_f32.to_le_bytes());
        scan.next(&memory, NextScan::Increased).unwrap();
        assert_eq!(addresses(&scan), [0x1004]);
        assert_eq!(
            scan.candidates().unwrap()[0].previous(ValueType::F32),
            Some(Number::Float(9.0))
        );
    }

    #[test]
    fn next_scan_compares_with_the_previous_scan_not_the_first() {
        let mut memory = FakeMemory::new(vec![f32_region(0x1000, &[5.0, 5.0])]);
        let mut scan = Scan::first(&memory, ValueType::F32, FirstScan::Unknown).unwrap();
        memory.write(0x1000, &6.0_f32.to_le_bytes());
        scan.next(&memory, NextScan::Changed).unwrap();
        memory.write(0x1000, &5.5_f32.to_le_bytes());
        scan.next(&memory, NextScan::Decreased).unwrap();
        assert_eq!(addresses(&scan), [0x1000]);
        scan.next(&memory, NextScan::Unchanged).unwrap();
        assert_eq!(addresses(&scan), [0x1000]);
        scan.next(&memory, NextScan::Increased).unwrap();
        assert_eq!(addresses(&scan), Vec::<u64>::new());
    }

    #[test]
    fn exact_and_range_tests_match_by_value_type() {
        let memory = FakeMemory::new(vec![f32_region(0x2000, &[0.1, 50.5, 99.0, 150.0])]);
        let exact = Scan::first(
            &memory,
            ValueType::F32,
            FirstScan::Value(ValueTest::Exact(Number::Float(0.1))),
        )
        .unwrap();
        assert_eq!(addresses(&exact), [0x2000]);

        let range = Scan::first(
            &memory,
            ValueType::F32,
            FirstScan::Value(ValueTest::Range {
                min: Number::Int(50),
                max: Number::Float(99.0),
            }),
        )
        .unwrap();
        assert_eq!(addresses(&range), [0x2004, 0x2008]);
    }

    #[test]
    fn integer_scans_read_aligned_values_of_their_size() {
        let mut bytes = Vec::new();
        bytes.extend(7_i64.to_le_bytes());
        bytes.extend((-7_i64).to_le_bytes());
        bytes.extend(7_i64.to_le_bytes());
        let memory = FakeMemory::new(vec![(0x3000, bytes)]);
        let signed = Scan::first(
            &memory,
            ValueType::I64,
            FirstScan::Value(ValueTest::Exact(Number::Int(-7))),
        )
        .unwrap();
        assert_eq!(addresses(&signed), [0x3008]);

        let halves = Scan::first(
            &memory,
            ValueType::U32,
            FirstScan::Value(ValueTest::Exact(Number::Int(7))),
        )
        .unwrap();
        assert_eq!(addresses(&halves), [0x3000, 0x3010]);
    }

    #[test]
    fn scans_skip_unreadable_pieces_and_drop_unreadable_candidates() {
        assert_eq!(PIECE, 0x10_0000);
        let mut data = vec![0_u8; 0x20_0000];
        data[..4].copy_from_slice(&42_i32.to_le_bytes());
        data[0x10_0000..0x10_0004].copy_from_slice(&42_i32.to_le_bytes());
        let mut memory = FakeMemory::new(vec![(0, data)]);
        memory.holes.push((0x10_0000, 0x10_0001));
        let exact = FirstScan::Value(ValueTest::Exact(Number::Int(42)));

        let mut scan = Scan::first(&memory, ValueType::I32, exact).unwrap();
        assert_eq!(addresses(&scan), [0]);

        memory.holes = vec![(0, 4)];
        scan.next(&memory, NextScan::Unchanged).unwrap();
        assert_eq!(addresses(&scan), Vec::<u64>::new());
    }

    #[test]
    fn unknown_scan_drops_snapshot_pieces_that_stop_reading() {
        let mut memory =
            FakeMemory::new(vec![f32_region(0x1000, &[1.0]), f32_region(0x9000, &[1.0])]);
        let mut scan = Scan::first(&memory, ValueType::F32, FirstScan::Unknown).unwrap();
        memory.holes.push((0x1000, 0x1004));
        scan.next(&memory, NextScan::Unchanged).unwrap();
        assert_eq!(addresses(&scan), [0x9000]);
    }

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
    fn reads_regions_in_pieces_of_at_most_one_mebibyte() {
        let memory = FakeMemory::new(vec![
            (0x10_0000, vec![0; 0x20_0005]),
            (0x80_0000, vec![0; 0x20_0000]),
        ]);
        let pieces = |base, size| -> Vec<(u64, usize)> {
            read_pieces(&memory, Region { base, size })
                .map(|piece| (piece.base, piece.bytes.len()))
                .collect()
        };
        assert_eq!(
            pieces(0x10_0000, 0x20_0005),
            [
                (0x10_0000, 0x10_0000),
                (0x20_0000, 0x10_0000),
                (0x30_0000, 5)
            ]
        );
        assert_eq!(
            pieces(0x80_0000, 0x20_0000),
            [(0x80_0000, 0x10_0000), (0x90_0000, 0x10_0000)]
        );
        assert_eq!(pieces(0x80_0000, 0), []);
    }

    #[test]
    fn snapshot_fails_only_past_its_limit() {
        let memory = FakeMemory::new(vec![
            (0x1000, vec![0; 8]),
            (0x2000, vec![0; 8]),
            (0x3000, vec![0; 8]),
        ]);
        let regions = memory.regions().unwrap();
        let held: usize = snapshot(&memory, &regions[..2], 16)
            .unwrap()
            .iter()
            .map(|piece| piece.bytes.len())
            .sum();
        assert_eq!(held, 16);
        assert!(matches!(
            snapshot(&memory, &regions, 16),
            Err(ScanError::SnapshotTooLarge)
        ));
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
    fn errors_explain_themselves_and_keep_their_source() {
        let regions = ScanError::Regions(io::Error::other("denied"));
        assert_eq!(
            regions.to_string(),
            "cannot list the process's memory regions"
        );
        assert_eq!(regions.source().unwrap().to_string(), "denied");
        assert_eq!(
            ScanError::SnapshotTooLarge.to_string(),
            "an unknown-value scan would snapshot more than 2048 MiB; scan for a value or range instead"
        );
        assert_eq!(
            ScanError::TooManyCandidates.to_string(),
            "the scan would keep more than 33554432 addresses; use a narrower test"
        );
        assert!(ScanError::TooManyCandidates.source().is_none());

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
