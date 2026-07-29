use std::{env, fs};

use z180_core::{
    HostBus, IrqLine, MachineConfig, Reg, RegionDef, RegionKind, Variant, WatchKind, Z180,
};

struct NullBus;

impl HostBus for NullBus {
    fn mem_read(&mut self, _phys: u32) -> u8 {
        0xff
    }

    fn mem_write(&mut self, _phys: u32, _value: u8) {}

    fn io_read(&mut self, _port: u16) -> u8 {
        0xff
    }

    fn io_write(&mut self, _port: u16, _value: u8) {}
}

fn main() {
    let output = env::args_os()
        .nth(1)
        .expect("usage: generate_state_v4 <output.bin>");
    let config = MachineConfig {
        variant: Variant::Z8S180,
        regions: vec![RegionDef {
            base: 0,
            size: 0x1_0000,
            kind: RegionKind::Ram,
        }],
        event_capacity: 8,
        ..MachineConfig::default()
    };
    let mut machine = Z180::new(config, NullBus).expect("fixture configuration must be valid");
    for (offset, byte) in [0x3e, 0x5a, 0x32, 0x34, 0x12, 0x00]
        .into_iter()
        .enumerate()
    {
        machine.mem_poke(offset as u32, byte);
    }
    machine.set_reg(Reg::AF, 0x1234);
    machine.set_reg(Reg::BC, 0x5678);
    machine.set_reg(Reg::DE, 0x9abc);
    machine.set_reg(Reg::HL, 0xdef0);
    machine.set_reg(Reg::SP, 0xeffe);
    machine.set_interrupt_mode(2);
    machine.set_irq(IrqLine::Int2, true);
    machine.set_dreq(1, true);
    let _watch = machine.add_mem_watch(0x1234, 1, WatchKind::Write);
    machine.set_io_trace(true);
    machine.set_irq_trace(true);
    machine.set_pc_watch(Some(0));
    machine.set_insn_trace(Some(4));

    assert_ne!(machine.step(), 0);
    assert_ne!(machine.step(), 0);
    assert_eq!(machine.mem_peek(0x1234), 0x5a);

    let state = machine.save_state();
    assert_eq!(state.first(), Some(&4));
    fs::write(output, state).expect("fixture must be written");
}
