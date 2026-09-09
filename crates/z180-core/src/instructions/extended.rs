use crate::*;

impl<B: HostBus> Z180<B> {
    #[allow(
        clippy::too_many_lines,
        reason = "the ED-page dispatcher stays aligned with the single auditable opcode table"
    )]
    pub(crate) fn execute_ed(&mut self, opcode: u8) {
        match opcode {
            0x00 | 0x08 | 0x10 | 0x18 | 0x20 | 0x28 | 0x30 | 0x38 => {
                let port = u16::from(self.read_logical(self.instruction_pc.wrapping_add(2)));
                let value = self.read_io(port);
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
                self.write_io(port, value);
            }
            0x04 | 0x0c | 0x14 | 0x1c | 0x24 | 0x2c | 0x34 | 0x3c => {
                let result = self.accumulator() & self.read_reg8((opcode >> 3) & 0x07);
                self.set_flags(Self::sign_zero_xy(result) | Self::parity_flag(result) | FLAG_H);
            }
            0x40 | 0x48 | 0x50 | 0x58 | 0x60 | 0x68 | 0x78 => {
                let value = self.read_io(self.registers.get(Reg::BC));
                self.write_reg8((opcode >> 3) & 0x07, value);
                self.set_flags(
                    Self::sign_zero_xy(value) | Self::parity_flag(value) | (self.flags() & FLAG_C),
                );
            }
            0x41 | 0x49 | 0x51 | 0x59 | 0x61 | 0x69 | 0x79 => {
                let value = self.read_reg8((opcode >> 3) & 0x07);
                self.write_io(self.registers.get(Reg::BC), value);
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
                let value = self.read_io(u16::from(self.registers.get(Reg::BC) as u8));
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
                    self.write_io(u16::from(c), value);
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
                    let value = self.read_io(port);
                    self.write_logical(address, value);
                    value
                } else {
                    let value = self.read_logical(address);
                    self.write_io(port, value);
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
                            if next_b.trailing_zeros() >= 4 {
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
}
