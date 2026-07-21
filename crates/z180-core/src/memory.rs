use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use crate::HostBus;

pub(crate) const PAGE_SIZE: u32 = 4096;

#[cfg_attr(feature = "state", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Variant {
    Z80180,
    Z8S180,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegionKind {
    Ram,
    Rom(Vec<u8>),
    External,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionDef {
    pub base: u32,
    pub size: u32,
    pub kind: RegionKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MachineConfig {
    pub clock_hz: u32,
    pub phys_addr_bits: u8,
    pub unmapped_read: u8,
    pub variant: Variant,
    pub regions: Vec<RegionDef>,
    pub event_capacity: usize,
}

impl Default for MachineConfig {
    fn default() -> Self {
        Self {
            clock_hz: 12_288_000,
            phys_addr_bits: 20,
            unmapped_read: 0xff,
            variant: Variant::Z80180,
            regions: Vec::new(),
            event_capacity: 4096,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConfigError {
    InvalidPhysicalAddressBits(u8),
    UnalignedRegion { base: u32, size: u32 },
    RegionOutOfRange { base: u32, size: u32 },
    OverlappingRegion { base: u32, size: u32 },
    RomSizeMismatch { region_size: u32, data_size: usize },
    InvalidExtMapTableLength { expected: usize, actual: usize },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPhysicalAddressBits(bits) => {
                write!(
                    formatter,
                    "physical address width {bits} is outside 20..=24"
                )
            }
            Self::UnalignedRegion { base, size } => write!(
                formatter,
                "region base {base:#x} and size {size:#x} must be 4 KiB aligned"
            ),
            Self::RegionOutOfRange { base, size } => {
                write!(
                    formatter,
                    "region base {base:#x} size {size:#x} is out of range"
                )
            }
            Self::OverlappingRegion { base, size } => write!(
                formatter,
                "region base {base:#x} size {size:#x} overlaps an existing region"
            ),
            Self::RomSizeMismatch {
                region_size,
                data_size,
            } => write!(
                formatter,
                "ROM region size {region_size:#x} does not match data size {data_size:#x}"
            ),
            Self::InvalidExtMapTableLength { expected, actual } => write!(
                formatter,
                "external mapper table has {actual} entries; expected {expected}"
            ),
        }
    }
}

impl core::error::Error for ConfigError {}

#[cfg_attr(feature = "state", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Page {
    Unmapped,
    Ram { store: usize, offset: usize },
    Rom { store: usize, offset: usize },
    External,
}

#[cfg_attr(feature = "state", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone)]
pub(crate) struct Memory {
    pages: Vec<Page>,
    ram_stores: Vec<Option<Vec<u8>>>,
    rom_stores: Vec<Option<Vec<u8>>>,
    unmapped_read: u8,
    physical_size: u32,
}

impl Memory {
    pub(crate) fn new(config: &MachineConfig) -> Result<Self, ConfigError> {
        if !(20..=24).contains(&config.phys_addr_bits) {
            return Err(ConfigError::InvalidPhysicalAddressBits(
                config.phys_addr_bits,
            ));
        }

        let physical_size = 1_u32 << config.phys_addr_bits;
        let page_count = (physical_size / PAGE_SIZE) as usize;
        let mut memory = Self {
            pages: vec![Page::Unmapped; page_count],
            ram_stores: Vec::new(),
            rom_stores: Vec::new(),
            unmapped_read: config.unmapped_read,
            physical_size,
        };

        for region in &config.regions {
            memory.map_region(region.base, region.size, region.kind.clone(), false)?;
        }

        Ok(memory)
    }

    pub(crate) fn remap(
        &mut self,
        base: u32,
        size: u32,
        kind: RegionKind,
    ) -> Result<(), ConfigError> {
        self.map_region(base, size, kind, true)
    }

    fn map_region(
        &mut self,
        base: u32,
        size: u32,
        kind: RegionKind,
        replace: bool,
    ) -> Result<(), ConfigError> {
        if !base.is_multiple_of(PAGE_SIZE) || !size.is_multiple_of(PAGE_SIZE) {
            return Err(ConfigError::UnalignedRegion { base, size });
        }

        let Some(end) = base.checked_add(size) else {
            return Err(ConfigError::RegionOutOfRange { base, size });
        };
        if end > self.physical_size {
            return Err(ConfigError::RegionOutOfRange { base, size });
        }
        if let RegionKind::Rom(data) = &kind
            && data.len() != size as usize
        {
            return Err(ConfigError::RomSizeMismatch {
                region_size: size,
                data_size: data.len(),
            });
        }

        let first_page = (base / PAGE_SIZE) as usize;
        let page_count = (size / PAGE_SIZE) as usize;
        if !replace
            && self.pages[first_page..first_page + page_count]
                .iter()
                .any(|page| *page != Page::Unmapped)
        {
            return Err(ConfigError::OverlappingRegion { base, size });
        }
        if size == 0 {
            return Ok(());
        }

        let page = match kind {
            RegionKind::Ram => {
                let store = Self::insert_store(&mut self.ram_stores, vec![0; size as usize]);
                Page::Ram { store, offset: 0 }
            }
            RegionKind::Rom(data) => {
                let store = Self::insert_store(&mut self.rom_stores, data);
                Page::Rom { store, offset: 0 }
            }
            RegionKind::External => Page::External,
        };

        for page_index in 0..page_count {
            self.pages[first_page + page_index] = match page {
                Page::Ram { store, offset } => Page::Ram {
                    store,
                    offset: offset + page_index * PAGE_SIZE as usize,
                },
                Page::Rom { store, offset } => Page::Rom {
                    store,
                    offset: offset + page_index * PAGE_SIZE as usize,
                },
                Page::External => Page::External,
                Page::Unmapped => Page::Unmapped,
            };
        }

        if replace {
            self.reclaim_unreferenced_stores();
        }

        Ok(())
    }

    fn insert_store(stores: &mut Vec<Option<Vec<u8>>>, bytes: Vec<u8>) -> usize {
        if let Some(index) = stores.iter().position(Option::is_none) {
            stores[index] = Some(bytes);
            index
        } else {
            let index = stores.len();
            stores.push(Some(bytes));
            index
        }
    }

    fn reclaim_unreferenced_stores(&mut self) {
        let mut live_ram = vec![false; self.ram_stores.len()];
        let mut live_rom = vec![false; self.rom_stores.len()];
        for page in &self.pages {
            match *page {
                Page::Ram { store, .. } => {
                    if let Some(live) = live_ram.get_mut(store) {
                        *live = true;
                    }
                }
                Page::Rom { store, .. } => {
                    if let Some(live) = live_rom.get_mut(store) {
                        *live = true;
                    }
                }
                Page::External | Page::Unmapped => {}
            }
        }
        for (store, live) in self.ram_stores.iter_mut().zip(live_ram) {
            if !live {
                *store = None;
            }
        }
        for (store, live) in self.rom_stores.iter_mut().zip(live_rom) {
            if !live {
                *store = None;
            }
        }
    }

    pub(crate) fn read<B: HostBus>(&mut self, bus: &mut B, phys: u32) -> u8 {
        let Some((page, in_page)) = self.lookup(phys) else {
            return self.unmapped_read;
        };
        match page {
            Page::Ram { store, offset } => self
                .ram_stores
                .get(store)
                .and_then(Option::as_ref)
                .and_then(|bytes| bytes.get(offset + in_page))
                .copied()
                .unwrap_or(self.unmapped_read),
            Page::Rom { store, offset } => self
                .rom_stores
                .get(store)
                .and_then(Option::as_ref)
                .and_then(|bytes| bytes.get(offset + in_page))
                .copied()
                .unwrap_or(self.unmapped_read),
            Page::External => bus.mem_read(phys),
            Page::Unmapped => self.unmapped_read,
        }
    }

    pub(crate) fn write<B: HostBus>(&mut self, bus: &mut B, phys: u32, value: u8) -> bool {
        let Some((page, in_page)) = self.lookup(phys) else {
            return false;
        };
        match page {
            Page::Ram { store, offset } => {
                if let Some(byte) = self
                    .ram_stores
                    .get_mut(store)
                    .and_then(Option::as_mut)
                    .and_then(|bytes| bytes.get_mut(offset + in_page))
                {
                    *byte = value;
                }
                false
            }
            Page::External => {
                bus.mem_write(phys, value);
                false
            }
            Page::Rom { .. } => true,
            Page::Unmapped => false,
        }
    }

    pub(crate) fn peek(&self, phys: u32) -> u8 {
        let Some((page, in_page)) = self.lookup(phys) else {
            return self.unmapped_read;
        };
        match page {
            Page::Ram { store, offset } => self
                .ram_stores
                .get(store)
                .and_then(Option::as_ref)
                .and_then(|bytes| bytes.get(offset + in_page))
                .copied()
                .unwrap_or(self.unmapped_read),
            Page::Rom { store, offset } => self
                .rom_stores
                .get(store)
                .and_then(Option::as_ref)
                .and_then(|bytes| bytes.get(offset + in_page))
                .copied()
                .unwrap_or(self.unmapped_read),
            Page::External | Page::Unmapped => self.unmapped_read,
        }
    }

    pub(crate) fn poke<B: HostBus>(&mut self, bus: &mut B, phys: u32, value: u8) {
        let _ = self.write(bus, phys, value);
    }

    pub(crate) fn ram_regions(&self) -> Vec<(u32, u32)> {
        let mut regions = Vec::new();
        let mut page_index = 0_usize;
        while page_index < self.pages.len() {
            let Page::Ram { store, offset } = self.pages[page_index] else {
                page_index += 1;
                continue;
            };
            let first_page = page_index;
            page_index += 1;
            while page_index < self.pages.len()
                && self.pages[page_index]
                    == (Page::Ram {
                        store,
                        offset: offset + (page_index - first_page) * PAGE_SIZE as usize,
                    })
            {
                page_index += 1;
            }
            regions.push((
                first_page as u32 * PAGE_SIZE,
                (page_index - first_page) as u32 * PAGE_SIZE,
            ));
        }
        regions
    }

    pub(crate) fn ram_region(&self, base: u32) -> Option<&[u8]> {
        let (store, offset, size) = self.ram_segment(base)?;
        self.ram_stores
            .get(store)?
            .as_ref()?
            .get(offset..offset + size)
    }

    pub(crate) fn ram_region_mut(&mut self, base: u32) -> Option<&mut [u8]> {
        let (store, offset, size) = self.ram_segment(base)?;
        self.ram_stores
            .get_mut(store)?
            .as_mut()?
            .get_mut(offset..offset + size)
    }

    fn ram_segment(&self, base: u32) -> Option<(usize, usize, usize)> {
        if !base.is_multiple_of(PAGE_SIZE) || base >= self.physical_size {
            return None;
        }
        let first_page = (base / PAGE_SIZE) as usize;
        let Page::Ram { store, offset } = self.pages[first_page] else {
            return None;
        };
        if first_page != 0
            && self.pages[first_page - 1]
                == (Page::Ram {
                    store,
                    offset: offset.checked_sub(PAGE_SIZE as usize)?,
                })
        {
            return None;
        }
        let mut page_index = first_page + 1;
        while page_index < self.pages.len()
            && self.pages[page_index]
                == (Page::Ram {
                    store,
                    offset: offset + (page_index - first_page) * PAGE_SIZE as usize,
                })
        {
            page_index += 1;
        }
        Some((
            store,
            offset,
            (page_index - first_page) * PAGE_SIZE as usize,
        ))
    }

    #[cfg(feature = "state")]
    pub(crate) fn is_valid(&self) -> bool {
        if !self.physical_size.is_power_of_two()
            || !(20..=24).contains(&(self.physical_size.trailing_zeros() as u8))
            || self.pages.len() != (self.physical_size / PAGE_SIZE) as usize
        {
            return false;
        }
        self.pages.iter().all(|page| match *page {
            Page::Ram { store, offset } => self
                .ram_stores
                .get(store)
                .and_then(Option::as_ref)
                .is_some_and(|bytes| {
                    offset
                        .checked_add(PAGE_SIZE as usize)
                        .is_some_and(|end| end <= bytes.len())
                }),
            Page::Rom { store, offset } => self
                .rom_stores
                .get(store)
                .and_then(Option::as_ref)
                .is_some_and(|bytes| {
                    offset
                        .checked_add(PAGE_SIZE as usize)
                        .is_some_and(|end| end <= bytes.len())
                }),
            Page::External | Page::Unmapped => true,
        })
    }

    fn lookup(&self, phys: u32) -> Option<(Page, usize)> {
        if phys >= self.physical_size {
            return None;
        }
        let page_index = (phys / PAGE_SIZE) as usize;
        let in_page = (phys % PAGE_SIZE) as usize;
        Some((self.pages[page_index], in_page))
    }
}
