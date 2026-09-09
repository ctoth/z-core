use super::*;

#[test]
fn comparison_masks_xy_flags() {
    let (mut cpu, _) = machine(Vec::new(), 0x1_0000).expect("valid test machine");
    cpu.set_reg(Reg::AF, pair(0x12, 0x28));
    cpu.set_reg(Reg::AF2, pair(0x34, 0x28));
    cpu.set_reg(Reg::IR, pair(0x56, 0x00));
    let case = standard_case(
        "masked fields",
        TestState {
            a: 0x12,
            af2: pair(0x34, 0x00),
            i: 0x56,
            ..zero_state()
        },
    );

    assert!(compare(&cpu, &case, false).is_none());
}

#[test]
fn comparison_reports_r_high_bit() {
    let (mut cpu, _) = machine(Vec::new(), 0x1_0000).expect("valid test machine");
    cpu.set_reg(Reg::IR, pair(0, 0x80));
    let case = standard_case("R bit 7", zero_state());

    let failure = compare(&cpu, &case, false).expect("R bit 7 must be compared");
    assert_eq!(failure.field, "r");
    assert_eq!(failure.expected, "00");
    assert_eq!(failure.actual, "80");
}

#[test]
fn comparison_reports_the_first_differing_field() {
    let (cpu, _) = machine(Vec::new(), 0x1_0000).expect("valid test machine");
    let case = standard_case(
        "wrong pc and sp",
        TestState {
            pc: 1,
            sp: 2,
            ..zero_state()
        },
    );

    let failure = compare(&cpu, &case, false).expect("comparison must fail");
    assert_eq!(failure.field, "pc");
    assert_eq!(failure.expected, "0001");
    assert_eq!(failure.actual, "0000");
}

#[test]
fn comparison_uses_the_generated_case_flag_mask() {
    let (mut cpu, _) = machine(Vec::new(), 0x1_0000).expect("valid test machine");
    cpu.set_reg(Reg::AF, pair(0, 0x80));
    cpu.set_reg(Reg::AF2, pair(0, 0x80));
    let mut case = standard_case("masked documented flags", zero_state());
    case.flags_mask = Some(0x42);

    assert!(compare(&cpu, &case, false).is_none());
}

#[test]
fn sabotage_reverses_only_ld_operands() {
    assert_eq!(reversed_ld_opcode(0x41), Some(0x48));
    assert_eq!(reversed_ld_opcode(0x70), Some(0x46));
    assert_eq!(reversed_ld_opcode(0x40), Some(0x40));
    assert_eq!(reversed_ld_opcode(0x76), None);
    assert_eq!(reversed_ld_opcode(0x00), None);
}

#[test]
fn scripted_bus_supplies_reads_and_records_writes_in_order() {
    let expected = vec![
        PortEvent(0x0040, 0x12, PortDirection::R),
        PortEvent(0x0041, 0x34, PortDirection::W),
    ];
    let script = Rc::new(RefCell::new(PortScript {
        expected,
        observed: Vec::new(),
    }));
    let mut bus = ScriptedBus {
        script: Rc::clone(&script),
    };

    assert_eq!(bus.io_read(0x0040), Ok(0x12));
    bus.io_write(0x0041, 0x34);
    assert!(compare_ports(&script.borrow(), "ports").is_none());
}

#[test]
fn standard_sst_relocates_internal_window_away_from_expected_external_port() {
    let mut initial = zero_state();
    initial.c = 0x13;
    initial.ram = vec![[0x0000, 0xed], [0x0001, 0x48]];
    let mut final_state = zero_state();
    final_state.c = 0xd5;
    final_state.f = 0x80;
    final_state.r = 2;
    final_state.pc = 2;
    final_state.ram = initial.ram.clone();
    let mut case = standard_case("IN C,(C) at low external port", final_state);
    case.initial = initial;
    case.ports = Some(vec![PortEvent(0x0013, 0xd5, PortDirection::R)]);

    let report = run_file("ed 48".to_owned(), vec![case], false, false)
        .expect("standard SST case must execute");
    assert_eq!(report.pass, 1);
    assert_eq!(report.fail, 0);
    assert_eq!(report.unimplemented, 0);
}

#[test]
fn generated_schema_dispatches_instruction_and_validates_mmu_pages() {
    let instruction = generated_case(CaseKind::Instruction, None);
    assert_eq!(
        validate_generated_cases("ed00", &[instruction]).expect("valid instruction schema"),
        Some(CaseKind::Instruction)
    );

    let probes = (0_u16..16)
        .map(|page| MmuProbe {
            logical: page << 12,
            expected_physical: u32::from(page) << 12,
            value: page as u8,
        })
        .collect();
    let mmu = generated_case(CaseKind::Mmu, Some(probes));
    assert_eq!(
        validate_generated_cases("mmu", &[mmu]).expect("valid MMU schema"),
        Some(CaseKind::Mmu)
    );
}

#[test]
fn generated_instruction_requires_reset_z180_state() {
    let mut instruction = generated_case(CaseKind::Instruction, None);
    instruction
        .initial
        .z180
        .as_mut()
        .expect("generated z180 state")
        .itc = 0x81;

    let error = validate_generated_cases("ed00", &[instruction])
        .expect_err("non-reset initial z180 state must be rejected");
    assert!(
        error
            .to_string()
            .contains("initial.z180 must equal reset state")
    );
}

#[test]
fn generated_comparison_includes_itc_and_sleeping() {
    let (cpu, _) = machine(Vec::new(), 0x1_0000).expect("valid test machine");
    let mut case = generated_case(CaseKind::Instruction, None);
    let expected_z180 = case
        .final_state
        .z180
        .as_mut()
        .expect("generated z180 state");
    expected_z180.itc = 0x81;
    expected_z180.sleeping = true;

    let failure = compare(&cpu, &case, false).expect("ITC mismatch must fail");
    assert_eq!(failure.field, "z180.itc");
}

#[test]
fn standard_sst_rejects_a_write_omitted_from_final_ram() {
    let mut initial = zero_state();
    initial.a = 0x5a;
    initial.h = 0x20;
    initial.ram = vec![[0x0000, 0x77]];
    let mut final_state = zero_state();
    final_state.a = 0x5a;
    final_state.h = 0x20;
    final_state.ram = vec![[0x0000, 0x77]];
    final_state.pc = 1;
    final_state.r = 1;
    let mut case = standard_case("stray write", final_state);
    case.initial = initial;

    let report = run_file("stray-write".to_owned(), vec![case], false, false)
        .expect("standard SST case must execute");
    assert_eq!(report.pass, 0);
    assert_eq!(report.fail, 1);
    assert_eq!(report.failures[0].field, "ram[2000]");
}

#[test]
fn generated_mmu_executes_register_program_and_all_logical_reads() {
    let probes = (0_u16..16)
        .map(|page| {
            let logical = (page << 12) | 0x0123;
            let relocation = if page < 4 {
                0_u32
            } else if page < 10 {
                0x40
            } else {
                0x80
            };
            MmuProbe {
                logical,
                expected_physical: ((relocation + u32::from(page)) & 0xff) << 12 | 0x0123,
                value: page as u8 ^ 0xa5,
            }
        })
        .collect();
    let mut case = generated_case(CaseKind::Mmu, Some(probes));
    let final_z180 = case
        .final_state
        .z180
        .as_mut()
        .expect("generated Z180 state");
    final_z180.cbr = 0x80;
    final_z180.bbr = 0x40;
    final_z180.cbar = 0xa4;

    let report = run_mmu_file("mmu".to_owned(), vec![case], false)
        .expect("generated MMU runner must complete");
    assert_eq!(report.pass, 1);
    assert_eq!(report.fail, 0);
    assert_eq!(report.unimplemented, 0);
}

fn standard_case(name: &str, final_state: TestState) -> TestCase {
    TestCase {
        name: name.to_owned(),
        kind: None,
        seed: None,
        flags_mask: None,
        disputed: None,
        dispute_note: None,
        ports: None,
        mmu_probes: None,
        initial: zero_state(),
        final_state,
        extra: BTreeMap::new(),
    }
}

fn generated_case(kind: CaseKind, mmu_probes: Option<Vec<MmuProbe>>) -> TestCase {
    TestCase {
        name: "generated".to_owned(),
        kind: Some(kind),
        seed: Some(1),
        flags_mask: Some(0xd7),
        disputed: Some(false),
        dispute_note: Some(String::new()),
        ports: Some(Vec::new()),
        mmu_probes,
        initial: generated_state(),
        final_state: generated_state(),
        extra: BTreeMap::new(),
    }
}

fn generated_state() -> TestState {
    TestState {
        z180: Some(Z180State {
            itc: 1,
            cbr: 0,
            bbr: 0,
            cbar: 0xf0,
            sleeping: false,
        }),
        ..zero_state()
    }
}

fn zero_state() -> TestState {
    TestState {
        pc: 0,
        sp: 0,
        a: 0,
        b: 0,
        c: 0,
        d: 0,
        e: 0,
        f: 0,
        h: 0,
        l: 0,
        i: 0,
        r: 0,
        ix: 0,
        iy: 0,
        af2: 0,
        bc2: 0,
        de2: 0,
        hl2: 0,
        iff1: 0,
        iff2: 0,
        im: 0,
        ram: Vec::new(),
        z180: None,
        extra: BTreeMap::new(),
    }
}
