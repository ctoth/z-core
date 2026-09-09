use super::*;

#[allow(
    clippy::many_single_char_names,
    reason = "the one-letter locals are the literal Z180 register names compared against the SST schema"
)]
pub(super) fn compare(cpu: &Z180<ScriptedBus>, case: &TestCase, ignore_r: bool) -> Option<Failure> {
    let expected = &case.final_state;
    let flag_mask = case.flags_mask.unwrap_or(FLAG_COMPARE_MASK);
    let [a, f] = cpu.reg(Reg::AF).to_be_bytes();
    let [b, c] = cpu.reg(Reg::BC).to_be_bytes();
    let [d, e] = cpu.reg(Reg::DE).to_be_bytes();
    let [h, l] = cpu.reg(Reg::HL).to_be_bytes();
    let [i, r] = cpu.reg(Reg::IR).to_be_bytes();

    let checks = [
        difference_u16("pc", expected.pc, cpu.reg(Reg::PC)),
        difference_u16("sp", expected.sp, cpu.reg(Reg::SP)),
        difference_u8("a", expected.a, a),
        difference_u8("b", expected.b, b),
        difference_u8("c", expected.c, c),
        difference_u8("d", expected.d, d),
        difference_u8("e", expected.e, e),
        difference_u8("f", expected.f & flag_mask, f & flag_mask),
        difference_u8("h", expected.h, h),
        difference_u8("l", expected.l, l),
        difference_u8("i", expected.i, i),
        (!ignore_r)
            .then(|| difference_u8("r", expected.r, r))
            .flatten(),
        difference_u16("ix", expected.ix, cpu.reg(Reg::IX)),
        difference_u16("iy", expected.iy, cpu.reg(Reg::IY)),
        difference_u16(
            "af_",
            mask_pair_flags(expected.af2, flag_mask),
            mask_pair_flags(cpu.reg(Reg::AF2), flag_mask),
        ),
        difference_u16("bc_", expected.bc2, cpu.reg(Reg::BC2)),
        difference_u16("de_", expected.de2, cpu.reg(Reg::DE2)),
        difference_u16("hl_", expected.hl2, cpu.reg(Reg::HL2)),
        difference_u8("iff1", expected.iff1, u8::from(cpu.iff1())),
        difference_u8("iff2", expected.iff2, u8::from(cpu.iff2())),
        difference_u8("im", expected.im, cpu.interrupt_mode()),
    ];

    if let Some(difference) = checks.into_iter().flatten().next() {
        return Some(Failure {
            test: case.name.clone(),
            field: difference.field.to_owned(),
            expected: difference.expected,
            actual: difference.actual,
        });
    }

    if let Some(expected_z180) = &expected.z180 {
        let z180_checks = [
            difference_u8("z180.itc", expected_z180.itc, cpu.itc()),
            difference_u8("z180.cbr", expected_z180.cbr, cpu.io_reg_peek(0x38)),
            difference_u8("z180.bbr", expected_z180.bbr, cpu.io_reg_peek(0x39)),
            difference_u8("z180.cbar", expected_z180.cbar, cpu.io_reg_peek(0x3a)),
            difference_u8(
                "z180.sleeping",
                u8::from(expected_z180.sleeping),
                u8::from(cpu.sleeping()),
            ),
        ];
        if let Some(difference) = z180_checks.into_iter().flatten().next() {
            return Some(Failure {
                test: case.name.clone(),
                field: difference.field.to_owned(),
                expected: difference.expected,
                actual: difference.actual,
            });
        }
    }

    for [address, expected_value] in &expected.ram {
        let actual = cpu.mem_peek(u32::from(*address));
        if u16::from(actual) != *expected_value {
            return Some(Failure {
                test: case.name.clone(),
                field: format!("ram[{address:04x}]"),
                expected: format!("{expected_value:02x}"),
                actual: format!("{actual:02x}"),
            });
        }
    }

    None
}

pub(super) fn compare_ports(script: &PortScript, test: &str) -> Option<Failure> {
    if script.expected == script.observed {
        return None;
    }
    let index = script
        .expected
        .iter()
        .zip(&script.observed)
        .position(|(expected, observed)| expected != observed)
        .unwrap_or_else(|| script.expected.len().min(script.observed.len()));
    Some(Failure {
        test: test.to_owned(),
        field: format!("ports[{index}]"),
        expected: script
            .expected
            .get(index)
            .map_or_else(|| "<end>".to_owned(), format_port_event),
        actual: script
            .observed
            .get(index)
            .map_or_else(|| "<end>".to_owned(), format_port_event),
    })
}

fn format_port_event(event: &PortEvent) -> String {
    let direction = match event.2 {
        PortDirection::R => 'r',
        PortDirection::W => 'w',
    };
    format!("{:04x}:{:02x}:{direction}", event.0, event.1)
}

struct Difference {
    field: &'static str,
    expected: String,
    actual: String,
}

fn difference_u8(field: &'static str, expected: u8, actual: u8) -> Option<Difference> {
    (expected != actual).then(|| Difference {
        field,
        expected: format!("{expected:02x}"),
        actual: format!("{actual:02x}"),
    })
}

fn difference_u16(field: &'static str, expected: u16, actual: u16) -> Option<Difference> {
    (expected != actual).then(|| Difference {
        field,
        expected: format!("{expected:04x}"),
        actual: format!("{actual:04x}"),
    })
}

pub(super) const fn mask_pair_flags(value: u16, flag_mask: u8) -> u16 {
    value & u16::from_be_bytes([0xff, flag_mask])
}
