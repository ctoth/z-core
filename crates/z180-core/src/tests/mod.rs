mod access;
mod debug;
mod instructions;
mod interrupts;
mod io_dma;
mod serial;
#[cfg(feature = "state")]
mod state;
mod timers;
mod timing;

extern crate std;

use alloc::vec;

use proptest::prelude::*;

use super::*;

#[derive(Default)]
struct NullBus;

impl HostBus for NullBus {
    type Error = Infallible;

    fn mem_read(&mut self, _phys: u32) -> Result<u8, Self::Error> {
        Ok(0xff)
    }

    fn mem_write(&mut self, _phys: u32, _value: u8) -> Result<(), Self::Error> {
        Ok(())
    }

    fn io_read(&mut self, _port: u16) -> Result<u8, Self::Error> {
        Ok(0xff)
    }

    fn io_write(&mut self, _port: u16, _value: u8) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[derive(Default)]
struct RecordingBus {
    read_value: u8,
    reads: Vec<u16>,
    writes: Vec<(u16, u8)>,
    memory_writes: Vec<(u32, u8)>,
}

impl HostBus for RecordingBus {
    type Error = Infallible;

    fn mem_read(&mut self, _phys: u32) -> Result<u8, Self::Error> {
        Ok(0xff)
    }

    fn mem_write(&mut self, phys: u32, value: u8) -> Result<(), Self::Error> {
        self.memory_writes.push((phys, value));
        Ok(())
    }

    fn io_read(&mut self, port: u16) -> Result<u8, Self::Error> {
        self.reads.push(port);
        Ok(self.read_value)
    }

    fn io_write(&mut self, port: u16, value: u8) -> Result<(), Self::Error> {
        self.writes.push((port, value));
        Ok(())
    }
}

struct FailingBus {
    program: [u8; 5],
    writes: Vec<(u32, u8)>,
}

impl HostBus for FailingBus {
    type Error = &'static str;

    fn mem_read(&mut self, phys: u32) -> Result<u8, Self::Error> {
        if phys == 3 {
            return Err("operand read failed");
        }
        Ok(self.program[phys as usize])
    }

    fn mem_write(&mut self, phys: u32, value: u8) -> Result<(), Self::Error> {
        self.writes.push((phys, value));
        Ok(())
    }

    fn io_read(&mut self, _port: u16) -> Result<u8, Self::Error> {
        Ok(0xff)
    }

    fn io_write(&mut self, _port: u16, _value: u8) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn machine() -> Z180<NullBus> {
    let config = MachineConfig {
        regions: vec![RegionDef {
            base: 0,
            size: 0x1_0000,
            kind: RegionKind::Ram,
        }],
        ..MachineConfig::default()
    };
    Z180::new(config, NullBus).expect("flat RAM configuration must be valid")
}

fn recording_machine(variant: Variant) -> Z180<RecordingBus> {
    let config = MachineConfig {
        variant,
        regions: vec![RegionDef {
            base: 0,
            size: 0x1_0000,
            kind: RegionKind::Ram,
        }],
        ..MachineConfig::default()
    };
    Z180::new(config, RecordingBus::default()).expect("flat recording configuration must be valid")
}

fn mmu_machine() -> Z180<NullBus> {
    let config = MachineConfig {
        regions: vec![RegionDef {
            base: 0,
            size: 0x10_0000,
            kind: RegionKind::Ram,
        }],
        ..MachineConfig::default()
    };
    Z180::new(config, NullBus).expect("1 MiB RAM configuration must be valid")
}
