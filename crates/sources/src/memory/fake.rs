//! Process memory for tests: regions at fixed addresses, with holes that fail
//! to read.

use std::io;

use super::{Module, PointerWidth, ProcessMemory, Region};

/// Process memory made of regions at fixed addresses; `holes` are ranges
/// that fail to read, as guard pages do.
pub(super) struct FakeMemory {
    regions: Vec<(u64, Vec<u8>)>,
    pub(super) holes: Vec<(u64, u64)>,
    pub(super) width: PointerWidth,
}

impl FakeMemory {
    pub(super) fn new(regions: Vec<(u64, Vec<u8>)>) -> Self {
        Self {
            regions,
            holes: Vec::new(),
            width: PointerWidth::Bits64,
        }
    }

    pub(super) fn write(&mut self, address: u64, bytes: &[u8]) {
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

    fn image_name(&self) -> io::Result<String> {
        Ok("Fake.exe".to_owned())
    }
}

pub(super) fn f32_region(base: u64, values: &[f32]) -> (u64, Vec<u8>) {
    (base, values.iter().flat_map(|v| v.to_le_bytes()).collect())
}
