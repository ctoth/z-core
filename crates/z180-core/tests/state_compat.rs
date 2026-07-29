#![cfg(feature = "state")]

use core::convert::Infallible;

use z180_core::{Event, HostBus, MachineConfig, Reg, StateError, TraceEntry, Z180};

const V0_1_0_STATE_V4: &[u8] = include_bytes!("fixtures/v0.1.0/state-v4.bin");

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

fn empty_machine() -> Z180<NullBus> {
    Z180::new(MachineConfig::default(), NullBus).expect("the default configuration must be valid")
}

#[test]
fn v0_1_0_state_v4_loads_resaves_and_resumes_exactly() {
    assert_eq!(V0_1_0_STATE_V4.len(), 66_050);
    assert_eq!(V0_1_0_STATE_V4.first(), Some(&4));

    let mut machine = empty_machine();
    assert_eq!(machine.load_state(V0_1_0_STATE_V4), Ok(()));
    assert_eq!(
        machine.save_state(),
        V0_1_0_STATE_V4,
        "current postcard encoding must preserve the released fixture byte for byte"
    );

    assert_eq!(machine.cycle_count(), 37);
    assert_eq!(machine.reg(Reg::PC), 5);
    assert_eq!(machine.reg(Reg::SP), 0xeffe);
    assert_eq!(machine.reg(Reg::AF), 0x5a34);
    assert_eq!(machine.reg(Reg::BC), 0x5678);
    assert_eq!(machine.reg(Reg::DE), 0x9abc);
    assert_eq!(machine.reg(Reg::HL), 0xdef0);
    assert_eq!(machine.mem_peek(0x1234), 0x5a);
    assert_eq!(machine.pc_watch_hits(), 1);
    assert_eq!(
        machine.drain_events(),
        vec![Event::MemWrite {
            cycle: 12,
            pc: 2,
            phys: 0x1234,
            val: 0x5a,
        }]
    );
    assert_eq!(
        machine.drain_insn_trace(),
        vec![
            TraceEntry {
                cycle: 0,
                pc: 0,
                phys_pc: 0,
                bytes: [0x3e, 0x5a, 0, 0],
                len: 2,
            },
            TraceEntry {
                cycle: 12,
                pc: 2,
                phys_pc: 2,
                bytes: [0x32, 0x34, 0x12, 0],
                len: 3,
            },
        ]
    );

    assert_eq!(machine.step(), 6);
    assert_eq!(machine.cycle_count(), 43);
    assert_eq!(machine.reg(Reg::PC), 6);
}

#[test]
fn undeclared_future_state_version_is_rejected_atomically() {
    let mut future = V0_1_0_STATE_V4.to_vec();
    future[0] = 5;
    let mut machine = empty_machine();
    let original = machine.save_state();

    assert_eq!(
        machine.load_state(&future),
        Err(StateError::UnsupportedVersion(5))
    );
    assert_eq!(machine.save_state(), original);
}
