use alloc::{format, string::String, string::ToString};
use core::fmt::Write as _;

use crate::{
    HostBus, Z180,
    optable::{Opcode, OperandKind},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisassembledInstruction {
    pub address: u16,
    pub bytes: [u8; 4],
    pub len: u8,
    pub text: String,
}

struct DisassemblyBus;

impl HostBus for DisassemblyBus {
    fn mem_read(&mut self, _phys: u32) -> u8 {
        0xff
    }

    fn mem_write(&mut self, _phys: u32, _value: u8) {}

    fn io_read(&mut self, _port: u16) -> u8 {
        0xff
    }

    fn io_write(&mut self, _port: u16, _value: u8) {}
}

#[derive(Clone, Copy)]
enum OpcodePage {
    Main,
    Cb,
    Ed,
    Index { iy: bool },
    IndexCb { iy: bool },
}

#[must_use]
pub fn disassemble_one(bytes: &[u8], address: u16) -> Option<DisassembledInstruction> {
    let first = *bytes.first()?;
    let decoded = decode(bytes);
    let Some((descriptor, opcode, page, encoded_len)) = decoded else {
        return Some(data_byte(address, first));
    };

    if descriptor.handler.is_none()
        || descriptor.length == 0
        || bytes.len() < usize::from(descriptor.length)
    {
        return Some(data_bytes(address, &bytes[..encoded_len.min(bytes.len())]));
    }

    let len = descriptor.length;
    let mut encoded = [0; 4];
    encoded[..usize::from(len)].copy_from_slice(&bytes[..usize::from(len)]);
    Some(DisassembledInstruction {
        address,
        bytes: encoded,
        len,
        text: format_instruction(descriptor, opcode, page, encoded, address),
    })
}

fn decode(bytes: &[u8]) -> Option<(Opcode<DisassemblyBus>, u8, OpcodePage, usize)> {
    let first = *bytes.first()?;
    match first {
        0xcb => {
            let opcode = *bytes.get(1)?;
            Some((
                Z180::<DisassemblyBus>::CB_OPCODES[usize::from(opcode)],
                opcode,
                OpcodePage::Cb,
                2,
            ))
        }
        0xed => {
            let opcode = *bytes.get(1)?;
            Some((
                Z180::<DisassemblyBus>::ED_OPCODES[usize::from(opcode)],
                opcode,
                OpcodePage::Ed,
                2,
            ))
        }
        0xdd | 0xfd => {
            let iy = first == 0xfd;
            let second = *bytes.get(1)?;
            if second == 0xcb {
                let opcode = *bytes.get(3)?;
                let descriptor = if iy {
                    Z180::<DisassemblyBus>::FDCB_OPCODES[usize::from(opcode)]
                } else {
                    Z180::<DisassemblyBus>::DDCB_OPCODES[usize::from(opcode)]
                };
                Some((descriptor, opcode, OpcodePage::IndexCb { iy }, 4))
            } else {
                let descriptor = if iy {
                    Z180::<DisassemblyBus>::FD_OPCODES[usize::from(second)]
                } else {
                    Z180::<DisassemblyBus>::DD_OPCODES[usize::from(second)]
                };
                Some((descriptor, second, OpcodePage::Index { iy }, 2))
            }
        }
        opcode => Some((
            Z180::<DisassemblyBus>::MAIN_OPCODES[usize::from(opcode)],
            opcode,
            OpcodePage::Main,
            1,
        )),
    }
}

fn format_instruction(
    descriptor: Opcode<DisassemblyBus>,
    opcode: u8,
    page: OpcodePage,
    bytes: [u8; 4],
    address: u16,
) -> String {
    const REG8: [&str; 8] = ["B", "C", "D", "E", "H", "L", "(HL)", "A"];
    const REG16: [&str; 4] = ["BC", "DE", "HL", "SP"];
    const STACK: [&str; 4] = ["BC", "DE", "HL", "AF"];
    const CONDITIONS: [&str; 8] = ["NZ", "Z", "NC", "C", "PO", "PE", "P", "M"];
    const JR_CONDITIONS: [&str; 4] = ["NZ", "Z", "NC", "C"];
    const ALU: [&str; 8] = ["ADD A", "ADC A", "SUB", "SBC A", "AND", "XOR", "OR", "CP"];

    let mut text = descriptor.mnemonic.to_string();
    let destination = REG8[usize::from((opcode >> 3) & 0x07)];
    let source = REG8[usize::from(opcode & 0x07)];
    let register_pair = REG16[usize::from((opcode >> 4) & 0x03)];
    let stack_pair = STACK[usize::from((opcode >> 4) & 0x03)];
    let register = match page {
        OpcodePage::Cb => source,
        OpcodePage::Index { .. } if has_operand(descriptor.operands, OperandKind::Reg8Source) => {
            source
        }
        OpcodePage::Main
        | OpcodePage::Ed
        | OpcodePage::Index { .. }
        | OpcodePage::IndexCb { .. } => destination,
    };
    let condition =
        if matches!(page, OpcodePage::Main) && matches!(opcode, 0x20 | 0x28 | 0x30 | 0x38) {
            JR_CONDITIONS[usize::from((opcode >> 3) & 0x03)]
        } else {
            CONDITIONS[usize::from((opcode >> 3) & 0x07)]
        };
    let index = match page {
        OpcodePage::Index { iy: true } | OpcodePage::IndexCb { iy: true } => "IY",
        OpcodePage::Index { iy: false } | OpcodePage::IndexCb { iy: false } => "IX",
        OpcodePage::Main | OpcodePage::Cb | OpcodePage::Ed => "",
    };

    text = text.replace("{dst}", destination);
    text = text.replace("{src}", source);
    text = text.replace("{g}", destination);
    text = text.replace("{r}", register);
    text = text.replace("{rr}", register_pair);
    text = text.replace("{qq}", stack_pair);
    text = text.replace("{cc}", condition);
    text = text.replace("{bit}", &((opcode >> 3) & 0x07).to_string());
    text = text.replace("{vector}", &format!("{:02X}h", opcode & 0x38));
    text = text.replace("{index}", index);
    text = text.replace("{alu} A", ALU[usize::from((opcode >> 3) & 0x07)]);

    if has_operand(descriptor.operands, OperandKind::IndirectIndex) {
        let displacement = bytes[2].cast_signed();
        let rendered = if displacement < 0 {
            format!("-{:02X}h", displacement.unsigned_abs())
        } else {
            format!("+{:02X}h", displacement.cast_unsigned())
        };
        text = text.replace("+{d}", &rendered);
    }
    if has_operand(descriptor.operands, OperandKind::Relative8) {
        let displacement = bytes[usize::from(descriptor.length - 1)].cast_signed();
        let target = address
            .wrapping_add(u16::from(descriptor.length))
            .wrapping_add_signed(i16::from(displacement));
        text = text.replace("{rel}", &format!("{target:04X}h"));
    }
    if has_operand(descriptor.operands, OperandKind::Immediate16)
        || has_operand(descriptor.operands, OperandKind::IndirectImmediate16)
    {
        let start = usize::from(descriptor.length - 2);
        let value = u16::from_le_bytes([bytes[start], bytes[start + 1]]);
        text = text.replace("{nn}", &format!("{value:04X}h"));
    }
    if has_operand(descriptor.operands, OperandKind::Immediate8)
        || has_operand(descriptor.operands, OperandKind::PortImmediate)
    {
        let value = bytes[usize::from(descriptor.length - 1)];
        text = text.replace("{n}", &format!("{value:02X}h"));
    }
    text
}

fn has_operand(operands: [OperandKind; 2], wanted: OperandKind) -> bool {
    operands[0] == wanted || operands[1] == wanted
}

fn data_byte(address: u16, byte: u8) -> DisassembledInstruction {
    data_bytes(address, &[byte])
}

fn data_bytes(address: u16, source: &[u8]) -> DisassembledInstruction {
    let len = source.len().min(4);
    let mut bytes = [0; 4];
    bytes[..len].copy_from_slice(&source[..len]);
    let mut text = String::from("DB ");
    for (index, byte) in source[..len].iter().enumerate() {
        if index != 0 {
            text.push(',');
        }
        let _ = write!(text, "{byte:02X}h");
    }
    DisassembledInstruction {
        address,
        bytes,
        len: len as u8,
        text,
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn disassembler_formats_immediates_indexes_relative_targets_and_unknowns() {
        assert_eq!(
            disassemble_one(&[0x3e, 0x42], 0x1000)
                .expect("LD must decode")
                .text,
            "LD A,42h"
        );
        assert_eq!(
            disassemble_one(&[0x18, 0xfe], 0x1000)
                .expect("JR must decode")
                .text,
            "JR 1000h"
        );
        assert_eq!(
            disassemble_one(&[0xdd, 0x21, 0x34, 0x12], 0)
                .expect("indexed LD must decode")
                .text,
            "LD IX,1234h"
        );
        assert_eq!(
            disassemble_one(&[0xfd, 0xcb, 0xfe, 0x46], 0)
                .expect("indexed BIT must decode")
                .text,
            "BIT 0,(IY-02h)"
        );
        assert_eq!(
            disassemble_one(&[0xed, 0x4c], 0)
                .expect("MLT must decode")
                .text,
            "MLT BC"
        );
        assert_eq!(
            disassemble_one(&[0xed, 0x31], 0).expect("undefined ED must tile"),
            DisassembledInstruction {
                address: 0,
                bytes: [0xed, 0x31, 0, 0],
                len: 2,
                text: String::from("DB EDh,31h"),
            }
        );
        assert_eq!(
            disassemble_one(&[0xdd, 0x21, 0x34], 0)
                .expect("truncated indexed LD must tile")
                .len,
            2
        );
    }

    #[test]
    fn every_implemented_optable_entry_formats_without_placeholders() {
        for opcode in 0_u8..=u8::MAX {
            assert_complete_if_implemented([opcode, 0x34, 0x12, 0x00], OpcodePage::Main, opcode);
            assert_complete_if_implemented([0xcb, opcode, 0x00, 0x00], OpcodePage::Cb, opcode);
            assert_complete_if_implemented([0xed, opcode, 0x34, 0x12], OpcodePage::Ed, opcode);
            assert_complete_if_implemented(
                [0xdd, opcode, 0x34, 0x12],
                OpcodePage::Index { iy: false },
                opcode,
            );
            assert_complete_if_implemented(
                [0xfd, opcode, 0x34, 0x12],
                OpcodePage::Index { iy: true },
                opcode,
            );
            assert_complete_if_implemented(
                [0xdd, 0xcb, 0x05, opcode],
                OpcodePage::IndexCb { iy: false },
                opcode,
            );
            assert_complete_if_implemented(
                [0xfd, 0xcb, 0x05, opcode],
                OpcodePage::IndexCb { iy: true },
                opcode,
            );
        }
    }

    fn assert_complete_if_implemented(bytes: [u8; 4], page: OpcodePage, opcode: u8) {
        let descriptor = match page {
            OpcodePage::Main => Z180::<DisassemblyBus>::MAIN_OPCODES[usize::from(opcode)],
            OpcodePage::Cb => Z180::<DisassemblyBus>::CB_OPCODES[usize::from(opcode)],
            OpcodePage::Ed => Z180::<DisassemblyBus>::ED_OPCODES[usize::from(opcode)],
            OpcodePage::Index { iy: false } => {
                Z180::<DisassemblyBus>::DD_OPCODES[usize::from(opcode)]
            }
            OpcodePage::Index { iy: true } => {
                Z180::<DisassemblyBus>::FD_OPCODES[usize::from(opcode)]
            }
            OpcodePage::IndexCb { iy: false } => {
                Z180::<DisassemblyBus>::DDCB_OPCODES[usize::from(opcode)]
            }
            OpcodePage::IndexCb { iy: true } => {
                Z180::<DisassemblyBus>::FDCB_OPCODES[usize::from(opcode)]
            }
        };
        if descriptor.handler.is_none() {
            return;
        }
        let decoded = disassemble_one(&bytes, 0).expect("four bytes always provide one record");
        assert_eq!(decoded.len, descriptor.length, "{bytes:02X?}");
        assert!(
            !decoded.text.contains('{'),
            "{bytes:02X?}: {}",
            decoded.text
        );
        assert!(!decoded.text.is_empty(), "{bytes:02X?}");
    }

    proptest! {
        #[test]
        fn disassembly_is_total_and_lengths_tile_the_input(
            bytes in prop::collection::vec(any::<u8>(), 0..1024),
            origin in any::<u16>(),
        ) {
            let mut offset = 0_usize;
            while offset < bytes.len() {
                let decoded = disassemble_one(
                    &bytes[offset..],
                    origin.wrapping_add(offset as u16),
                ).expect("a nonempty slice always produces one record");
                let len = usize::from(decoded.len);
                prop_assert!(len >= 1);
                prop_assert!(len <= 4);
                prop_assert!(offset + len <= bytes.len());
                offset += len;
            }
            prop_assert_eq!(offset, bytes.len());
        }
    }
}
