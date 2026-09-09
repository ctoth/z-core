use super::*;
mod extended;
mod indexed;

impl<B: HostBus> Z180<B> {
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
    pub(super) fn read_word(&mut self, address: u16) -> u16 {
        let low = self.read_logical(address);
        let high = self.read_logical(address.wrapping_add(1));
        u16::from_le_bytes([low, high])
    }
    fn write_word(&mut self, address: u16, value: u16) {
        let [low, high] = value.to_le_bytes();
        self.write_logical(address, low);
        self.write_logical(address.wrapping_add(1), high);
    }
    pub(super) fn push_word(&mut self, value: u16) {
        let sp = self.registers.get(Reg::SP);
        let [low, high] = value.to_le_bytes();
        self.write_logical(sp.wrapping_sub(1), high);
        self.write_logical(sp.wrapping_sub(2), low);
        self.registers.set(Reg::SP, sp.wrapping_sub(2));
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
    #[allow(
        clippy::unused_self,
        reason = "all opcode handlers share the table's method-pointer signature, including NOP"
    )]
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
        if value.trailing_zeros() >= 4 {
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
        let displacement = self.immediate8();
        if next_b != 0 {
            self.timing_branch_taken = true;
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
        let displacement = self.immediate8();
        if self.condition((opcode >> 3) & 0x03) {
            self.timing_branch_taken = true;
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
        let target = self.immediate16();
        if self.condition((opcode >> 3) & 0x07) {
            self.timing_branch_taken = true;
            self.registers.set(Reg::PC, target);
        }
    }
    pub(crate) fn execute_call_condition(&mut self, opcode: u8) {
        let target = self.immediate16();
        if self.condition((opcode >> 3) & 0x07) {
            self.timing_branch_taken = true;
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
        self.write_io(port, accumulator);
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
        let value = self.read_io(port);
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
}
