use alloc::vec;
use alloc::vec::Vec;

use crate::HostBus;

pub(crate) const PAGE_SIZE: u32 = 4096;

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
}

impl Default for MachineConfig {
    fn default() -> Self {
        Self {
            clock_hz: 12_288_000,
            phys_addr_bits: 20,
            unmapped_read: 0xff,
            variant: Variant::Z80180,
            regions: Vec::new(),
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Page {
    Unmapped,
    Ram { offset: usize },
    Rom { offset: usize },
    External,
}

pub(crate) struct Memory {
    pages: Vec<Page>,
    ram: Vec<u8>,
    rom: Vec<u8>,
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
            ram: Vec::new(),
            rom: Vec::new(),
            unmapped_read: config.unmapped_read,
            physical_size,
        };

        for region in &config.regions {
            memory.add_region(region)?;
        }

        Ok(memory)
    }

    fn add_region(&mut self, region: &RegionDef) -> Result<(), ConfigError> {
        if !region.base.is_multiple_of(PAGE_SIZE) || !region.size.is_multiple_of(PAGE_SIZE) {
            return Err(ConfigError::UnalignedRegion {
                base: region.base,
                size: region.size,
            });
        }

        let Some(end) = region.base.checked_add(region.size) else {
            return Err(ConfigError::RegionOutOfRange {
                base: region.base,
                size: region.size,
            });
        };
        if end > self.physical_size {
            return Err(ConfigError::RegionOutOfRange {
                base: region.base,
                size: region.size,
            });
        }

        let first_page = (region.base / PAGE_SIZE) as usize;
        let page_count = (region.size / PAGE_SIZE) as usize;
        if self.pages[first_page..first_page + page_count]
            .iter()
            .any(|page| *page != Page::Unmapped)
        {
            return Err(ConfigError::OverlappingRegion {
                base: region.base,
                size: region.size,
            });
        }

        let page = match &region.kind {
            RegionKind::Ram => {
                let offset = self.ram.len();
                self.ram.resize(offset + region.size as usize, 0);
                Page::Ram { offset }
            }
            RegionKind::Rom(data) => {
                if data.len() != region.size as usize {
                    return Err(ConfigError::RomSizeMismatch {
                        region_size: region.size,
                        data_size: data.len(),
                    });
                }
                let offset = self.rom.len();
                self.rom.extend_from_slice(data);
                Page::Rom { offset }
            }
            RegionKind::External => Page::External,
        };

        for page_index in 0..page_count {
            self.pages[first_page + page_index] = match page {
                Page::Ram { offset } => Page::Ram {
                    offset: offset + page_index * PAGE_SIZE as usize,
                },
                Page::Rom { offset } => Page::Rom {
                    offset: offset + page_index * PAGE_SIZE as usize,
                },
                Page::External => Page::External,
                Page::Unmapped => Page::Unmapped,
            };
        }

        Ok(())
    }

    pub(crate) fn read<B: HostBus>(&mut self, bus: &mut B, phys: u32) -> u8 {
        let Some((page, in_page)) = self.lookup(phys) else {
            return self.unmapped_read;
        };
        match page {
            Page::Ram { offset } => self.ram[offset + in_page],
            Page::Rom { offset } => self.rom[offset + in_page],
            Page::External => bus.mem_read(phys),
            Page::Unmapped => self.unmapped_read,
        }
    }

    pub(crate) fn write<B: HostBus>(&mut self, bus: &mut B, phys: u32, value: u8) {
        let Some((page, in_page)) = self.lookup(phys) else {
            return;
        };
        match page {
            Page::Ram { offset } => self.ram[offset + in_page] = value,
            Page::External => bus.mem_write(phys, value),
            Page::Rom { .. } | Page::Unmapped => {}
        }
    }

    pub(crate) fn peek(&self, phys: u32) -> u8 {
        let Some((page, in_page)) = self.lookup(phys) else {
            return self.unmapped_read;
        };
        match page {
            Page::Ram { offset } => self.ram[offset + in_page],
            Page::Rom { offset } => self.rom[offset + in_page],
            Page::External | Page::Unmapped => self.unmapped_read,
        }
    }

    pub(crate) fn poke<B: HostBus>(&mut self, bus: &mut B, phys: u32, value: u8) {
        self.write(bus, phys, value);
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
