#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reg {
    PC,
    SP,
    AF,
    BC,
    DE,
    HL,
    IX,
    IY,
    AF2,
    BC2,
    DE2,
    HL2,
    IR,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Registers {
    pc: u16,
    sp: u16,
    af: u16,
    bc: u16,
    de: u16,
    hl: u16,
    ix: u16,
    iy: u16,
    af2: u16,
    bc2: u16,
    de2: u16,
    hl2: u16,
    ir: u16,
}

impl Registers {
    pub(crate) fn get(self, reg: Reg) -> u16 {
        match reg {
            Reg::PC => self.pc,
            Reg::SP => self.sp,
            Reg::AF => self.af,
            Reg::BC => self.bc,
            Reg::DE => self.de,
            Reg::HL => self.hl,
            Reg::IX => self.ix,
            Reg::IY => self.iy,
            Reg::AF2 => self.af2,
            Reg::BC2 => self.bc2,
            Reg::DE2 => self.de2,
            Reg::HL2 => self.hl2,
            Reg::IR => self.ir,
        }
    }

    pub(crate) fn set(&mut self, reg: Reg, value: u16) {
        match reg {
            Reg::PC => self.pc = value,
            Reg::SP => self.sp = value,
            Reg::AF => self.af = value,
            Reg::BC => self.bc = value,
            Reg::DE => self.de = value,
            Reg::HL => self.hl = value,
            Reg::IX => self.ix = value,
            Reg::IY => self.iy = value,
            Reg::AF2 => self.af2 = value,
            Reg::BC2 => self.bc2 = value,
            Reg::DE2 => self.de2 = value,
            Reg::HL2 => self.hl2 = value,
            Reg::IR => self.ir = value,
        }
    }

    pub(crate) fn byte(self, code: u8) -> Option<u8> {
        match code & 0x07 {
            0 => Some(self.bc.to_be_bytes()[0]),
            1 => Some(self.bc.to_be_bytes()[1]),
            2 => Some(self.de.to_be_bytes()[0]),
            3 => Some(self.de.to_be_bytes()[1]),
            4 => Some(self.hl.to_be_bytes()[0]),
            5 => Some(self.hl.to_be_bytes()[1]),
            7 => Some(self.af.to_be_bytes()[0]),
            _ => None,
        }
    }

    pub(crate) fn set_byte(&mut self, code: u8, value: u8) -> bool {
        match code & 0x07 {
            0 => self.bc = u16::from_be_bytes([value, self.bc.to_be_bytes()[1]]),
            1 => self.bc = u16::from_be_bytes([self.bc.to_be_bytes()[0], value]),
            2 => self.de = u16::from_be_bytes([value, self.de.to_be_bytes()[1]]),
            3 => self.de = u16::from_be_bytes([self.de.to_be_bytes()[0], value]),
            4 => self.hl = u16::from_be_bytes([value, self.hl.to_be_bytes()[1]]),
            5 => self.hl = u16::from_be_bytes([self.hl.to_be_bytes()[0], value]),
            7 => self.af = u16::from_be_bytes([value, self.af.to_be_bytes()[1]]),
            _ => return false,
        }
        true
    }

    pub(crate) fn increment_pc(&mut self) {
        self.pc = self.pc.wrapping_add(1);
    }

    pub(crate) fn increment_r(&mut self) {
        let [i, r] = self.ir.to_be_bytes();
        let next_r = (r & 0x80) | (r.wrapping_add(1) & 0x7f);
        self.ir = u16::from_be_bytes([i, next_r]);
    }
}
