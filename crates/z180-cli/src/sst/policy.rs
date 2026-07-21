use std::collections::BTreeSet;

use anyhow::{Context, Result, bail};

#[derive(Debug)]
pub(crate) struct OnlyFilter {
    stems: Option<BTreeSet<String>>,
}

impl OnlyFilter {
    pub(crate) fn parse(value: Option<&str>) -> Result<Self> {
        let Some(value) = value else {
            return Ok(Self { stems: None });
        };

        let mut stems = BTreeSet::new();
        for raw_token in value.split(',') {
            let token = raw_token.trim().to_ascii_lowercase();
            if token.is_empty() {
                bail!("--only contains an empty item");
            }
            if let Some((start, end)) = token.split_once("..") {
                let start = parse_main_opcode(start)
                    .with_context(|| format!("invalid --only range start {start:?}"))?;
                let end = parse_main_opcode(end)
                    .with_context(|| format!("invalid --only range end {end:?}"))?;
                if start > end {
                    bail!("--only range {token:?} runs backward");
                }
                for opcode in start..=end {
                    stems.insert(format!("{opcode:02x}"));
                }
            } else {
                stems.insert(token);
            }
        }

        Ok(Self { stems: Some(stems) })
    }

    pub(crate) fn matches(&self, stem: &str) -> bool {
        self.stems.as_ref().is_none_or(|stems| stems.contains(stem))
    }
}

pub(crate) fn opcode_bytes(stem: &str) -> Result<Vec<u8>> {
    stem.split_ascii_whitespace()
        .filter(|part| *part != "__")
        .map(|part| {
            if part.len() != 2 {
                bail!("invalid opcode component {part:?} in {stem:?}");
            }
            u8::from_str_radix(part, 16)
                .with_context(|| format!("invalid opcode component {part:?} in {stem:?}"))
        })
        .collect()
}

pub(crate) fn exclusion_reason(opcodes: &[u8]) -> Option<&'static str> {
    match opcodes {
        [_main] => None,
        [0xcb, opcode] if (0x30..=0x37).contains(opcode) => {
            Some("CB SLL is undefined on Z80180 (UM0050 Table 49)")
        }
        [0xcb, _opcode] => None,
        [0xed, 0x4c | 0x4d | 0x5c | 0x6c | 0x7c | 0x64 | 0x74 | 0x76] => {
            Some("standard Z80 transition contradicts the defined Z80180 instruction")
        }
        [0xed, opcode] if !defined_ed(*opcode) => {
            Some("blank ED-map cell is undefined on Z80180 (UM0050 Table 50)")
        }
        [0xed, _opcode] => None,
        [prefix @ (0xdd | 0xfd), 0xcb, opcode] if !defined_indexed_cb(*opcode) => {
            if *prefix == 0xdd {
                Some("undocumented DDCB result form is undefined on Z80180")
            } else {
                Some("undocumented FDCB result form is undefined on Z80180")
            }
        }
        [0xdd | 0xfd, 0xcb, _opcode] => None,
        [0xdd | 0xfd, opcode] if !defined_index(*opcode) => {
            Some("DD/FD opcode has no documented HL/(HL) substitution")
        }
        [0xdd | 0xfd, _opcode] => None,
        [0xdd | 0xfd, ..] => Some("undefined DD/FD prefix sequence"),
        _ => None,
    }
}

fn parse_main_opcode(value: &str) -> Result<u8> {
    if value.len() != 2 {
        bail!("main opcode must contain exactly two hexadecimal digits");
    }
    u8::from_str_radix(value, 16).context("main opcode is not hexadecimal")
}

const fn defined_index(opcode: u8) -> bool {
    matches!(
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
            | 0x71
            | 0x72
            | 0x73
            | 0x74
            | 0x75
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
    )
}

const fn defined_indexed_cb(opcode: u8) -> bool {
    opcode & 0x07 == 0x06 && !(opcode >= 0x30 && opcode <= 0x37)
}

const fn defined_ed(opcode: u8) -> bool {
    matches!(
        opcode,
        0x00 | 0x01
            | 0x04
            | 0x08
            | 0x09
            | 0x0c
            | 0x10
            | 0x11
            | 0x14
            | 0x18
            | 0x19
            | 0x1c
            | 0x20
            | 0x21
            | 0x24
            | 0x28
            | 0x29
            | 0x2c
            | 0x30
            | 0x34
            | 0x38
            | 0x39
            | 0x3c
            | 0x40
            | 0x41
            | 0x42
            | 0x43
            | 0x44
            | 0x45
            | 0x46
            | 0x47
            | 0x48
            | 0x49
            | 0x4a
            | 0x4b
            | 0x4c
            | 0x4d
            | 0x4f
            | 0x50
            | 0x51
            | 0x52
            | 0x53
            | 0x56
            | 0x57
            | 0x58
            | 0x59
            | 0x5a
            | 0x5b
            | 0x5c
            | 0x5e
            | 0x5f
            | 0x60
            | 0x61
            | 0x62
            | 0x63
            | 0x64
            | 0x67
            | 0x68
            | 0x69
            | 0x6a
            | 0x6b
            | 0x6c
            | 0x6f
            | 0x72
            | 0x73
            | 0x74
            | 0x76
            | 0x78
            | 0x79
            | 0x7a
            | 0x7b
            | 0x7c
            | 0x83
            | 0x8b
            | 0x93
            | 0x9b
            | 0xa0
            | 0xa1
            | 0xa2
            | 0xa3
            | 0xa8
            | 0xa9
            | 0xaa
            | 0xab
            | 0xb0
            | 0xb1
            | 0xb2
            | 0xb3
            | 0xb8
            | 0xb9
            | 0xba
            | 0xbb
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_filter_expands_inclusive_main_opcode_ranges() {
        let filter = OnlyFilter::parse(Some("00,76,40..7f")).expect("valid filter");
        assert!(filter.matches("00"));
        assert!(filter.matches("40"));
        assert!(filter.matches("7f"));
        assert!(!filter.matches("80"));
        assert!(!filter.matches("cb 40"));
    }

    #[test]
    fn opcode_parser_skips_displacement_placeholder() {
        assert_eq!(
            opcode_bytes("dd cb __ 46").expect("valid opcode stem"),
            [0xdd, 0xcb, 0x46]
        );
    }

    #[test]
    fn appendix_a_policy_excludes_known_undefined_families() {
        assert!(exclusion_reason(&[0xcb, 0x30]).is_some());
        assert!(exclusion_reason(&[0xcb, 0x2f]).is_none());
        assert!(exclusion_reason(&[0xed, 0x02]).is_some());
        assert!(exclusion_reason(&[0xed, 0x30]).is_none());
        assert!(exclusion_reason(&[0xed, 0x31]).is_some());
        assert!(exclusion_reason(&[0xdd, 0x24]).is_some());
        assert!(exclusion_reason(&[0xdd, 0x21]).is_none());
        assert!(exclusion_reason(&[0xfd, 0xeb]).is_some());
        assert!(exclusion_reason(&[0xdd, 0xcb, 0x46]).is_none());
        assert!(exclusion_reason(&[0xdd, 0xcb, 0x40]).is_some());
        assert!(exclusion_reason(&[0xfd, 0xcb, 0x36]).is_some());
    }

    #[test]
    fn appendix_a_policy_excludes_incompatible_ed_transitions() {
        for opcode in [0x4c, 0x4d, 0x5c, 0x6c, 0x7c, 0x64, 0x74, 0x76] {
            assert!(exclusion_reason(&[0xed, opcode]).is_some());
        }
        assert!(exclusion_reason(&[0xed, 0x44]).is_none());
    }
}
