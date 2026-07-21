#![forbid(unsafe_code)]
#![no_std]

extern crate alloc;

use alloc::vec::Vec;

mod memory;
mod optable;
mod registers;

pub use memory::{ConfigError, MachineConfig, RegionDef, RegionKind, Variant};
pub use registers::Reg;

use memory::Memory;
use optable::{HALT_IDLE_CYCLES, SECOND_OPCODE_TRAP_CYCLES, THIRD_OPCODE_TRAP_CYCLES};
use registers::Registers;

const FLAG_S: u8 = 0x80;
const FLAG_Z: u8 = 0x40;
const FLAG_Y: u8 = 0x20;
const FLAG_H: u8 = 0x10;
const FLAG_X: u8 = 0x08;
const FLAG_PV: u8 = 0x04;
const FLAG_N: u8 = 0x02;
const FLAG_C: u8 = 0x01;
const FLAG_XY: u8 = FLAG_X | FLAG_Y;

pub trait HostBus {
    fn mem_read(&mut self, phys: u32) -> u8;
    fn mem_write(&mut self, phys: u32, value: u8);
    fn io_read(&mut self, port: u16) -> u8;
    fn io_write(&mut self, port: u16, value: u8);
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Trap {
        cycle: u64,
        pc: u16,
        opcode: [u8; 3],
        len: u8,
    },
}

pub struct Z180<B: HostBus> {
    registers: Registers,
    memory: Memory,
    bus: B,
    instruction_pc: u16,
    cycle_count: u64,
    variant: Variant,
    timing_branch_taken: bool,
    timing_repeat_iterations: u16,
    halted: bool,
    sleeping: bool,
    itc: u8,
    iff1: bool,
    iff2: bool,
    ei_shadow: bool,
    interrupt_mode: u8,
    events: Vec<Event>,
}

impl<B: HostBus> Z180<B> {
    pub fn new(config: MachineConfig, bus: B) -> Result<Self, ConfigError> {
        Ok(Self {
            registers: Registers::default(),
            memory: Memory::new(&config)?,
            bus,
            instruction_pc: 0,
            cycle_count: 0,
            variant: config.variant,
            timing_branch_taken: false,
            timing_repeat_iterations: 0,
            halted: false,
            sleeping: false,
            itc: 0x01,
            iff1: false,
            iff2: false,
            ei_shadow: false,
            interrupt_mode: 0,
            events: Vec::new(),
        })
    }

    pub fn reset(&mut self) {
        self.registers = Registers::default();
        self.instruction_pc = 0;
        self.timing_branch_taken = false;
        self.timing_repeat_iterations = 0;
        self.halted = false;
        self.sleeping = false;
        self.itc = 0x01;
        self.iff1 = false;
        self.iff2 = false;
        self.ei_shadow = false;
        self.interrupt_mode = 0;
        self.events.clear();
    }

    pub fn step(&mut self) -> u32 {
        if let Some(cycles) = self.interrupt_check_point() {
            return self.finish_step(cycles);
        }

        if self.halted {
            return self.finish_step(u32::from(HALT_IDLE_CYCLES));
        }
        if self.sleeping {
            return 0;
        }

        let pc = self.registers.get(Reg::PC);
        self.instruction_pc = pc;
        let first_opcode = self.read_logical(pc);
        let (opcode, descriptor, m1_fetches, is_indexed_bit) = match first_opcode {
            0xcb => {
                let opcode = self.read_logical(pc.wrapping_add(1));
                (opcode, Self::CB_OPCODES[usize::from(opcode)], 2, false)
            }
            0xdd => {
                let opcode = self.read_logical(pc.wrapping_add(1));
                if opcode == 0xcb {
                    let opcode = self.read_logical(pc.wrapping_add(3));
                    (opcode, Self::DDCB_OPCODES[usize::from(opcode)], 2, true)
                } else {
                    (opcode, Self::DD_OPCODES[usize::from(opcode)], 2, false)
                }
            }
            0xed => {
                let opcode = self.read_logical(pc.wrapping_add(1));
                (opcode, Self::ED_OPCODES[usize::from(opcode)], 2, false)
            }
            0xfd => {
                let opcode = self.read_logical(pc.wrapping_add(1));
                if opcode == 0xcb {
                    let opcode = self.read_logical(pc.wrapping_add(3));
                    (opcode, Self::FDCB_OPCODES[usize::from(opcode)], 2, true)
                } else {
                    (opcode, Self::FD_OPCODES[usize::from(opcode)], 2, false)
                }
            }
            _ => (
                first_opcode,
                Self::MAIN_OPCODES[usize::from(first_opcode)],
                1,
                false,
            ),
        };
        let Some(handler) = descriptor.handler else {
            if is_indexed_bit {
                self.take_trap([first_opcode, 0xcb, opcode], 3, pc.wrapping_add(4), true, 3);
            } else if matches!(first_opcode, 0xcb | 0xdd | 0xed | 0xfd) {
                self.take_trap([first_opcode, opcode, 0], 2, pc.wrapping_add(2), false, 2);
            } else {
                self.take_trap([first_opcode, 0, 0], 1, pc.wrapping_add(1), false, 1);
            }
            let cycles = if is_indexed_bit {
                THIRD_OPCODE_TRAP_CYCLES
            } else {
                SECOND_OPCODE_TRAP_CYCLES
            };
            return self.finish_step(u32::from(cycles));
        };
        debug_assert!(!descriptor.mnemonic.is_empty());
        debug_assert!(descriptor.length != 0);
        for _ in 0..descriptor.length {
            self.registers.increment_pc();
        }
        for _ in 0..m1_fetches {
            self.registers.increment_r();
        }

        // P2.2 owns the EI shadow state. P2.3 will sample this state at the
        // interrupt-check point; consuming it before dispatch lets a second EI
        // establish a fresh one-instruction shadow.
        self.ei_shadow = false;

        self.timing_branch_taken = false;
        self.timing_repeat_iterations = 0;
        handler(self, opcode);

        let repeat_completed = self.registers.get(Reg::PC) != self.instruction_pc;
        let cycles = descriptor
            .cycles
            .expect("implemented opcode is missing UM0050 timing")
            .resolve(
                self.variant,
                self.timing_branch_taken,
                self.timing_repeat_iterations,
                repeat_completed,
            );
        self.finish_step(cycles)
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

    pub fn sleeping(&self) -> bool {
        self.sleeping
    }

    pub fn itc(&self) -> u8 {
        self.itc
    }

    pub fn drain_events(&mut self) -> Vec<Event> {
        core::mem::take(&mut self.events)
    }

    pub fn iff1(&self) -> bool {
        self.iff1
    }

    pub fn set_iff1(&mut self, enabled: bool) {
        self.iff1 = enabled;
    }

    pub fn iff2(&self) -> bool {
        self.iff2
    }

    pub fn set_iff2(&mut self, enabled: bool) {
        self.iff2 = enabled;
    }

    pub fn interrupt_mode(&self) -> u8 {
        self.interrupt_mode
    }

    pub fn set_interrupt_mode(&mut self, mode: u8) {
        self.interrupt_mode = mode;
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

    pub fn is_instruction_implemented(opcodes: &[u8]) -> bool {
        match opcodes {
            [opcode] => Self::MAIN_OPCODES[usize::from(*opcode)].handler.is_some(),
            [0xcb, opcode] => Self::CB_OPCODES[usize::from(*opcode)].handler.is_some(),
            [0xdd, opcode] => Self::DD_OPCODES[usize::from(*opcode)].handler.is_some(),
            [0xed, opcode] => Self::ED_OPCODES[usize::from(*opcode)].handler.is_some(),
            [0xfd, opcode] => Self::FD_OPCODES[usize::from(*opcode)].handler.is_some(),
            [0xdd, 0xcb, opcode] => Self::DDCB_OPCODES[usize::from(*opcode)].handler.is_some(),
            [0xfd, 0xcb, opcode] => Self::FDCB_OPCODES[usize::from(*opcode)].handler.is_some(),
            _ => false,
        }
    }

    fn interrupt_check_point(&mut self) -> Option<u32> {
        // Phase 2 establishes the pre-fetch service boundary only. Phase 5
        // owns interrupt pins, prioritized sources, acknowledge behavior, and
        // HALT wake-up, so no source can fire here yet.
        None
    }

    fn take_trap(&mut self, opcode: [u8; 3], len: u8, stacked_pc: u16, ufo: bool, m1_fetches: u8) {
        self.itc = (self.itc & 0x07) | 0x80 | if ufo { 0x40 } else { 0 };
        for _ in 0..m1_fetches {
            self.registers.increment_r();
        }
        self.ei_shadow = false;
        self.push_word(stacked_pc);
        self.registers.set(Reg::PC, 0);
        self.events.push(Event::Trap {
            cycle: self.cycle_count,
            pc: self.instruction_pc,
            opcode,
            len,
        });
    }

    fn accumulator(&self) -> u8 {
        self.registers.get(Reg::AF).to_be_bytes()[0]
    }

    fn flags(&self) -> u8 {
        self.registers.get(Reg::AF).to_be_bytes()[1]
    }

    fn set_accumulator(&mut self, value: u8) {
        self.registers
            .set(Reg::AF, u16::from_be_bytes([value, self.flags()]));
    }

    fn set_flags(&mut self, value: u8) {
        self.registers
            .set(Reg::AF, u16::from_be_bytes([self.accumulator(), value]));
    }

    fn set_accumulator_and_flags(&mut self, accumulator: u8, flags: u8) {
        self.registers
            .set(Reg::AF, u16::from_be_bytes([accumulator, flags]));
    }

    fn read_reg8(&mut self, code: u8) -> u8 {
        if code & 0x07 == 6 {
            self.read_logical(self.registers.get(Reg::HL))
        } else {
            self.registers.byte(code).unwrap_or(0)
        }
    }

    fn write_reg8(&mut self, code: u8, value: u8) {
        if code & 0x07 == 6 {
            self.write_logical(self.registers.get(Reg::HL), value);
        } else {
            let _ = self.registers.set_byte(code, value);
        }
    }

    fn reg16(&self, code: u8) -> u16 {
        match code & 0x03 {
            0 => self.registers.get(Reg::BC),
            1 => self.registers.get(Reg::DE),
            2 => self.registers.get(Reg::HL),
            _ => self.registers.get(Reg::SP),
        }
    }

    fn set_reg16(&mut self, code: u8, value: u16) {
        let reg = match code & 0x03 {
            0 => Reg::BC,
            1 => Reg::DE,
            2 => Reg::HL,
            _ => Reg::SP,
        };
        self.registers.set(reg, value);
    }

    fn stack_reg16(&self, code: u8) -> u16 {
        match code & 0x03 {
            0 => self.registers.get(Reg::BC),
            1 => self.registers.get(Reg::DE),
            2 => self.registers.get(Reg::HL),
            _ => self.registers.get(Reg::AF),
        }
    }

    fn set_stack_reg16(&mut self, code: u8, value: u16) {
        let reg = match code & 0x03 {
            0 => Reg::BC,
            1 => Reg::DE,
            2 => Reg::HL,
            _ => Reg::AF,
        };
        self.registers.set(reg, value);
    }

    fn immediate8(&mut self) -> u8 {
        self.read_logical(self.instruction_pc.wrapping_add(1))
    }

    fn immediate16(&mut self) -> u16 {
        self.read_word(self.instruction_pc.wrapping_add(1))
    }

    fn read_word(&mut self, address: u16) -> u16 {
        let low = self.read_logical(address);
        let high = self.read_logical(address.wrapping_add(1));
        u16::from_le_bytes([low, high])
    }

    fn write_word(&mut self, address: u16, value: u16) {
        let [low, high] = value.to_le_bytes();
        self.write_logical(address, low);
        self.write_logical(address.wrapping_add(1), high);
    }

    fn push_word(&mut self, value: u16) {
        let new_sp = self.registers.get(Reg::SP).wrapping_sub(2);
        self.write_word(new_sp, value);
        self.registers.set(Reg::SP, new_sp);
    }

    fn pop_word(&mut self) -> u16 {
        let sp = self.registers.get(Reg::SP);
        let value = self.read_word(sp);
        self.registers.set(Reg::SP, sp.wrapping_add(2));
        value
    }

    fn condition(&self, code: u8) -> bool {
        let flags = self.flags();
        match code & 0x07 {
            0 => flags & FLAG_Z == 0,
            1 => flags & FLAG_Z != 0,
            2 => flags & FLAG_C == 0,
            3 => flags & FLAG_C != 0,
            4 => flags & FLAG_PV == 0,
            5 => flags & FLAG_PV != 0,
            6 => flags & FLAG_S == 0,
            _ => flags & FLAG_S != 0,
        }
    }

    fn relative_target(&self, displacement: u8) -> u16 {
        let signed = i16::from(displacement as i8);
        self.registers.get(Reg::PC).wrapping_add(signed as u16)
    }

    const fn sign_zero_xy(value: u8) -> u8 {
        let mut flags = value & (FLAG_S | FLAG_XY);
        if value == 0 {
            flags |= FLAG_Z;
        }
        flags
    }

    const fn parity(value: u8) -> bool {
        value.count_ones() & 1 == 0
    }

    const fn parity_flag(value: u8) -> u8 {
        if Self::parity(value) { FLAG_PV } else { 0 }
    }

    fn add8(&mut self, value: u8, with_carry: bool) {
        let accumulator = self.accumulator();
        let carry = u8::from(with_carry && self.flags() & FLAG_C != 0);
        let sum = u16::from(accumulator) + u16::from(value) + u16::from(carry);
        let result = sum as u8;
        let mut flags = Self::sign_zero_xy(result);
        if (accumulator & 0x0f) + (value & 0x0f) + carry > 0x0f {
            flags |= FLAG_H;
        }
        if (!(accumulator ^ value) & (accumulator ^ result) & 0x80) != 0 {
            flags |= FLAG_PV;
        }
        if sum > 0xff {
            flags |= FLAG_C;
        }
        self.set_accumulator_and_flags(result, flags);
    }

    fn sub8(&mut self, value: u8, with_carry: bool, compare_only: bool) {
        let accumulator = self.accumulator();
        let carry = u8::from(with_carry && self.flags() & FLAG_C != 0);
        let result = accumulator.wrapping_sub(value).wrapping_sub(carry);
        let mut flags = Self::sign_zero_xy(result) | FLAG_N;
        if (accumulator & 0x0f) < (value & 0x0f) + carry {
            flags |= FLAG_H;
        }
        if ((accumulator ^ value) & (accumulator ^ result) & 0x80) != 0 {
            flags |= FLAG_PV;
        }
        if u16::from(accumulator) < u16::from(value) + u16::from(carry) {
            flags |= FLAG_C;
        }
        if compare_only {
            self.set_flags(flags);
        } else {
            self.set_accumulator_and_flags(result, flags);
        }
    }

    fn execute_alu(&mut self, operation: u8, value: u8) {
        match operation & 0x07 {
            0 => self.add8(value, false),
            1 => self.add8(value, true),
            2 => self.sub8(value, false, false),
            3 => self.sub8(value, true, false),
            4 => {
                let result = self.accumulator() & value;
                let flags = Self::sign_zero_xy(result) | Self::parity_flag(result) | FLAG_H;
                self.set_accumulator_and_flags(result, flags);
            }
            5 => {
                let result = self.accumulator() ^ value;
                let flags = Self::sign_zero_xy(result) | Self::parity_flag(result);
                self.set_accumulator_and_flags(result, flags);
            }
            6 => {
                let result = self.accumulator() | value;
                let flags = Self::sign_zero_xy(result) | Self::parity_flag(result);
                self.set_accumulator_and_flags(result, flags);
            }
            _ => self.sub8(value, false, true),
        }
    }

    pub(crate) fn execute_nop(&mut self, _opcode: u8) {}

    pub(crate) fn execute_halt(&mut self, _opcode: u8) {
        self.halted = true;
    }

    pub(crate) fn execute_ld_reg16_immediate(&mut self, opcode: u8) {
        let value = self.immediate16();
        self.set_reg16(opcode >> 4, value);
    }

    pub(crate) fn execute_ld_indirect_a(&mut self, opcode: u8) {
        let address = if opcode & 0x10 == 0 {
            self.registers.get(Reg::BC)
        } else {
            self.registers.get(Reg::DE)
        };
        self.write_logical(address, self.accumulator());
    }

    pub(crate) fn execute_ld_a_indirect(&mut self, opcode: u8) {
        let address = if opcode & 0x10 == 0 {
            self.registers.get(Reg::BC)
        } else {
            self.registers.get(Reg::DE)
        };
        let value = self.read_logical(address);
        self.set_accumulator(value);
    }

    pub(crate) fn execute_inc_reg16(&mut self, opcode: u8) {
        let code = opcode >> 4;
        self.set_reg16(code, self.reg16(code).wrapping_add(1));
    }

    pub(crate) fn execute_dec_reg16(&mut self, opcode: u8) {
        let code = opcode >> 4;
        self.set_reg16(code, self.reg16(code).wrapping_sub(1));
    }

    pub(crate) fn execute_inc_reg8(&mut self, opcode: u8) {
        let code = (opcode >> 3) & 0x07;
        let value = self.read_reg8(code);
        let result = value.wrapping_add(1);
        let mut flags = Self::sign_zero_xy(result) | (self.flags() & FLAG_C);
        if value & 0x0f == 0x0f {
            flags |= FLAG_H;
        }
        if value == 0x7f {
            flags |= FLAG_PV;
        }
        self.write_reg8(code, result);
        self.set_flags(flags);
    }

    pub(crate) fn execute_dec_reg8(&mut self, opcode: u8) {
        let code = (opcode >> 3) & 0x07;
        let value = self.read_reg8(code);
        let result = value.wrapping_sub(1);
        let mut flags = Self::sign_zero_xy(result) | (self.flags() & FLAG_C) | FLAG_N;
        if value & 0x0f == 0 {
            flags |= FLAG_H;
        }
        if value == 0x80 {
            flags |= FLAG_PV;
        }
        self.write_reg8(code, result);
        self.set_flags(flags);
    }

    pub(crate) fn execute_ld_reg8_immediate(&mut self, opcode: u8) {
        let value = self.immediate8();
        self.write_reg8((opcode >> 3) & 0x07, value);
    }

    pub(crate) fn execute_add_hl(&mut self, opcode: u8) {
        let hl = self.registers.get(Reg::HL);
        let value = self.reg16(opcode >> 4);
        let sum = u32::from(hl) + u32::from(value);
        let result = sum as u16;
        let mut flags = self.flags() & (FLAG_S | FLAG_Z | FLAG_PV);
        flags |= result.to_be_bytes()[0] & FLAG_XY;
        if ((hl ^ value ^ result) & 0x1000) != 0 {
            flags |= FLAG_H;
        }
        if sum > 0xffff {
            flags |= FLAG_C;
        }
        self.registers.set(Reg::HL, result);
        self.set_flags(flags);
    }

    pub(crate) fn execute_accumulator_rotate(&mut self, opcode: u8) {
        let accumulator = self.accumulator();
        let old_carry = u8::from(self.flags() & FLAG_C != 0);
        let (result, carry) = match opcode {
            0x07 => (accumulator.rotate_left(1), accumulator >> 7),
            0x0f => (accumulator.rotate_right(1), accumulator & 1),
            0x17 => ((accumulator << 1) | old_carry, accumulator >> 7),
            _ => ((accumulator >> 1) | (old_carry << 7), accumulator & 1),
        };
        let flags =
            (self.flags() & (FLAG_S | FLAG_Z | FLAG_PV)) | (result & FLAG_XY) | (carry & FLAG_C);
        self.set_accumulator_and_flags(result, flags);
    }

    pub(crate) fn execute_cb_rotate_shift(&mut self, opcode: u8) {
        let code = opcode & 0x07;
        let value = self.read_reg8(code);
        let old_carry = u8::from(self.flags() & FLAG_C != 0);
        let (result, carry) = match (opcode >> 3) & 0x07 {
            0 => (value.rotate_left(1), value >> 7),
            1 => (value.rotate_right(1), value & 1),
            2 => ((value << 1) | old_carry, value >> 7),
            3 => ((value >> 1) | (old_carry << 7), value & 1),
            4 => (value << 1, value >> 7),
            5 => ((value >> 1) | (value & 0x80), value & 1),
            7 => (value >> 1, value & 1),
            _ => return,
        };
        let flags = Self::sign_zero_xy(result) | Self::parity_flag(result) | carry;
        self.write_reg8(code, result);
        self.set_flags(flags);
    }

    pub(crate) fn execute_cb_bit(&mut self, opcode: u8) {
        let bit = (opcode >> 3) & 0x07;
        let value = self.read_reg8(opcode & 0x07);
        let mask = 1_u8 << bit;
        let mut flags = (self.flags() & FLAG_C) | (value & FLAG_XY) | FLAG_H;
        if value & mask == 0 {
            flags |= FLAG_Z | FLAG_PV;
        } else if bit == 7 {
            flags |= FLAG_S;
        }
        self.set_flags(flags);
    }

    pub(crate) fn execute_cb_res(&mut self, opcode: u8) {
        let code = opcode & 0x07;
        let value = self.read_reg8(code);
        let mask = 1_u8 << ((opcode >> 3) & 0x07);
        self.write_reg8(code, value & !mask);
    }

    pub(crate) fn execute_cb_set(&mut self, opcode: u8) {
        let code = opcode & 0x07;
        let value = self.read_reg8(code);
        let mask = 1_u8 << ((opcode >> 3) & 0x07);
        self.write_reg8(code, value | mask);
    }

    pub(crate) fn execute_index<const IY: bool>(&mut self, opcode: u8) {
        let index_reg = if IY { Reg::IY } else { Reg::IX };

        match opcode {
            0x09 | 0x19 | 0x29 | 0x39 => {
                let index = self.registers.get(index_reg);
                let value = match (opcode >> 4) & 0x03 {
                    0 => self.registers.get(Reg::BC),
                    1 => self.registers.get(Reg::DE),
                    2 => index,
                    _ => self.registers.get(Reg::SP),
                };
                let sum = u32::from(index) + u32::from(value);
                let result = sum as u16;
                let mut flags = self.flags() & (FLAG_S | FLAG_Z | FLAG_PV);
                flags |= result.to_be_bytes()[0] & FLAG_XY;
                if ((index ^ value ^ result) & 0x1000) != 0 {
                    flags |= FLAG_H;
                }
                if sum > 0xffff {
                    flags |= FLAG_C;
                }
                self.registers.set(index_reg, result);
                self.set_flags(flags);
            }
            0x21 => {
                let value = self.read_word(self.instruction_pc.wrapping_add(2));
                self.registers.set(index_reg, value);
            }
            0x22 => {
                let address = self.read_word(self.instruction_pc.wrapping_add(2));
                self.write_word(address, self.registers.get(index_reg));
            }
            0x23 => {
                let value = self.registers.get(index_reg).wrapping_add(1);
                self.registers.set(index_reg, value);
            }
            0x2a => {
                let address = self.read_word(self.instruction_pc.wrapping_add(2));
                let value = self.read_word(address);
                self.registers.set(index_reg, value);
            }
            0x2b => {
                let value = self.registers.get(index_reg).wrapping_sub(1);
                self.registers.set(index_reg, value);
            }
            0x34 | 0x35 => {
                let displacement = self.read_logical(self.instruction_pc.wrapping_add(2)) as i8;
                let address = self
                    .registers
                    .get(index_reg)
                    .wrapping_add(i16::from(displacement) as u16);
                let value = self.read_logical(address);
                let result = if opcode == 0x34 {
                    value.wrapping_add(1)
                } else {
                    value.wrapping_sub(1)
                };
                let mut flags = Self::sign_zero_xy(result) | (self.flags() & FLAG_C);
                if opcode == 0x34 {
                    if value & 0x0f == 0x0f {
                        flags |= FLAG_H;
                    }
                    if value == 0x7f {
                        flags |= FLAG_PV;
                    }
                } else {
                    flags |= FLAG_N;
                    if value & 0x0f == 0 {
                        flags |= FLAG_H;
                    }
                    if value == 0x80 {
                        flags |= FLAG_PV;
                    }
                }
                self.write_logical(address, result);
                self.set_flags(flags);
            }
            0x36 => {
                let displacement = self.read_logical(self.instruction_pc.wrapping_add(2)) as i8;
                let address = self
                    .registers
                    .get(index_reg)
                    .wrapping_add(i16::from(displacement) as u16);
                let value = self.read_logical(self.instruction_pc.wrapping_add(3));
                self.write_logical(address, value);
            }
            0x46 | 0x4e | 0x56 | 0x5e | 0x66 | 0x6e | 0x7e => {
                let displacement = self.read_logical(self.instruction_pc.wrapping_add(2)) as i8;
                let address = self
                    .registers
                    .get(index_reg)
                    .wrapping_add(i16::from(displacement) as u16);
                let value = self.read_logical(address);
                self.write_reg8((opcode >> 3) & 0x07, value);
            }
            0x70..=0x75 | 0x77 => {
                let displacement = self.read_logical(self.instruction_pc.wrapping_add(2)) as i8;
                let address = self
                    .registers
                    .get(index_reg)
                    .wrapping_add(i16::from(displacement) as u16);
                let value = self.read_reg8(opcode & 0x07);
                self.write_logical(address, value);
            }
            0x86 | 0x8e | 0x96 | 0x9e | 0xa6 | 0xae | 0xb6 | 0xbe => {
                let displacement = self.read_logical(self.instruction_pc.wrapping_add(2)) as i8;
                let address = self
                    .registers
                    .get(index_reg)
                    .wrapping_add(i16::from(displacement) as u16);
                let value = self.read_logical(address);
                self.execute_alu((opcode >> 3) & 0x07, value);
            }
            0xe1 => {
                let value = self.pop_word();
                self.registers.set(index_reg, value);
            }
            0xe3 => {
                let sp = self.registers.get(Reg::SP);
                let memory_value = self.read_word(sp);
                let index = self.registers.get(index_reg);
                self.write_word(sp, index);
                self.registers.set(index_reg, memory_value);
            }
            0xe5 => self.push_word(self.registers.get(index_reg)),
            0xe9 => self.registers.set(Reg::PC, self.registers.get(index_reg)),
            0xf9 => self.registers.set(Reg::SP, self.registers.get(index_reg)),
            _ => {}
        }
    }

    pub(crate) fn execute_index_cb<const IY: bool>(&mut self, opcode: u8) {
        if opcode & 0x07 != 6 || (0x30..=0x37).contains(&opcode) {
            return;
        }

        let index_reg = if IY { Reg::IY } else { Reg::IX };
        let displacement = self.read_logical(self.instruction_pc.wrapping_add(2)) as i8;
        let address = self
            .registers
            .get(index_reg)
            .wrapping_add(i16::from(displacement) as u16);
        let value = self.read_logical(address);

        match opcode {
            0x00..=0x3f => {
                let old_carry = u8::from(self.flags() & FLAG_C != 0);
                let (result, carry) = match (opcode >> 3) & 0x07 {
                    0 => (value.rotate_left(1), value >> 7),
                    1 => (value.rotate_right(1), value & 1),
                    2 => ((value << 1) | old_carry, value >> 7),
                    3 => ((value >> 1) | (old_carry << 7), value & 1),
                    4 => (value << 1, value >> 7),
                    5 => ((value >> 1) | (value & 0x80), value & 1),
                    7 => (value >> 1, value & 1),
                    _ => return,
                };
                let flags = Self::sign_zero_xy(result) | Self::parity_flag(result) | carry;
                self.write_logical(address, result);
                self.set_flags(flags);
            }
            0x40..=0x7f => {
                let bit = (opcode >> 3) & 0x07;
                let mask = 1_u8 << bit;
                let mut flags = (self.flags() & FLAG_C) | (value & FLAG_XY) | FLAG_H;
                if value & mask == 0 {
                    flags |= FLAG_Z | FLAG_PV;
                } else if bit == 7 {
                    flags |= FLAG_S;
                }
                self.set_flags(flags);
            }
            0x80..=0xbf => {
                let mask = 1_u8 << ((opcode >> 3) & 0x07);
                self.write_logical(address, value & !mask);
            }
            _ => {
                let mask = 1_u8 << ((opcode >> 3) & 0x07);
                self.write_logical(address, value | mask);
            }
        }
    }

    pub(crate) fn execute_ed(&mut self, opcode: u8) {
        match opcode {
            0x00 | 0x08 | 0x10 | 0x18 | 0x20 | 0x28 | 0x30 | 0x38 => {
                let port = u16::from(self.read_logical(self.instruction_pc.wrapping_add(2)));
                let value = self.bus.io_read(port);
                let code = (opcode >> 3) & 0x07;
                if code != 6 {
                    self.write_reg8(code, value);
                }
                self.set_flags(
                    Self::sign_zero_xy(value) | Self::parity_flag(value) | (self.flags() & FLAG_C),
                );
            }
            0x01 | 0x09 | 0x11 | 0x19 | 0x21 | 0x29 | 0x39 => {
                let port = u16::from(self.read_logical(self.instruction_pc.wrapping_add(2)));
                let value = self.read_reg8((opcode >> 3) & 0x07);
                self.bus.io_write(port, value);
            }
            0x04 | 0x0c | 0x14 | 0x1c | 0x24 | 0x2c | 0x34 | 0x3c => {
                let result = self.accumulator() & self.read_reg8((opcode >> 3) & 0x07);
                self.set_flags(Self::sign_zero_xy(result) | Self::parity_flag(result) | FLAG_H);
            }
            0x40 | 0x48 | 0x50 | 0x58 | 0x60 | 0x68 | 0x78 => {
                let value = self.bus.io_read(self.registers.get(Reg::BC));
                self.write_reg8((opcode >> 3) & 0x07, value);
                self.set_flags(
                    Self::sign_zero_xy(value) | Self::parity_flag(value) | (self.flags() & FLAG_C),
                );
            }
            0x41 | 0x49 | 0x51 | 0x59 | 0x61 | 0x69 | 0x79 => {
                let value = self.read_reg8((opcode >> 3) & 0x07);
                self.bus.io_write(self.registers.get(Reg::BC), value);
            }
            0x42 | 0x52 | 0x62 | 0x72 | 0x4a | 0x5a | 0x6a | 0x7a => {
                let subtract = opcode & 0x08 == 0;
                let left = self.registers.get(Reg::HL);
                let right = self.reg16((opcode >> 4) & 0x03);
                let carry = u32::from(self.flags() & FLAG_C != 0);
                let result = if subtract {
                    left.wrapping_sub(right).wrapping_sub(carry as u16)
                } else {
                    left.wrapping_add(right).wrapping_add(carry as u16)
                };
                let mut flags = result.to_be_bytes()[0] & (FLAG_S | FLAG_XY);
                if result == 0 {
                    flags |= FLAG_Z;
                }
                if ((left ^ right ^ result) & 0x1000) != 0 {
                    flags |= FLAG_H;
                }
                if subtract {
                    flags |= FLAG_N;
                    if ((left ^ right) & (left ^ result) & 0x8000) != 0 {
                        flags |= FLAG_PV;
                    }
                    if u32::from(left) < u32::from(right) + carry {
                        flags |= FLAG_C;
                    }
                } else {
                    if (!(left ^ right) & (left ^ result) & 0x8000) != 0 {
                        flags |= FLAG_PV;
                    }
                    if u32::from(left) + u32::from(right) + carry > 0xffff {
                        flags |= FLAG_C;
                    }
                }
                self.registers.set(Reg::HL, result);
                self.set_flags(flags);
            }
            0x43 | 0x53 | 0x63 | 0x73 => {
                let address = self.read_word(self.instruction_pc.wrapping_add(2));
                self.write_word(address, self.reg16((opcode >> 4) & 0x03));
            }
            0x4b | 0x5b | 0x6b | 0x7b => {
                let address = self.read_word(self.instruction_pc.wrapping_add(2));
                let value = self.read_word(address);
                self.set_reg16((opcode >> 4) & 0x03, value);
            }
            0x44 => {
                let value = self.accumulator();
                self.set_accumulator(0);
                self.sub8(value, false, false);
            }
            0x45 | 0x4d => {
                let pc = self.pop_word();
                self.registers.set(Reg::PC, pc);
                if opcode == 0x45 {
                    self.iff1 = self.iff2;
                }
            }
            0x46 => self.interrupt_mode = 0,
            0x56 => self.interrupt_mode = 1,
            0x5e => self.interrupt_mode = 2,
            0x47 | 0x4f => {
                let [i, r] = self.registers.get(Reg::IR).to_be_bytes();
                let ir = if opcode == 0x47 {
                    u16::from_be_bytes([self.accumulator(), r])
                } else {
                    u16::from_be_bytes([i, self.accumulator()])
                };
                self.registers.set(Reg::IR, ir);
            }
            0x57 | 0x5f => {
                let [i, r] = self.registers.get(Reg::IR).to_be_bytes();
                let value = if opcode == 0x57 { i } else { r };
                let flags = Self::sign_zero_xy(value)
                    | (self.flags() & FLAG_C)
                    | (u8::from(self.iff2) * FLAG_PV);
                self.set_accumulator_and_flags(value, flags);
            }
            0x4c | 0x5c | 0x6c | 0x7c => {
                let code = (opcode >> 4) & 0x03;
                let [high, low] = self.reg16(code).to_be_bytes();
                self.set_reg16(code, u16::from(high) * u16::from(low));
            }
            0x64 => {
                let value = self.read_logical(self.instruction_pc.wrapping_add(2));
                let result = self.accumulator() & value;
                self.set_flags(Self::sign_zero_xy(result) | Self::parity_flag(result) | FLAG_H);
            }
            0x67 | 0x6f => {
                let address = self.registers.get(Reg::HL);
                let memory = self.read_logical(address);
                let accumulator = self.accumulator();
                let (next_memory, next_accumulator) = if opcode == 0x67 {
                    (
                        (accumulator << 4) | (memory >> 4),
                        (accumulator & 0xf0) | (memory & 0x0f),
                    )
                } else {
                    (
                        (memory << 4) | (accumulator & 0x0f),
                        (accumulator & 0xf0) | (memory >> 4),
                    )
                };
                self.write_logical(address, next_memory);
                let flags = Self::sign_zero_xy(next_accumulator)
                    | Self::parity_flag(next_accumulator)
                    | (self.flags() & FLAG_C);
                self.set_accumulator_and_flags(next_accumulator, flags);
            }
            0x74 => {
                let mask = self.read_logical(self.instruction_pc.wrapping_add(2));
                let value = self
                    .bus
                    .io_read(u16::from(self.registers.get(Reg::BC) as u8));
                let result = value & mask;
                self.set_flags(Self::sign_zero_xy(result) | Self::parity_flag(result) | FLAG_H);
            }
            0x76 => self.sleeping = true,
            0x83 | 0x8b | 0x93 | 0x9b => {
                let decrement = opcode & 0x08 != 0;
                let repeat = opcode & 0x10 != 0;
                loop {
                    let [b, c] = self.registers.get(Reg::BC).to_be_bytes();
                    let value = self.read_logical(self.registers.get(Reg::HL));
                    self.bus.io_write(u16::from(c), value);
                    let next_b = b.wrapping_sub(1);
                    let next_c = if decrement {
                        c.wrapping_sub(1)
                    } else {
                        c.wrapping_add(1)
                    };
                    let next_hl = if decrement {
                        self.registers.get(Reg::HL).wrapping_sub(1)
                    } else {
                        self.registers.get(Reg::HL).wrapping_add(1)
                    };
                    self.registers
                        .set(Reg::BC, u16::from_be_bytes([next_b, next_c]));
                    self.registers.set(Reg::HL, next_hl);
                    let mut flags = if value & 0x80 != 0 { FLAG_N } else { 0 };
                    if next_b == 0 {
                        flags |= FLAG_Z;
                        if repeat {
                            flags |= FLAG_PV;
                        }
                    }
                    self.set_flags(flags);
                    if !repeat || next_b == 0 {
                        break;
                    }
                    self.timing_repeat_iterations = self.timing_repeat_iterations.saturating_add(1);
                }
            }
            0xa0 | 0xa8 | 0xb0 | 0xb8 => {
                let decrement = opcode & 0x08 != 0;
                let repeat = opcode & 0x10 != 0;
                let value = self.read_logical(self.registers.get(Reg::HL));
                self.write_logical(self.registers.get(Reg::DE), value);
                let delta = if decrement { u16::MAX } else { 1 };
                self.registers
                    .set(Reg::HL, self.registers.get(Reg::HL).wrapping_add(delta));
                self.registers
                    .set(Reg::DE, self.registers.get(Reg::DE).wrapping_add(delta));
                let count = self.registers.get(Reg::BC).wrapping_sub(1);
                self.registers.set(Reg::BC, count);
                let mut flags = self.flags() & (FLAG_S | FLAG_Z | FLAG_C);
                if count != 0 {
                    flags |= FLAG_PV;
                }
                self.set_flags(flags);
                if repeat && count != 0 {
                    self.timing_repeat_iterations = 1;
                    self.registers
                        .set(Reg::PC, self.registers.get(Reg::PC).wrapping_sub(2));
                }
            }
            0xa1 | 0xa9 | 0xb1 | 0xb9 => {
                let decrement = opcode & 0x08 != 0;
                let repeat = opcode & 0x10 != 0;
                let value = self.read_logical(self.registers.get(Reg::HL));
                let accumulator = self.accumulator();
                let result = accumulator.wrapping_sub(value);
                let count = self.registers.get(Reg::BC).wrapping_sub(1);
                let delta = if decrement { u16::MAX } else { 1 };
                self.registers.set(Reg::BC, count);
                self.registers
                    .set(Reg::HL, self.registers.get(Reg::HL).wrapping_add(delta));
                let mut flags = Self::sign_zero_xy(result) | (self.flags() & FLAG_C) | FLAG_N;
                if accumulator & 0x0f < value & 0x0f {
                    flags |= FLAG_H;
                }
                if count != 0 {
                    flags |= FLAG_PV;
                }
                self.set_flags(flags);
                if repeat && count != 0 && result != 0 {
                    self.timing_repeat_iterations = 1;
                    self.registers
                        .set(Reg::PC, self.registers.get(Reg::PC).wrapping_sub(2));
                }
            }
            0xa2 | 0xaa | 0xb2 | 0xba | 0xa3 | 0xab | 0xb3 | 0xbb => {
                let input = opcode & 0x01 == 0;
                let decrement = opcode & 0x08 != 0;
                let repeat = opcode & 0x10 != 0;
                let [b, c] = self.registers.get(Reg::BC).to_be_bytes();
                let next_b = b.wrapping_sub(1);
                let port = u16::from_be_bytes([if input { b } else { next_b }, c]);
                let address = self.registers.get(Reg::HL);
                let value = if input {
                    let value = self.bus.io_read(port);
                    self.write_logical(address, value);
                    value
                } else {
                    let value = self.read_logical(address);
                    self.bus.io_write(port, value);
                    value
                };
                self.registers.set(Reg::BC, u16::from_be_bytes([next_b, c]));
                let delta = if decrement { u16::MAX } else { 1 };
                let next_hl = address.wrapping_add(delta);
                self.registers.set(Reg::HL, next_hl);
                let adjustment = if input {
                    c.wrapping_add(if decrement { u8::MAX } else { 1 })
                } else {
                    next_hl.to_le_bytes()[0]
                };
                let flag_sum = u16::from(value) + u16::from(adjustment);
                let mut flags = Self::sign_zero_xy(next_b);
                if value & 0x80 != 0 {
                    flags |= FLAG_N;
                }
                if flag_sum > u16::from(u8::MAX) {
                    flags |= FLAG_H | FLAG_C;
                }
                flags |= Self::parity_flag((flag_sum.to_le_bytes()[0] & 0x07) ^ next_b);
                if repeat && next_b != 0 {
                    let parity_input = if flags & FLAG_C != 0 {
                        flags &= !FLAG_H;
                        if flags & FLAG_N != 0 {
                            if next_b & 0x0f == 0 {
                                flags |= FLAG_H;
                            }
                            next_b.wrapping_sub(1) & 0x07
                        } else {
                            if next_b & 0x0f == 0x0f {
                                flags |= FLAG_H;
                            }
                            next_b.wrapping_add(1) & 0x07
                        }
                    } else {
                        next_b & 0x07
                    };
                    flags ^= Self::parity_flag(parity_input) ^ FLAG_PV;
                }
                self.set_flags(flags);
                if repeat && next_b != 0 {
                    self.timing_repeat_iterations = 1;
                    self.registers
                        .set(Reg::PC, self.registers.get(Reg::PC).wrapping_sub(2));
                }
            }
            _ => {}
        }
    }

    pub(crate) fn execute_ld_absolute_hl(&mut self, _opcode: u8) {
        let address = self.immediate16();
        self.write_word(address, self.registers.get(Reg::HL));
    }

    pub(crate) fn execute_ld_hl_absolute(&mut self, _opcode: u8) {
        let address = self.immediate16();
        let value = self.read_word(address);
        self.registers.set(Reg::HL, value);
    }

    pub(crate) fn execute_ld_absolute_a(&mut self, _opcode: u8) {
        let address = self.immediate16();
        self.write_logical(address, self.accumulator());
    }

    pub(crate) fn execute_ld_a_absolute(&mut self, _opcode: u8) {
        let address = self.immediate16();
        let value = self.read_logical(address);
        self.set_accumulator(value);
    }

    pub(crate) fn execute_daa(&mut self, _opcode: u8) {
        let accumulator = self.accumulator();
        let old_flags = self.flags();
        let subtract = old_flags & FLAG_N != 0;
        let mut correction = 0_u8;

        if old_flags & FLAG_H != 0 || accumulator & 0x0f > 9 {
            correction |= 0x06;
        }
        let carry = if old_flags & FLAG_C != 0 || accumulator > 0x99 {
            correction |= 0x60;
            true
        } else {
            false
        };

        let result = if subtract {
            accumulator.wrapping_sub(correction)
        } else {
            accumulator.wrapping_add(correction)
        };
        let mut flags =
            Self::sign_zero_xy(result) | Self::parity_flag(result) | (old_flags & FLAG_N);
        if (accumulator ^ result) & 0x10 != 0 {
            flags |= FLAG_H;
        }
        if carry {
            flags |= FLAG_C;
        }
        self.set_accumulator_and_flags(result, flags);
    }

    pub(crate) fn execute_cpl(&mut self, _opcode: u8) {
        let accumulator = !self.accumulator();
        let flags = (self.flags() & (FLAG_S | FLAG_Z | FLAG_PV | FLAG_C))
            | (accumulator & FLAG_XY)
            | FLAG_H
            | FLAG_N;
        self.set_accumulator_and_flags(accumulator, flags);
    }

    pub(crate) fn execute_scf(&mut self, _opcode: u8) {
        let flags =
            (self.flags() & (FLAG_S | FLAG_Z | FLAG_PV)) | (self.accumulator() & FLAG_XY) | FLAG_C;
        self.set_flags(flags);
    }

    pub(crate) fn execute_ccf(&mut self, _opcode: u8) {
        let old_carry = self.flags() & FLAG_C;
        let mut flags =
            (self.flags() & (FLAG_S | FLAG_Z | FLAG_PV)) | (self.accumulator() & FLAG_XY);
        if old_carry != 0 {
            flags |= FLAG_H;
        } else {
            flags |= FLAG_C;
        }
        self.set_flags(flags);
    }

    pub(crate) fn execute_ld_block(&mut self, opcode: u8) {
        let destination = (opcode >> 3) & 0x07;
        let source = opcode & 0x07;
        let value = self.read_reg8(source);
        self.write_reg8(destination, value);
    }

    pub(crate) fn execute_alu_reg8(&mut self, opcode: u8) {
        let value = self.read_reg8(opcode & 0x07);
        self.execute_alu((opcode >> 3) & 0x07, value);
    }

    pub(crate) fn execute_alu_immediate(&mut self, opcode: u8) {
        let value = self.immediate8();
        self.execute_alu((opcode >> 3) & 0x07, value);
    }

    pub(crate) fn execute_ex_af(&mut self, _opcode: u8) {
        let primary = self.registers.get(Reg::AF);
        let alternate = self.registers.get(Reg::AF2);
        self.registers.set(Reg::AF, alternate);
        self.registers.set(Reg::AF2, primary);
    }

    pub(crate) fn execute_djnz(&mut self, _opcode: u8) {
        let [b, c] = self.registers.get(Reg::BC).to_be_bytes();
        let next_b = b.wrapping_sub(1);
        self.registers.set(Reg::BC, u16::from_be_bytes([next_b, c]));
        if next_b != 0 {
            self.timing_branch_taken = true;
            let displacement = self.immediate8();
            self.registers
                .set(Reg::PC, self.relative_target(displacement));
        }
    }

    pub(crate) fn execute_jr(&mut self, _opcode: u8) {
        let displacement = self.immediate8();
        self.registers
            .set(Reg::PC, self.relative_target(displacement));
    }

    pub(crate) fn execute_jr_condition(&mut self, opcode: u8) {
        if self.condition((opcode >> 3) & 0x03) {
            self.timing_branch_taken = true;
            let displacement = self.immediate8();
            self.registers
                .set(Reg::PC, self.relative_target(displacement));
        }
    }

    pub(crate) fn execute_ret_condition(&mut self, opcode: u8) {
        if self.condition((opcode >> 3) & 0x07) {
            self.timing_branch_taken = true;
            let target = self.pop_word();
            self.registers.set(Reg::PC, target);
        }
    }

    pub(crate) fn execute_pop(&mut self, opcode: u8) {
        let value = self.pop_word();
        self.set_stack_reg16(opcode >> 4, value);
    }

    pub(crate) fn execute_jp_condition(&mut self, opcode: u8) {
        if self.condition((opcode >> 3) & 0x07) {
            self.timing_branch_taken = true;
            let target = self.immediate16();
            self.registers.set(Reg::PC, target);
        }
    }

    pub(crate) fn execute_call_condition(&mut self, opcode: u8) {
        if self.condition((opcode >> 3) & 0x07) {
            self.timing_branch_taken = true;
            let target = self.immediate16();
            self.push_word(self.registers.get(Reg::PC));
            self.registers.set(Reg::PC, target);
        }
    }

    pub(crate) fn execute_push(&mut self, opcode: u8) {
        self.push_word(self.stack_reg16(opcode >> 4));
    }

    pub(crate) fn execute_rst(&mut self, opcode: u8) {
        self.push_word(self.registers.get(Reg::PC));
        self.registers.set(Reg::PC, u16::from(opcode & 0x38));
    }

    pub(crate) fn execute_jp(&mut self, _opcode: u8) {
        let target = self.immediate16();
        self.registers.set(Reg::PC, target);
    }

    pub(crate) fn execute_ret(&mut self, _opcode: u8) {
        let target = self.pop_word();
        self.registers.set(Reg::PC, target);
    }

    pub(crate) fn execute_call(&mut self, _opcode: u8) {
        let target = self.immediate16();
        self.push_word(self.registers.get(Reg::PC));
        self.registers.set(Reg::PC, target);
    }

    pub(crate) fn execute_out_immediate(&mut self, _opcode: u8) {
        let accumulator = self.accumulator();
        let port = u16::from_be_bytes([accumulator, self.immediate8()]);
        self.bus.io_write(port, accumulator);
    }

    pub(crate) fn execute_exx(&mut self, _opcode: u8) {
        for (primary, alternate) in [
            (Reg::BC, Reg::BC2),
            (Reg::DE, Reg::DE2),
            (Reg::HL, Reg::HL2),
        ] {
            let primary_value = self.registers.get(primary);
            let alternate_value = self.registers.get(alternate);
            self.registers.set(primary, alternate_value);
            self.registers.set(alternate, primary_value);
        }
    }

    pub(crate) fn execute_in_immediate(&mut self, _opcode: u8) {
        let accumulator = self.accumulator();
        let port = u16::from_be_bytes([accumulator, self.immediate8()]);
        let value = self.bus.io_read(port);
        self.set_accumulator(value);
    }

    pub(crate) fn execute_ex_sp_hl(&mut self, _opcode: u8) {
        let sp = self.registers.get(Reg::SP);
        let memory_value = self.read_word(sp);
        let hl = self.registers.get(Reg::HL);
        self.write_word(sp, hl);
        self.registers.set(Reg::HL, memory_value);
    }

    pub(crate) fn execute_jp_hl(&mut self, _opcode: u8) {
        self.registers.set(Reg::PC, self.registers.get(Reg::HL));
    }

    pub(crate) fn execute_ex_de_hl(&mut self, _opcode: u8) {
        let de = self.registers.get(Reg::DE);
        let hl = self.registers.get(Reg::HL);
        self.registers.set(Reg::DE, hl);
        self.registers.set(Reg::HL, de);
    }

    pub(crate) fn execute_di(&mut self, _opcode: u8) {
        self.iff1 = false;
        self.iff2 = false;
        self.ei_shadow = false;
    }

    pub(crate) fn execute_ld_sp_hl(&mut self, _opcode: u8) {
        self.registers.set(Reg::SP, self.registers.get(Reg::HL));
    }

    pub(crate) fn execute_ei(&mut self, _opcode: u8) {
        self.iff1 = true;
        self.iff2 = true;
        self.ei_shadow = true;
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
    fn implemented_set_is_every_documented_unprefixed_opcode() {
        for opcode in 0_u8..=u8::MAX {
            let expected = !matches!(opcode, 0xcb | 0xdd | 0xed | 0xfd);
            assert_eq!(
                Z180::<NullBus>::is_instruction_implemented(&[opcode]),
                expected,
                "opcode {opcode:02x}"
            );
        }

        for opcode in 0_u8..=u8::MAX {
            let expected = !(0x30..=0x37).contains(&opcode);
            assert_eq!(
                Z180::<NullBus>::is_instruction_implemented(&[0xcb, opcode]),
                expected,
                "CB {opcode:02x}"
            );
        }

        for prefix in [0xdd, 0xfd] {
            for opcode in 0_u8..=u8::MAX {
                let expected = matches!(
                    opcode,
                    0x09 | 0x19
                        | 0x21
                        | 0x22
                        | 0x23
                        | 0x29
                        | 0x2a
                        | 0x2b
                        | 0x34
                        | 0x35
                        | 0x36
                        | 0x39
                        | 0x46
                        | 0x4e
                        | 0x56
                        | 0x5e
                        | 0x66
                        | 0x6e
                        | 0x70
                        ..=0x75
                            | 0x77
                            | 0x7e
                            | 0x86
                            | 0x8e
                            | 0x96
                            | 0x9e
                            | 0xa6
                            | 0xae
                            | 0xb6
                            | 0xbe
                            | 0xe1
                            | 0xe3
                            | 0xe5
                            | 0xe9
                            | 0xf9
                );
                assert_eq!(
                    Z180::<NullBus>::is_instruction_implemented(&[prefix, opcode]),
                    expected,
                    "{prefix:02x} {opcode:02x}"
                );
            }
        }

        for prefix in [0xdd, 0xfd] {
            for opcode in 0_u8..=u8::MAX {
                let expected = opcode & 0x07 == 6 && !(0x30..=0x37).contains(&opcode);
                assert_eq!(
                    Z180::<NullBus>::is_instruction_implemented(&[prefix, 0xcb, opcode]),
                    expected,
                    "{prefix:02x} cb __ {opcode:02x}"
                );
            }
        }
    }

    #[test]
    fn undefined_second_opcode_takes_trap() {
        for opcodes in [[0xcb, 0x30], [0xdd, 0x24], [0xed, 0x31], [0xfd, 0x24]] {
            let mut cpu = machine();
            cpu.mem_poke(0x1234, opcodes[0]);
            cpu.mem_poke(0x1235, opcodes[1]);
            cpu.set_reg(Reg::PC, 0x1234);
            cpu.set_reg(Reg::SP, 0x8000);
            cpu.set_reg(Reg::IR, 0x56fe);
            cpu.set_iff1(true);
            cpu.set_iff2(false);

            assert_eq!(cpu.step(), 17, "{opcodes:02x?}");
            assert_eq!(cpu.instruction_pc(), 0x1234, "{opcodes:02x?}");
            assert_eq!(cpu.reg(Reg::PC), 0, "{opcodes:02x?}");
            assert_eq!(cpu.reg(Reg::SP), 0x7ffe, "{opcodes:02x?}");
            assert_eq!(cpu.mem_peek(0x7ffe), 0x36, "{opcodes:02x?}");
            assert_eq!(cpu.mem_peek(0x7fff), 0x12, "{opcodes:02x?}");
            assert_eq!(cpu.reg(Reg::IR), 0x5680, "{opcodes:02x?}");
            assert_eq!(cpu.itc(), 0x81, "{opcodes:02x?}");
            assert!(cpu.iff1(), "{opcodes:02x?}");
            assert!(!cpu.iff2(), "{opcodes:02x?}");
            assert_eq!(cpu.cycle_count(), 17, "{opcodes:02x?}");
            assert_eq!(
                cpu.drain_events(),
                vec![Event::Trap {
                    cycle: 0,
                    pc: 0x1234,
                    opcode: [opcodes[0], opcodes[1], 0],
                    len: 2,
                }],
                "{opcodes:02x?}"
            );
            assert!(cpu.drain_events().is_empty(), "{opcodes:02x?}");
        }
    }

    #[test]
    fn undefined_third_opcode_takes_trap_with_ufo() {
        for prefix in [0xdd, 0xfd] {
            let mut cpu = machine();
            cpu.mem_poke(0x1234, prefix);
            cpu.mem_poke(0x1235, 0xcb);
            cpu.mem_poke(0x1236, 0x05);
            cpu.mem_poke(0x1237, 0x40);
            cpu.set_reg(Reg::PC, 0x1234);
            cpu.set_reg(Reg::SP, 0x8000);
            cpu.set_reg(Reg::IR, 0x567e);
            cpu.set_iff1(false);
            cpu.set_iff2(true);

            assert_eq!(cpu.step(), 23, "{prefix:02x}");
            assert_eq!(cpu.instruction_pc(), 0x1234, "{prefix:02x}");
            assert_eq!(cpu.reg(Reg::PC), 0, "{prefix:02x}");
            assert_eq!(cpu.reg(Reg::SP), 0x7ffe, "{prefix:02x}");
            assert_eq!(cpu.mem_peek(0x7ffe), 0x38, "{prefix:02x}");
            assert_eq!(cpu.mem_peek(0x7fff), 0x12, "{prefix:02x}");
            assert_eq!(cpu.reg(Reg::IR), 0x5601, "{prefix:02x}");
            assert_eq!(cpu.itc(), 0xc1, "{prefix:02x}");
            assert!(!cpu.iff1(), "{prefix:02x}");
            assert!(cpu.iff2(), "{prefix:02x}");
            assert_eq!(cpu.cycle_count(), 23, "{prefix:02x}");
            assert_eq!(
                cpu.drain_events(),
                vec![Event::Trap {
                    cycle: 0,
                    pc: 0x1234,
                    opcode: [prefix, 0xcb, 0x40],
                    len: 3,
                }],
                "{prefix:02x}"
            );
        }
    }

    #[test]
    fn daa_matches_every_accumulator_and_control_flag_combination() {
        let mut cpu = machine();
        cpu.mem_poke(0, 0x27);

        for accumulator in 0_u8..=u8::MAX {
            for controls in 0_u8..8 {
                let initial_flags = (u8::from(controls & 1 != 0) * FLAG_C)
                    | (u8::from(controls & 2 != 0) * FLAG_H)
                    | (u8::from(controls & 4 != 0) * FLAG_N);
                cpu.reset();
                cpu.set_reg(Reg::AF, u16::from_be_bytes([accumulator, initial_flags]));

                assert_eq!(cpu.step(), 4);

                let [actual_accumulator, actual_flags] = cpu.reg(Reg::AF).to_be_bytes();
                let (expected_accumulator, expected_flags) =
                    expected_daa(accumulator, initial_flags);
                assert_eq!(
                    (actual_accumulator, actual_flags),
                    (expected_accumulator, expected_flags),
                    "A={accumulator:02x} F={initial_flags:02x}"
                );
            }
        }
    }

    #[test]
    fn ei_shadow_lasts_through_exactly_the_following_instruction() {
        let mut cpu = machine();
        cpu.mem_poke(0, 0xfb);
        cpu.mem_poke(1, 0x00);
        cpu.mem_poke(2, 0xfb);
        cpu.mem_poke(3, 0xfb);
        cpu.mem_poke(4, 0xf3);

        assert_eq!(cpu.step(), 3);
        assert!(cpu.iff1());
        assert!(cpu.iff2());
        assert!(cpu.ei_shadow);

        assert_eq!(cpu.step(), 3);
        assert!(!cpu.ei_shadow);

        cpu.step();
        assert!(cpu.ei_shadow);
        cpu.step();
        assert!(cpu.ei_shadow);

        cpu.step();
        assert!(!cpu.iff1());
        assert!(!cpu.iff2());
        assert!(!cpu.ei_shadow);
    }

    #[test]
    fn interrupt_check_point_has_no_phase_two_sources() {
        let mut cpu = machine();
        cpu.ei_shadow = true;

        assert_eq!(cpu.interrupt_check_point(), None);
        assert!(cpu.ei_shadow);
    }

    fn expected_daa(accumulator: u8, flags: u8) -> (u8, u8) {
        let subtract = flags & FLAG_N != 0;
        let low_adjust = flags & FLAG_H != 0 || accumulator & 0x0f > 9;
        let high_adjust = flags & FLAG_C != 0 || accumulator > 0x99;
        let correction = u8::from(low_adjust) * 0x06 + u8::from(high_adjust) * 0x60;
        let result = if subtract {
            accumulator.wrapping_sub(correction)
        } else {
            accumulator.wrapping_add(correction)
        };
        let parity = u8::from(result.count_ones() & 1 == 0) * FLAG_PV;
        let half_carry = u8::from((accumulator ^ result) & 0x10 != 0) * FLAG_H;
        let zero = u8::from(result == 0) * FLAG_Z;
        let expected_flags = (result & (FLAG_S | FLAG_XY))
            | zero
            | half_carry
            | parity
            | (flags & FLAG_N)
            | (u8::from(high_adjust) * FLAG_C);
        (result, expected_flags)
    }

    #[test]
    fn nop_advances_pc_and_r_without_changing_registers() {
        let mut cpu = machine();
        cpu.mem_poke(0x1234, 0x00);
        cpu.set_reg(Reg::PC, 0x1234);
        cpu.set_reg(Reg::AF, 0x56d7);
        cpu.set_reg(Reg::BC, 0x89ab);
        cpu.set_reg(Reg::IR, 0x34ff);

        assert_eq!(cpu.step(), 3);

        assert_eq!(cpu.instruction_pc(), 0x1234);
        assert_eq!(cpu.reg(Reg::PC), 0x1235);
        assert_eq!(cpu.reg(Reg::AF), 0x56d7);
        assert_eq!(cpu.reg(Reg::BC), 0x89ab);
        assert_eq!(cpu.reg(Reg::IR), 0x3480);
        assert_eq!(cpu.cycle_count(), 3);
        assert!(!cpu.halted());
    }

    #[test]
    fn halt_enters_halted_state_and_leaves_flags_unchanged() {
        let mut cpu = machine();
        cpu.mem_poke(0, 0x76);
        cpu.set_reg(Reg::AF, 0x12a5);

        assert_eq!(cpu.step(), 3);
        assert_eq!(cpu.step(), 3);

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
        cpu.set_iff1(true);
        cpu.set_iff2(true);
        cpu.set_interrupt_mode(2);
        cpu.step();
        cpu.mem_poke(0x4000, 0xcc);

        cpu.reset();

        assert_eq!(cpu.reg(Reg::IR), 0);
        assert_eq!(cpu.reg(Reg::PC), 0);
        assert!(!cpu.halted());
        assert!(!cpu.iff1());
        assert!(!cpu.iff2());
        assert_eq!(cpu.interrupt_mode(), 0);
        assert_eq!(cpu.mem_peek(0x4000), 0xcc);
    }

    #[test]
    fn z180_repeat_block_output_sets_terminal_flags() {
        let cases: [(u8, u16, u16, u16, u8); 2] = [
            (0x93, 0x2000, 0x2001, 0x2002, 0x12),
            (0x9b, 0x2001, 0x2000, 0x1fff, 0x0e),
        ];
        for (opcode, initial_hl, final_value_address, final_hl, final_c) in cases {
            let mut cpu = machine();
            cpu.mem_poke(0, 0xed);
            cpu.mem_poke(1, opcode);
            cpu.mem_poke(initial_hl.into(), 0x01);
            cpu.mem_poke(final_value_address.into(), 0x80);
            cpu.set_reg(Reg::AF, 0x55ff);
            cpu.set_reg(Reg::BC, 0x0210);
            cpu.set_reg(Reg::HL, initial_hl);

            assert_eq!(cpu.step(), 30, "ED {opcode:02x}");
            assert_eq!(cpu.reg(Reg::AF), 0x5546, "ED {opcode:02x}");
            assert_eq!(cpu.reg(Reg::BC), u16::from(final_c), "ED {opcode:02x}");
            assert_eq!(cpu.reg(Reg::HL), final_hl, "ED {opcode:02x}");
        }
    }

    #[test]
    fn reti_preserves_iffs_while_retn_restores_iff1() {
        let mut reti = machine();
        reti.mem_poke(0, 0xed);
        reti.mem_poke(1, 0x4d);
        reti.mem_poke(0x2000, 0x34);
        reti.mem_poke(0x2001, 0x12);
        reti.set_reg(Reg::SP, 0x2000);
        reti.set_iff1(false);
        reti.set_iff2(true);

        assert_eq!(reti.step(), 22);

        assert_eq!(reti.reg(Reg::PC), 0x1234);
        assert_eq!(reti.reg(Reg::SP), 0x2002);
        assert!(!reti.iff1());
        assert!(reti.iff2());

        let mut retn = machine();
        retn.mem_poke(0, 0xed);
        retn.mem_poke(1, 0x45);
        retn.mem_poke(0x2000, 0x78);
        retn.mem_poke(0x2001, 0x56);
        retn.set_reg(Reg::SP, 0x2000);
        retn.set_iff1(false);
        retn.set_iff2(true);

        assert_eq!(retn.step(), 12);

        assert_eq!(retn.reg(Reg::PC), 0x5678);
        assert_eq!(retn.reg(Reg::SP), 0x2002);
        assert!(retn.iff1());
        assert!(retn.iff2());

        let config = MachineConfig {
            variant: Variant::Z8S180,
            regions: vec![RegionDef {
                base: 0,
                size: 0x1_0000,
                kind: RegionKind::Ram,
            }],
            ..MachineConfig::default()
        };
        let mut z8s180_reti =
            Z180::new(config, NullBus).expect("flat Z8S180 RAM configuration must be valid");
        z8s180_reti.mem_poke(0, 0xed);
        z8s180_reti.mem_poke(1, 0x4d);
        z8s180_reti.mem_poke(0x2000, 0x34);
        z8s180_reti.mem_poke(0x2001, 0x12);
        z8s180_reti.set_reg(Reg::SP, 0x2000);
        assert_eq!(z8s180_reti.step(), 12);
    }

    #[test]
    fn timing_selects_conditional_and_repeat_paths() {
        let mut call_not_taken = machine();
        call_not_taken.mem_poke(0, 0xc4);
        call_not_taken.mem_poke(1, 0x34);
        call_not_taken.mem_poke(2, 0x12);
        call_not_taken.set_reg(Reg::AF, u16::from(FLAG_Z));
        assert_eq!(call_not_taken.step(), 6);
        assert_eq!(call_not_taken.reg(Reg::PC), 3);

        let mut call_taken = machine();
        call_taken.mem_poke(0, 0xc4);
        call_taken.mem_poke(1, 0x34);
        call_taken.mem_poke(2, 0x12);
        call_taken.set_reg(Reg::SP, 0x2000);
        assert_eq!(call_taken.step(), 16);
        assert_eq!(call_taken.reg(Reg::PC), 0x1234);

        let mut ldir_terminal = machine();
        ldir_terminal.mem_poke(0, 0xed);
        ldir_terminal.mem_poke(1, 0xb0);
        ldir_terminal.mem_poke(0x1000, 0x5a);
        ldir_terminal.set_reg(Reg::BC, 1);
        ldir_terminal.set_reg(Reg::DE, 0x2000);
        ldir_terminal.set_reg(Reg::HL, 0x1000);
        assert_eq!(ldir_terminal.step(), 12);
        assert_eq!(ldir_terminal.reg(Reg::PC), 2);

        let mut ldir_repeating = machine();
        ldir_repeating.mem_poke(0, 0xed);
        ldir_repeating.mem_poke(1, 0xb0);
        ldir_repeating.mem_poke(0x1000, 0x5a);
        ldir_repeating.set_reg(Reg::BC, 2);
        ldir_repeating.set_reg(Reg::DE, 0x2000);
        ldir_repeating.set_reg(Reg::HL, 0x1000);
        assert_eq!(ldir_repeating.step(), 14);
        assert_eq!(ldir_repeating.reg(Reg::PC), 0);
    }
}
