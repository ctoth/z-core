use crate::{HostBus, Z180};

pub(crate) type Handler<B> = fn(&mut Z180<B>, u8);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OperandKind {
    None,
    Accumulator,
    Bit,
    Condition,
    Immediate8,
    Immediate16,
    IndirectBc,
    IndirectDe,
    IndirectHl,
    IndirectIndex,
    IndirectSp,
    IndirectImmediate16,
    PortImmediate,
    Reg8Destination,
    Reg8Source,
    Reg16,
    Reg16Hl,
    Reg16Index,
    Reg16Sp,
    Reg16Stack,
    Relative8,
    RestartVector,
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
        length: u8,
        handler: Handler<B>,
    ) -> Self {
        Self {
            mnemonic,
            operands,
            length,
            // Hardware cycle counts are deliberately absent until Phase 4
            // verifies and transcribes the UM0050 timing tables.
            cycles: None,
            handler: Some(handler),
        }
    }
}

const fn build_main_table<B: HostBus>() -> [Opcode<B>; 256] {
    let mut table = [Opcode::UNIMPLEMENTED; 256];

    table[0x00] = Opcode::implemented("NOP", [OperandKind::None; 2], 1, Z180::<B>::execute_nop);

    let mut pair = 0_usize;
    while pair < 4 {
        table[0x01 + pair * 0x10] = Opcode::implemented(
            "LD {rr},{nn}",
            [OperandKind::Reg16, OperandKind::Immediate16],
            3,
            Z180::<B>::execute_ld_reg16_immediate,
        );
        table[0x03 + pair * 0x10] = Opcode::implemented(
            "INC {rr}",
            [OperandKind::Reg16, OperandKind::None],
            1,
            Z180::<B>::execute_inc_reg16,
        );
        table[0x09 + pair * 0x10] = Opcode::implemented(
            "ADD HL,{rr}",
            [OperandKind::Reg16Hl, OperandKind::Reg16],
            1,
            Z180::<B>::execute_add_hl,
        );
        table[0x0b + pair * 0x10] = Opcode::implemented(
            "DEC {rr}",
            [OperandKind::Reg16, OperandKind::None],
            1,
            Z180::<B>::execute_dec_reg16,
        );
        pair += 1;
    }

    table[0x02] = Opcode::implemented(
        "LD (BC),A",
        [OperandKind::IndirectBc, OperandKind::Accumulator],
        1,
        Z180::<B>::execute_ld_indirect_a,
    );
    table[0x0a] = Opcode::implemented(
        "LD A,(BC)",
        [OperandKind::Accumulator, OperandKind::IndirectBc],
        1,
        Z180::<B>::execute_ld_a_indirect,
    );
    table[0x12] = Opcode::implemented(
        "LD (DE),A",
        [OperandKind::IndirectDe, OperandKind::Accumulator],
        1,
        Z180::<B>::execute_ld_indirect_a,
    );
    table[0x1a] = Opcode::implemented(
        "LD A,(DE)",
        [OperandKind::Accumulator, OperandKind::IndirectDe],
        1,
        Z180::<B>::execute_ld_a_indirect,
    );

    let mut register = 0_usize;
    while register < 8 {
        table[0x04 + register * 8] = Opcode::implemented(
            "INC {r}",
            [OperandKind::Reg8Destination, OperandKind::None],
            1,
            Z180::<B>::execute_inc_reg8,
        );
        table[0x05 + register * 8] = Opcode::implemented(
            "DEC {r}",
            [OperandKind::Reg8Destination, OperandKind::None],
            1,
            Z180::<B>::execute_dec_reg8,
        );
        table[0x06 + register * 8] = Opcode::implemented(
            "LD {r},{n}",
            [OperandKind::Reg8Destination, OperandKind::Immediate8],
            2,
            Z180::<B>::execute_ld_reg8_immediate,
        );
        register += 1;
    }

    table[0x07] = Opcode::implemented(
        "RLCA",
        [OperandKind::Accumulator, OperandKind::None],
        1,
        Z180::<B>::execute_accumulator_rotate,
    );
    table[0x08] = Opcode::implemented(
        "EX AF,AF'",
        [OperandKind::None; 2],
        1,
        Z180::<B>::execute_ex_af,
    );
    table[0x0f] = Opcode::implemented(
        "RRCA",
        [OperandKind::Accumulator, OperandKind::None],
        1,
        Z180::<B>::execute_accumulator_rotate,
    );
    table[0x10] = Opcode::implemented(
        "DJNZ {rel}",
        [OperandKind::Relative8, OperandKind::None],
        2,
        Z180::<B>::execute_djnz,
    );
    table[0x17] = Opcode::implemented(
        "RLA",
        [OperandKind::Accumulator, OperandKind::None],
        1,
        Z180::<B>::execute_accumulator_rotate,
    );
    table[0x18] = Opcode::implemented(
        "JR {rel}",
        [OperandKind::Relative8, OperandKind::None],
        2,
        Z180::<B>::execute_jr,
    );
    table[0x1f] = Opcode::implemented(
        "RRA",
        [OperandKind::Accumulator, OperandKind::None],
        1,
        Z180::<B>::execute_accumulator_rotate,
    );

    let mut relative_condition = 0_usize;
    while relative_condition < 4 {
        table[0x20 + relative_condition * 8] = Opcode::implemented(
            "JR {cc},{rel}",
            [OperandKind::Condition, OperandKind::Relative8],
            2,
            Z180::<B>::execute_jr_condition,
        );
        relative_condition += 1;
    }

    table[0x22] = Opcode::implemented(
        "LD ({nn}),HL",
        [OperandKind::IndirectImmediate16, OperandKind::Reg16Hl],
        3,
        Z180::<B>::execute_ld_absolute_hl,
    );
    table[0x27] = Opcode::implemented(
        "DAA",
        [OperandKind::Accumulator, OperandKind::None],
        1,
        Z180::<B>::execute_daa,
    );
    table[0x2a] = Opcode::implemented(
        "LD HL,({nn})",
        [OperandKind::Reg16Hl, OperandKind::IndirectImmediate16],
        3,
        Z180::<B>::execute_ld_hl_absolute,
    );
    table[0x2f] = Opcode::implemented(
        "CPL",
        [OperandKind::Accumulator, OperandKind::None],
        1,
        Z180::<B>::execute_cpl,
    );
    table[0x32] = Opcode::implemented(
        "LD ({nn}),A",
        [OperandKind::IndirectImmediate16, OperandKind::Accumulator],
        3,
        Z180::<B>::execute_ld_absolute_a,
    );
    table[0x37] = Opcode::implemented("SCF", [OperandKind::None; 2], 1, Z180::<B>::execute_scf);
    table[0x3a] = Opcode::implemented(
        "LD A,({nn})",
        [OperandKind::Accumulator, OperandKind::IndirectImmediate16],
        3,
        Z180::<B>::execute_ld_a_absolute,
    );
    table[0x3f] = Opcode::implemented("CCF", [OperandKind::None; 2], 1, Z180::<B>::execute_ccf);

    let mut opcode = 0x40_usize;
    while opcode <= 0x7f {
        if opcode != 0x76 {
            table[opcode] = Opcode::implemented(
                "LD {dst},{src}",
                [OperandKind::Reg8Destination, OperandKind::Reg8Source],
                1,
                Z180::<B>::execute_ld_block,
            );
        }
        opcode += 1;
    }

    table[0x76] = Opcode::implemented("HALT", [OperandKind::None; 2], 1, Z180::<B>::execute_halt);

    opcode = 0x80;
    while opcode <= 0xbf {
        let mnemonic = match (opcode >> 3) & 0x07 {
            0 => "ADD A,{src}",
            1 => "ADC A,{src}",
            2 => "SUB {src}",
            3 => "SBC A,{src}",
            4 => "AND {src}",
            5 => "XOR {src}",
            6 => "OR {src}",
            _ => "CP {src}",
        };
        table[opcode] = Opcode::implemented(
            mnemonic,
            [OperandKind::Accumulator, OperandKind::Reg8Source],
            1,
            Z180::<B>::execute_alu_reg8,
        );
        opcode += 1;
    }

    let mut condition = 0_usize;
    while condition < 8 {
        table[0xc0 + condition * 8] = Opcode::implemented(
            "RET {cc}",
            [OperandKind::Condition, OperandKind::None],
            1,
            Z180::<B>::execute_ret_condition,
        );
        table[0xc2 + condition * 8] = Opcode::implemented(
            "JP {cc},{nn}",
            [OperandKind::Condition, OperandKind::Immediate16],
            3,
            Z180::<B>::execute_jp_condition,
        );
        table[0xc4 + condition * 8] = Opcode::implemented(
            "CALL {cc},{nn}",
            [OperandKind::Condition, OperandKind::Immediate16],
            3,
            Z180::<B>::execute_call_condition,
        );
        condition += 1;
    }

    pair = 0;
    while pair < 4 {
        table[0xc1 + pair * 0x10] = Opcode::implemented(
            "POP {qq}",
            [OperandKind::Reg16Stack, OperandKind::None],
            1,
            Z180::<B>::execute_pop,
        );
        table[0xc5 + pair * 0x10] = Opcode::implemented(
            "PUSH {qq}",
            [OperandKind::Reg16Stack, OperandKind::None],
            1,
            Z180::<B>::execute_push,
        );
        pair += 1;
    }

    let mut operation = 0_usize;
    while operation < 8 {
        let mnemonic = match operation {
            0 => "ADD A,{n}",
            1 => "ADC A,{n}",
            2 => "SUB {n}",
            3 => "SBC A,{n}",
            4 => "AND {n}",
            5 => "XOR {n}",
            6 => "OR {n}",
            _ => "CP {n}",
        };
        table[0xc6 + operation * 8] = Opcode::implemented(
            mnemonic,
            [OperandKind::Accumulator, OperandKind::Immediate8],
            2,
            Z180::<B>::execute_alu_immediate,
        );
        table[0xc7 + operation * 8] = Opcode::implemented(
            "RST {vector}",
            [OperandKind::RestartVector, OperandKind::None],
            1,
            Z180::<B>::execute_rst,
        );
        operation += 1;
    }

    table[0xc3] = Opcode::implemented(
        "JP {nn}",
        [OperandKind::Immediate16, OperandKind::None],
        3,
        Z180::<B>::execute_jp,
    );
    table[0xc9] = Opcode::implemented("RET", [OperandKind::None; 2], 1, Z180::<B>::execute_ret);
    table[0xcd] = Opcode::implemented(
        "CALL {nn}",
        [OperandKind::Immediate16, OperandKind::None],
        3,
        Z180::<B>::execute_call,
    );
    table[0xd3] = Opcode::implemented(
        "OUT ({n}),A",
        [OperandKind::PortImmediate, OperandKind::Accumulator],
        2,
        Z180::<B>::execute_out_immediate,
    );
    table[0xd9] = Opcode::implemented("EXX", [OperandKind::None; 2], 1, Z180::<B>::execute_exx);
    table[0xdb] = Opcode::implemented(
        "IN A,({n})",
        [OperandKind::Accumulator, OperandKind::PortImmediate],
        2,
        Z180::<B>::execute_in_immediate,
    );
    table[0xe3] = Opcode::implemented(
        "EX (SP),HL",
        [OperandKind::IndirectSp, OperandKind::Reg16Hl],
        1,
        Z180::<B>::execute_ex_sp_hl,
    );
    table[0xe9] = Opcode::implemented(
        "JP (HL)",
        [OperandKind::IndirectHl, OperandKind::None],
        1,
        Z180::<B>::execute_jp_hl,
    );
    table[0xeb] = Opcode::implemented(
        "EX DE,HL",
        [OperandKind::None; 2],
        1,
        Z180::<B>::execute_ex_de_hl,
    );
    table[0xf3] = Opcode::implemented("DI", [OperandKind::None; 2], 1, Z180::<B>::execute_di);
    table[0xf9] = Opcode::implemented(
        "LD SP,HL",
        [OperandKind::Reg16Sp, OperandKind::Reg16Hl],
        1,
        Z180::<B>::execute_ld_sp_hl,
    );
    table[0xfb] = Opcode::implemented("EI", [OperandKind::None; 2], 1, Z180::<B>::execute_ei);

    table
}

const fn build_cb_table<B: HostBus>() -> [Opcode<B>; 256] {
    let mut table = [Opcode::UNIMPLEMENTED; 256];
    let mut opcode = 0_usize;

    while opcode < 0x40 {
        if opcode < 0x30 || opcode >= 0x38 {
            let mnemonic = match (opcode >> 3) & 0x07 {
                0 => "RLC {r}",
                1 => "RRC {r}",
                2 => "RL {r}",
                3 => "RR {r}",
                4 => "SLA {r}",
                5 => "SRA {r}",
                _ => "SRL {r}",
            };
            table[opcode] = Opcode::implemented(
                mnemonic,
                [OperandKind::Reg8Destination, OperandKind::None],
                2,
                Z180::<B>::execute_cb_rotate_shift,
            );
        }
        opcode += 1;
    }

    while opcode < 0x80 {
        table[opcode] = Opcode::implemented(
            "BIT {bit},{r}",
            [OperandKind::Bit, OperandKind::Reg8Source],
            2,
            Z180::<B>::execute_cb_bit,
        );
        opcode += 1;
    }

    while opcode < 0xc0 {
        table[opcode] = Opcode::implemented(
            "RES {bit},{r}",
            [OperandKind::Bit, OperandKind::Reg8Destination],
            2,
            Z180::<B>::execute_cb_res,
        );
        opcode += 1;
    }

    while opcode < 0x100 {
        table[opcode] = Opcode::implemented(
            "SET {bit},{r}",
            [OperandKind::Bit, OperandKind::Reg8Destination],
            2,
            Z180::<B>::execute_cb_set,
        );
        opcode += 1;
    }

    table
}

const fn build_index_table<B: HostBus, const IY: bool>() -> [Opcode<B>; 256] {
    let mut table = [Opcode::UNIMPLEMENTED; 256];

    let mut pair = 0_usize;
    while pair < 4 {
        table[0x09 + pair * 0x10] = Opcode::implemented(
            "ADD {index},{rr}",
            [OperandKind::Reg16Index, OperandKind::Reg16],
            2,
            Z180::<B>::execute_index::<IY>,
        );
        pair += 1;
    }

    table[0x21] = Opcode::implemented(
        "LD {index},{nn}",
        [OperandKind::Reg16Index, OperandKind::Immediate16],
        4,
        Z180::<B>::execute_index::<IY>,
    );
    table[0x22] = Opcode::implemented(
        "LD ({nn}),{index}",
        [OperandKind::IndirectImmediate16, OperandKind::Reg16Index],
        4,
        Z180::<B>::execute_index::<IY>,
    );
    table[0x23] = Opcode::implemented(
        "INC {index}",
        [OperandKind::Reg16Index, OperandKind::None],
        2,
        Z180::<B>::execute_index::<IY>,
    );
    table[0x2a] = Opcode::implemented(
        "LD {index},({nn})",
        [OperandKind::Reg16Index, OperandKind::IndirectImmediate16],
        4,
        Z180::<B>::execute_index::<IY>,
    );
    table[0x2b] = Opcode::implemented(
        "DEC {index}",
        [OperandKind::Reg16Index, OperandKind::None],
        2,
        Z180::<B>::execute_index::<IY>,
    );
    table[0x34] = Opcode::implemented(
        "INC ({index}+{d})",
        [OperandKind::IndirectIndex, OperandKind::None],
        3,
        Z180::<B>::execute_index::<IY>,
    );
    table[0x35] = Opcode::implemented(
        "DEC ({index}+{d})",
        [OperandKind::IndirectIndex, OperandKind::None],
        3,
        Z180::<B>::execute_index::<IY>,
    );
    table[0x36] = Opcode::implemented(
        "LD ({index}+{d}),{n}",
        [OperandKind::IndirectIndex, OperandKind::Immediate8],
        4,
        Z180::<B>::execute_index::<IY>,
    );

    let mut register = 0_usize;
    while register < 8 {
        if register != 6 {
            table[0x46 + register * 8] = Opcode::implemented(
                "LD {r},({index}+{d})",
                [OperandKind::Reg8Destination, OperandKind::IndirectIndex],
                3,
                Z180::<B>::execute_index::<IY>,
            );
            table[0x70 + register] = Opcode::implemented(
                "LD ({index}+{d}),{r}",
                [OperandKind::IndirectIndex, OperandKind::Reg8Source],
                3,
                Z180::<B>::execute_index::<IY>,
            );
        }
        register += 1;
    }

    let mut operation = 0_usize;
    while operation < 8 {
        table[0x86 + operation * 8] = Opcode::implemented(
            "{alu} A,({index}+{d})",
            [OperandKind::Accumulator, OperandKind::IndirectIndex],
            3,
            Z180::<B>::execute_index::<IY>,
        );
        operation += 1;
    }

    table[0xe1] = Opcode::implemented(
        "POP {index}",
        [OperandKind::Reg16Index, OperandKind::None],
        2,
        Z180::<B>::execute_index::<IY>,
    );
    table[0xe3] = Opcode::implemented(
        "EX (SP),{index}",
        [OperandKind::IndirectSp, OperandKind::Reg16Index],
        2,
        Z180::<B>::execute_index::<IY>,
    );
    table[0xe5] = Opcode::implemented(
        "PUSH {index}",
        [OperandKind::Reg16Index, OperandKind::None],
        2,
        Z180::<B>::execute_index::<IY>,
    );
    table[0xe9] = Opcode::implemented(
        "JP ({index})",
        [OperandKind::Reg16Index, OperandKind::None],
        2,
        Z180::<B>::execute_index::<IY>,
    );
    table[0xf9] = Opcode::implemented(
        "LD SP,{index}",
        [OperandKind::Reg16Sp, OperandKind::Reg16Index],
        2,
        Z180::<B>::execute_index::<IY>,
    );

    table
}

impl<B: HostBus> Z180<B> {
    pub(crate) const MAIN_OPCODES: [Opcode<B>; 256] = build_main_table::<B>();
    pub(crate) const CB_OPCODES: [Opcode<B>; 256] = build_cb_table::<B>();
    pub(crate) const DD_OPCODES: [Opcode<B>; 256] = build_index_table::<B, false>();
    pub(crate) const FD_OPCODES: [Opcode<B>; 256] = build_index_table::<B, true>();
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
    fn main_table_contains_every_documented_unprefixed_handler() {
        let table = &Z180::<NullBus>::MAIN_OPCODES;
        for (opcode, entry) in table.iter().enumerate() {
            let expected = !matches!(opcode, 0xcb | 0xdd | 0xed | 0xfd);
            assert_eq!(entry.handler.is_some(), expected, "opcode {opcode:02x}");
        }
    }

    #[test]
    fn main_metadata_is_defined_once_in_the_table() {
        let table = &Z180::<NullBus>::MAIN_OPCODES;
        assert_eq!(table[0x00].mnemonic, "NOP");
        assert_eq!(table[0x00].length, 1);
        assert_eq!(table[0x00].cycles, None);
        assert_eq!(table[0x76].mnemonic, "HALT");
        assert_eq!(table[0x78].mnemonic, "LD {dst},{src}");
        assert_eq!(table[0x01].length, 3);
        assert_eq!(table[0x06].length, 2);
        assert_eq!(table[0xc3].length, 3);
        assert_eq!(table[0xcb].length, 0);
        assert_eq!(
            table[0x78].operands,
            [OperandKind::Reg8Destination, OperandKind::Reg8Source]
        );
    }

    #[test]
    fn cb_table_contains_every_documented_form_except_sll() {
        let table = &Z180::<NullBus>::CB_OPCODES;
        for (opcode, entry) in table.iter().enumerate() {
            let expected = !(0x30..=0x37).contains(&opcode);
            assert_eq!(entry.handler.is_some(), expected, "CB {opcode:02x}");
            if expected {
                assert_eq!(entry.length, 2, "CB {opcode:02x}");
            }
        }
        assert_eq!(table[0x00].mnemonic, "RLC {r}");
        assert_eq!(table[0x38].mnemonic, "SRL {r}");
        assert_eq!(table[0x40].mnemonic, "BIT {bit},{r}");
        assert_eq!(table[0x80].mnemonic, "RES {bit},{r}");
        assert_eq!(table[0xc0].mnemonic, "SET {bit},{r}");
    }

    #[test]
    fn index_tables_contain_only_table_48_substitutions() {
        for table in [&Z180::<NullBus>::DD_OPCODES, &Z180::<NullBus>::FD_OPCODES] {
            for (opcode, entry) in table.iter().enumerate() {
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
                assert_eq!(entry.handler.is_some(), expected, "{opcode:02x}");
            }
        }
    }
}
