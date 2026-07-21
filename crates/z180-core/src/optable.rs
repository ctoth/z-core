use crate::{HostBus, Z180};

pub(crate) type Handler<B> = fn(&mut Z180<B>, u8);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OperandKind {
    None,
    Reg8Destination,
    Reg8Source,
}

#[derive(Debug)]
pub(crate) struct Opcode<B: HostBus> {
    pub(crate) mnemonic: &'static str,
    #[allow(
        dead_code,
        reason = "consumed by the planned table-driven disassembler and docs generator"
    )]
    pub(crate) operands: [OperandKind; 2],
    pub(crate) length: u8,
    pub(crate) cycles: Option<u8>,
    pub(crate) handler: Option<Handler<B>>,
}

impl<B: HostBus> Clone for Opcode<B> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<B: HostBus> Copy for Opcode<B> {}

impl<B: HostBus> Opcode<B> {
    const UNIMPLEMENTED: Self = Self {
        mnemonic: "",
        operands: [OperandKind::None; 2],
        length: 0,
        cycles: None,
        handler: None,
    };

    const fn implemented(
        mnemonic: &'static str,
        operands: [OperandKind; 2],
        handler: Handler<B>,
    ) -> Self {
        Self {
            mnemonic,
            operands,
            length: 1,
            // Hardware cycle counts are deliberately absent until Phase 4
            // verifies and transcribes the UM0050 timing tables.
            cycles: None,
            handler: Some(handler),
        }
    }
}

const fn build_main_table<B: HostBus>() -> [Opcode<B>; 256] {
    let mut table = [Opcode::UNIMPLEMENTED; 256];
    table[0x00] = Opcode::implemented("NOP", [OperandKind::None; 2], Z180::<B>::execute_nop);
    table[0x76] = Opcode::implemented("HALT", [OperandKind::None; 2], Z180::<B>::execute_halt);

    let mut opcode = 0x40_usize;
    while opcode <= 0x7f {
        if opcode != 0x76 {
            table[opcode] = Opcode::implemented(
                "LD {dst},{src}",
                [OperandKind::Reg8Destination, OperandKind::Reg8Source],
                Z180::<B>::execute_ld_block,
            );
        }
        opcode += 1;
    }
    table
}

impl<B: HostBus> Z180<B> {
    pub(crate) const MAIN_OPCODES: [Opcode<B>; 256] = build_main_table::<B>();
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn main_table_contains_only_the_phase_one_stub_handlers() {
        let table = &Z180::<NullBus>::MAIN_OPCODES;
        for (opcode, entry) in table.iter().enumerate() {
            let expected = opcode == 0x00 || (0x40..=0x7f).contains(&opcode);
            assert_eq!(entry.handler.is_some(), expected, "opcode {opcode:02x}");
        }
    }

    #[test]
    fn stub_metadata_is_defined_once_in_the_table() {
        let table = &Z180::<NullBus>::MAIN_OPCODES;
        assert_eq!(table[0x00].mnemonic, "NOP");
        assert_eq!(table[0x00].length, 1);
        assert_eq!(table[0x00].cycles, None);
        assert_eq!(table[0x76].mnemonic, "HALT");
        assert_eq!(table[0x78].mnemonic, "LD {dst},{src}");
        assert_eq!(
            table[0x78].operands,
            [OperandKind::Reg8Destination, OperandKind::Reg8Source]
        );
    }
}
