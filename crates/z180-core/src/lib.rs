#![forbid(unsafe_code)]
#![no_std]

extern crate alloc;

mod memory;
mod registers;

pub use memory::{ConfigError, MachineConfig, RegionDef, RegionKind, Variant};
pub use registers::Reg;

use memory::Memory;
use registers::Registers;

pub trait HostBus {
    fn mem_read(&mut self, phys: u32) -> u8;
    fn mem_write(&mut self, phys: u32, value: u8);
    fn io_read(&mut self, port: u16) -> u8;
    fn io_write(&mut self, port: u16, value: u8);
}

pub struct Z180<B: HostBus> {
    registers: Registers,
    memory: Memory,
    bus: B,
    instruction_pc: u16,
    cycle_count: u64,
    halted: bool,
}

impl<B: HostBus> Z180<B> {
    pub fn new(config: MachineConfig, bus: B) -> Result<Self, ConfigError> {
        Ok(Self {
            registers: Registers::default(),
            memory: Memory::new(&config)?,
            bus,
            instruction_pc: 0,
            cycle_count: 0,
            halted: false,
        })
    }

    pub fn reset(&mut self) {
        self.registers = Registers::default();
        self.instruction_pc = 0;
        self.halted = false;
    }

    pub fn step(&mut self) -> u32 {
        if self.halted {
            return self.finish_step(1);
        }

        let pc = self.registers.get(Reg::PC);
        self.instruction_pc = pc;
        let opcode = self.read_logical(pc);
        if !is_opcode_implemented(opcode) {
            return 0;
        }
        self.registers.increment_pc();
        self.registers.increment_r();

        match opcode {
            0x00 => {}
            0x76 => self.halted = true,
            0x40..=0x7f => self.execute_ld_block(opcode),
            _ => return 0,
        }

        // Phase 1 deliberately has no timing model. This non-hardware unit lets
        // the execution skeleton make progress until UM0050 timings land in P4.
        self.finish_step(1)
    }

    pub fn run(&mut self, cycles: u32) -> u32 {
        let mut consumed = 0_u32;
        while consumed < cycles {
            let step_cycles = self.step();
            if step_cycles == 0 {
                break;
            }
            consumed = consumed.saturating_add(step_cycles);
        }
        consumed
    }

    pub fn cycle_count(&self) -> u64 {
        self.cycle_count
    }

    pub fn halted(&self) -> bool {
        self.halted
    }

    pub fn reg(&self, reg: Reg) -> u16 {
        self.registers.get(reg)
    }

    pub fn set_reg(&mut self, reg: Reg, value: u16) {
        self.registers.set(reg, value);
    }

    pub fn instruction_pc(&self) -> u16 {
        self.instruction_pc
    }

    pub fn mem_peek(&self, phys: u32) -> u8 {
        self.memory.peek(phys)
    }

    pub fn mem_poke(&mut self, phys: u32, value: u8) {
        self.memory.poke(&mut self.bus, phys, value);
    }

    fn execute_ld_block(&mut self, opcode: u8) {
        let destination = (opcode >> 3) & 0x07;
        let source = opcode & 0x07;
        let value = if source == 6 {
            let address = self.registers.get(Reg::HL);
            self.read_logical(address)
        } else {
            let Some(value) = self.registers.byte(source) else {
                return;
            };
            value
        };

        if destination == 6 {
            let address = self.registers.get(Reg::HL);
            self.write_logical(address, value);
        } else {
            let _ = self.registers.set_byte(destination, value);
        }
    }

    fn read_logical(&mut self, logical: u16) -> u8 {
        self.memory.read(&mut self.bus, u32::from(logical))
    }

    fn write_logical(&mut self, logical: u16, value: u8) {
        self.memory.write(&mut self.bus, u32::from(logical), value);
    }

    fn finish_step(&mut self, cycles: u32) -> u32 {
        self.cycle_count = self.cycle_count.saturating_add(u64::from(cycles));
        cycles
    }
}

pub const fn is_opcode_implemented(opcode: u8) -> bool {
    opcode == 0x00 || (opcode >= 0x40 && opcode <= 0x7f)
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    #[derive(Default)]
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

    #[test]
    fn implemented_set_is_only_the_phase_one_stub() {
        for opcode in 0_u8..=u8::MAX {
            let expected = opcode == 0x00 || (0x40..=0x7f).contains(&opcode);
            assert_eq!(
                is_opcode_implemented(opcode),
                expected,
                "opcode {opcode:02x}"
            );
        }
    }

    #[test]
    fn unimplemented_opcode_does_not_execute() {
        let mut cpu = machine();
        cpu.mem_poke(0x1234, 0x01);
        cpu.set_reg(Reg::PC, 0x1234);
        cpu.set_reg(Reg::IR, 0x5678);

        assert_eq!(cpu.step(), 0);
        assert_eq!(cpu.instruction_pc(), 0x1234);
        assert_eq!(cpu.reg(Reg::PC), 0x1234);
        assert_eq!(cpu.reg(Reg::IR), 0x5678);
        assert_eq!(cpu.cycle_count(), 0);
        assert_eq!(cpu.run(10), 0);
    }

    #[test]
    fn nop_advances_pc_and_r_without_changing_registers() {
        let mut cpu = machine();
        cpu.mem_poke(0x1234, 0x00);
        cpu.set_reg(Reg::PC, 0x1234);
        cpu.set_reg(Reg::AF, 0x56d7);
        cpu.set_reg(Reg::BC, 0x89ab);
        cpu.set_reg(Reg::IR, 0x34ff);

        assert_eq!(cpu.step(), 1);

        assert_eq!(cpu.instruction_pc(), 0x1234);
        assert_eq!(cpu.reg(Reg::PC), 0x1235);
        assert_eq!(cpu.reg(Reg::AF), 0x56d7);
        assert_eq!(cpu.reg(Reg::BC), 0x89ab);
        assert_eq!(cpu.reg(Reg::IR), 0x3480);
        assert_eq!(cpu.cycle_count(), 1);
        assert!(!cpu.halted());
    }

    #[test]
    fn halt_enters_halted_state_and_leaves_flags_unchanged() {
        let mut cpu = machine();
        cpu.mem_poke(0, 0x76);
        cpu.set_reg(Reg::AF, 0x12a5);

        cpu.step();

        assert!(cpu.halted());
        assert_eq!(cpu.reg(Reg::PC), 1);
        assert_eq!(cpu.reg(Reg::AF), 0x12a5);
    }

    #[test]
    fn ld_register_to_register_preserves_flags() {
        let mut cpu = machine();
        cpu.mem_poke(0, 0x78);
        cpu.set_reg(Reg::AF, 0x11d5);
        cpu.set_reg(Reg::BC, 0x42ee);

        cpu.step();

        assert_eq!(cpu.reg(Reg::AF), 0x42d5);
    }

    #[test]
    fn ld_reads_and_writes_through_hl() {
        let mut cpu = machine();
        cpu.mem_poke(0, 0x46);
        cpu.mem_poke(1, 0x70);
        cpu.mem_poke(0x2000, 0x5a);
        cpu.set_reg(Reg::HL, 0x2000);

        cpu.step();
        assert_eq!(cpu.reg(Reg::BC).to_be_bytes()[0], 0x5a);

        cpu.set_reg(Reg::BC, 0xa5ff);
        cpu.step();
        assert_eq!(cpu.mem_peek(0x2000), 0xa5);
    }

    #[test]
    fn opcode_76_is_halt_not_ld_hl_hl() {
        let mut cpu = machine();
        cpu.mem_poke(0, 0x76);
        cpu.mem_poke(0x2222, 0x7c);
        cpu.set_reg(Reg::HL, 0x2222);

        cpu.step();

        assert!(cpu.halted());
        assert_eq!(cpu.mem_peek(0x2222), 0x7c);
    }

    #[test]
    fn reset_preserves_memory_and_clears_r_and_halt() {
        let mut cpu = machine();
        cpu.mem_poke(0, 0x76);
        cpu.set_reg(Reg::IR, 0xab7f);
        cpu.step();
        cpu.mem_poke(0x4000, 0xcc);

        cpu.reset();

        assert_eq!(cpu.reg(Reg::IR), 0);
        assert_eq!(cpu.reg(Reg::PC), 0);
        assert!(!cpu.halted());
        assert_eq!(cpu.mem_peek(0x4000), 0xcc);
    }
}
