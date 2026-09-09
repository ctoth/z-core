use crate::*;

impl<B: HostBus> Z180<B> {
    #[allow(
        clippy::too_many_lines,
        reason = "the indexed-page dispatcher stays aligned with the single auditable opcode table"
    )]
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
                    if value.trailing_zeros() >= 4 {
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
        let displacement = self
            .indexed_displacement
            .take()
            .expect("indexed-bit displacement was not decoded");
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
}
