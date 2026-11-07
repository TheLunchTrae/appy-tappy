//! The guided value scan: a first scan over the process's readable memory and
//! next scans that narrow its candidates.

use std::cmp::Ordering;
use std::error::Error;
use std::fmt;
use std::io;
use std::ops::Range;

use super::{Number, ProcessMemory, Region, ValueTest, ValueType};

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
                for window in read_windows(candidates, value_type.size()) {
                    let window = candidates.get(window).unwrap_or_default();
                    reread_window(memory, window, value_type, scan, &mut kept);
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
                    let values = piece.bytes.len().checked_div(self.value_type.size());
                    values.map_or(0, |values| u64::try_from(values).unwrap_or(u64::MAX))
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

/// Splits sorted candidates into runs that one read of at most [`PIECE`]
/// bytes covers, so a next scan reads memory in a few large reads rather than
/// one read per candidate.
fn read_windows(candidates: &[Candidate], size: usize) -> Vec<Range<usize>> {
    let size = u64::try_from(size).unwrap_or(u64::MAX);
    let mut windows = Vec::new();
    let mut start = 0;
    let mut base = None;
    for (index, candidate) in candidates.iter().enumerate() {
        let span = base.and_then(|base| candidate.address.checked_add(size)?.checked_sub(base));
        if span.is_none_or(|span| span > PIECE) {
            if index > start {
                windows.push(start..index);
            }
            start = index;
            base = Some(candidate.address);
        }
    }
    if start < candidates.len() {
        windows.push(start..candidates.len());
    }
    windows
}

/// Rereads one window of candidates and keeps those that pass `scan`. When
/// the window does not read as a whole, as when it spans an unreadable page,
/// each candidate is read alone.
fn reread_window(
    memory: &impl ProcessMemory,
    window: &[Candidate],
    value_type: ValueType,
    scan: NextScan,
    kept: &mut Vec<Candidate>,
) {
    let (Some(first), Some(last)) = (window.first(), window.last()) else {
        return;
    };
    let size = value_type.size();
    let length = last
        .address
        .checked_sub(first.address)
        .and_then(|span| usize::try_from(span).ok())
        .and_then(|span| span.checked_add(size));
    let mut bytes = vec![0; length.unwrap_or(0)];
    let whole = length.is_some() && memory.read(first.address, &mut bytes).is_ok();
    let mut alone = vec![0; size];
    for candidate in window {
        let current = if whole {
            candidate
                .address
                .checked_sub(first.address)
                .and_then(|offset| usize::try_from(offset).ok())
                .and_then(|offset| bytes.get(offset..offset.checked_add(size)?))
        } else {
            memory
                .read(candidate.address, &mut alone)
                .is_ok()
                .then_some(alone.as_slice())
        };
        if let Some(current) = current
            && scan.passes(value_type, &candidate.previous, current)
        {
            kept.push(Candidate::new(candidate.address, current));
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::fake::{FakeMemory, f32_region};

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
    fn groups_candidates_into_reads_of_at_most_one_piece() {
        let at = |addresses: &[u64]| -> Vec<Candidate> {
            addresses
                .iter()
                .map(|&address| Candidate::new(address, &[0; 4]))
                .collect()
        };
        assert_eq!(read_windows(&[], 4), []);
        assert_eq!(
            read_windows(&at(&[0x1000, 0x1004, 0x10_0ffc, 0x10_1000]), 4),
            [0..3, 3..4]
        );
        assert_eq!(
            read_windows(&at(&[0x1000]), 4),
            [Range { start: 0, end: 1 }]
        );
    }

    #[test]
    fn next_scan_reads_candidates_around_an_unreadable_page() {
        let mut memory = FakeMemory::new(vec![(0x1000, vec![0; 0x30])]);
        for address in [0x1000, 0x1010, 0x1020] {
            memory.write(address, &5_i32.to_le_bytes());
        }
        let five = FirstScan::Value(ValueTest::Exact(Number::Int(5)));
        let mut scan = Scan::first(&memory, ValueType::I32, five).unwrap();
        memory.holes.push((0x1010, 0x1014));
        memory.write(0x1020, &6_i32.to_le_bytes());
        scan.next(&memory, NextScan::Unchanged).unwrap();
        assert_eq!(addresses(&scan), [0x1000]);
    }

    #[test]
    fn scan_errors_explain_themselves_and_keep_their_source() {
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
    }
}
