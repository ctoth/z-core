use std::{fmt::Write as _, fs, path::PathBuf};

use anyhow::{Context, Result};
use clap::Args;
use z180_core::disassemble_one;

#[derive(Debug, Args)]
pub struct DisArgs {
    /// Raw binary file to disassemble.
    pub file: PathBuf,
    /// Logical address assigned to the first byte (decimal or 0x-prefixed hex).
    #[arg(long, default_value = "0x0000", value_parser = parse_u16)]
    pub org: u16,
}

pub fn run(args: DisArgs) -> Result<()> {
    let bytes = fs::read(&args.file)
        .with_context(|| format!("failed to read binary file {}", args.file.display()))?;
    print!("{}", render(&bytes, args.org));
    Ok(())
}

pub(crate) fn render(bytes: &[u8], origin: u16) -> String {
    let mut output = String::new();
    let mut offset = 0_usize;
    while offset < bytes.len() {
        let address = origin.wrapping_add(offset as u16);
        let Some(instruction) = disassemble_one(&bytes[offset..], address) else {
            break;
        };
        let len = usize::from(instruction.len).max(1);
        let mut encoded = String::new();
        for (index, byte) in instruction.bytes[..len].iter().enumerate() {
            if index != 0 {
                encoded.push(' ');
            }
            write!(encoded, "{byte:02X}").expect("writing to a String cannot fail");
        }
        writeln!(
            output,
            "{:04X}  {encoded:<11} {}",
            instruction.address, instruction.text
        )
        .expect("writing to a String cannot fail");
        offset += len;
    }
    output
}

fn parse_u16(value: &str) -> Result<u16, String> {
    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u16::from_str_radix(hex, 16).map_err(|error| error.to_string())
    } else {
        value.parse::<u16>().map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, fs};

    use super::*;

    const EVERY_MNEMONIC: &[u8] = &[
        0x88, 0x80, 0xa0, 0xcb, 0x40, 0xcd, 0x34, 0x12, 0x3f, 0xb8, 0xed, 0xa9, 0xed, 0xb9, 0xed,
        0xa1, 0xed, 0xb1, 0x2f, 0x27, 0x05, 0xf3, 0x10, 0xfe, 0xfb, 0x08, 0xd9, 0x76, 0xed, 0x46,
        0xdb, 0x12, 0xed, 0x00, 0x12, 0x04, 0xed, 0xaa, 0xed, 0xba, 0xed, 0xa2, 0xed, 0xb2, 0xc3,
        0x34, 0x12, 0x18, 0xfe, 0x3e, 0x42, 0xed, 0xa8, 0xed, 0xb8, 0xed, 0xa0, 0xed, 0xb0, 0xed,
        0x4c, 0xed, 0x44, 0x00, 0xb0, 0xed, 0x8b, 0xed, 0x9b, 0xed, 0xbb, 0xed, 0x83, 0xed, 0x93,
        0xed, 0xb3, 0xd3, 0x12, 0xed, 0x01, 0x12, 0xed, 0xab, 0xed, 0xa3, 0xc1, 0xc5, 0xcb, 0x80,
        0xc9, 0xed, 0x4d, 0xed, 0x45, 0xcb, 0x10, 0x17, 0xcb, 0x00, 0x07, 0xed, 0x6f, 0xcb, 0x18,
        0x1f, 0xcb, 0x08, 0x0f, 0xed, 0x67, 0xc7, 0x98, 0x37, 0xcb, 0xc0, 0xcb, 0x20, 0xed, 0x76,
        0xcb, 0x28, 0xcb, 0x38, 0x90, 0xed, 0x04, 0xed, 0x74, 0x12, 0xa8,
    ];

    const MNEMONICS: &[&str] = &[
        "ADC", "ADD", "AND", "BIT", "CALL", "CCF", "CP", "CPD", "CPDR", "CPI", "CPIR", "CPL",
        "DAA", "DEC", "DI", "DJNZ", "EI", "EX", "EXX", "HALT", "IM", "IN", "IN0", "INC", "IND",
        "INDR", "INI", "INIR", "JP", "JR", "LD", "LDD", "LDDR", "LDI", "LDIR", "MLT", "NEG", "NOP",
        "OR", "OTDM", "OTDMR", "OTDR", "OTIM", "OTIMR", "OTIR", "OUT", "OUT0", "OUTD", "OUTI",
        "POP", "PUSH", "RES", "RET", "RETI", "RETN", "RL", "RLA", "RLC", "RLCA", "RLD", "RR",
        "RRA", "RRC", "RRCA", "RRD", "RST", "SBC", "SCF", "SET", "SLA", "SLP", "SRA", "SRL", "SUB",
        "TST", "TSTIO", "XOR",
    ];

    #[test]
    fn disassembler_golden_covers_every_mnemonic_once() {
        let rendered = render(EVERY_MNEMONIC, 0x4000);
        let actual_mnemonics: Vec<&str> = rendered
            .lines()
            .map(|line| {
                line.get(18..)
                    .and_then(|text| text.split_whitespace().next())
                    .expect("every rendered line has an instruction mnemonic")
            })
            .collect();
        assert_eq!(actual_mnemonics, MNEMONICS);
        assert_eq!(
            actual_mnemonics
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
                .len(),
            MNEMONICS.len(),
            "the crafted binary must contain each mnemonic exactly once"
        );

        let golden_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/dis_every_mnemonic.golden"
        );
        let binary_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/dis_every_mnemonic.bin"
        );
        if std::env::var_os("UPDATE_DIS_GOLDEN").is_some() {
            fs::write(binary_path, EVERY_MNEMONIC).expect("binary fixture must be writable");
            fs::write(golden_path, &rendered).expect("golden file must be writable");
            return;
        }
        assert_eq!(
            include_bytes!("../tests/fixtures/dis_every_mnemonic.bin"),
            EVERY_MNEMONIC
        );
        assert_eq!(
            rendered,
            include_str!("../tests/fixtures/dis_every_mnemonic.golden")
        );
    }

    #[test]
    fn origin_parser_accepts_decimal_and_prefixed_hex() {
        assert_eq!(parse_u16("4660"), Ok(0x1234));
        assert_eq!(parse_u16("0x1234"), Ok(0x1234));
        assert!(parse_u16("0x10000").is_err());
    }
}
