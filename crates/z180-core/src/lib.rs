#![forbid(unsafe_code)]
#![no_std]

extern crate alloc;

mod memory;
mod optable;
mod registers;

pub use memory::{ConfigError, MachineConfig, RegionDef, RegionKind, Variant};
pub use registers::Reg;

use memory::Memory;
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

pub struct Z180<B: HostBus> {
    registers: Registers,
    memory: Memory,
    bus: B,
    instruction_pc: u16,
    cycle_count: u64,
    halted: bool,
    iff1: bool,
    iff2: bool,
    ei_shadow: bool,
    interrupt_mode: u8,
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
            iff1: false,
            iff2: false,
            ei_shadow: false,
            interrupt_mode: 0,
        })
    }

    pub fn reset(&mut self) {
        self.registers = Registers::default();
        self.instruction_pc = 0;
        self.halted = false;
        self.iff1 = false;
        self.iff2 = false;
        self.ei_shadow = false;
        self.interrupt_mode = 0;
    }

    pub fn step(&mut self) -> u32 {
        if let Some(cycles) = self.interrupt_check_point() {
            return self.finish_step(cycles);
        }

        if self.halted {
            return self.finish_step(1);
        }

        let pc = self.registers.get(Reg::PC);
        self.instruction_pc = pc;
        let first_opcode = self.read_logical(pc);
        let (opcode, descriptor, m1_fetches) = if first_opcode == 0xcb {
            let opcode = self.read_logical(pc.wrapping_add(1));
            (opcode, Self::CB_OPCODES[usize::from(opcode)], 2)
        } else {
            (
                first_opcode,
                Self::MAIN_OPCODES[usize::from(first_opcode)],
                1,
            )
        };
        let Some(handler) = descriptor.handler else {
            return 0;
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

        handler(self, opcode);

        // Phase 1 deliberately has no timing model. This non-hardware unit lets
        // the execution skeleton make progress until UM0050 timings land in P4.
        self.finish_step(descriptor.cycles.map_or(1, u32::from))
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
            _ => false,
        }
    }

    fn interrupt_check_point(&mut self) -> Option<u32> {
        // Phase 2 establishes the pre-fetch service boundary only. Phase 5
        // owns interrupt pins, prioritized sources, acknowledge behavior, and
        // HALT wake-up, so no source can fire here yet.
        None
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
            let displacement = self.immediate8();
            self.registers
                .set(Reg::PC, self.relative_target(displacement));
        }
    }

    pub(crate) fn execute_ret_condition(&mut self, opcode: u8) {
        if self.condition((opcode >> 3) & 0x07) {
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
            let target = self.immediate16();
            self.registers.set(Reg::PC, target);
        }
    }

    pub(crate) fn execute_call_condition(&mut self, opcode: u8) {
        if self.condition((opcode >> 3) & 0x07) {
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

        assert!(!Z180::<NullBus>::is_instruction_implemented(&[0xdd, 0x00]));
    }

    #[test]
    fn unimplemented_opcode_does_not_execute() {
        let mut cpu = machine();
        cpu.mem_poke(0x1234, 0xcb);
        cpu.mem_poke(0x1235, 0x30);
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

                assert_eq!(cpu.step(), 1);

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

        cpu.step();
        assert!(cpu.iff1());
        assert!(cpu.iff2());
        assert!(cpu.ei_shadow);

        cpu.step();
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
}
