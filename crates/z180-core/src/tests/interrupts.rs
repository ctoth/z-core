use super::*;

#[test]
fn undefined_second_opcode_takes_trap() {
    for opcodes in [[0xcb, 0x30], [0xdd, 0x24], [0xed, 0x31], [0xfd, 0x24]] {
        let mut cpu = machine();
        cpu.mem_poke(0x1234, opcodes[0]);
        cpu.mem_poke(0x1235, opcodes[1]);
        cpu.set_reg(Reg::PC, 0x1234);
        cpu.set_reg(Reg::SP, 0x8000);
        cpu.set_reg(Reg::IR, 0x56fe);
        cpu.set_iff1(true);
        cpu.set_iff2(false);

        assert_eq!(cpu.step(), 29, "{opcodes:02x?}");
        assert_eq!(cpu.instruction_pc(), 0x1234, "{opcodes:02x?}");
        assert_eq!(cpu.reg(Reg::PC), 0, "{opcodes:02x?}");
        assert_eq!(cpu.reg(Reg::SP), 0x7ffe, "{opcodes:02x?}");
        assert_eq!(cpu.mem_peek(0x7ffe), 0x35, "{opcodes:02x?}");
        assert_eq!(cpu.mem_peek(0x7fff), 0x12, "{opcodes:02x?}");
        assert_eq!(cpu.reg(Reg::IR), 0x5680, "{opcodes:02x?}");
        assert_eq!(cpu.itc(), 0x81, "{opcodes:02x?}");
        assert!(cpu.iff1(), "{opcodes:02x?}");
        assert!(!cpu.iff2(), "{opcodes:02x?}");
        assert_eq!(cpu.cycle_count(), 29, "{opcodes:02x?}");
        assert_eq!(
            cpu.drain_events(),
            vec![Event::Trap {
                cycle: 0,
                pc: 0x1234,
                opcode: [opcodes[0], opcodes[1], 0],
                len: 2,
            }],
            "{opcodes:02x?}"
        );
        assert!(cpu.drain_events().is_empty(), "{opcodes:02x?}");
    }
}

#[test]
fn undefined_third_opcode_takes_trap_with_ufo() {
    for prefix in [0xdd, 0xfd] {
        let mut cpu = machine();
        cpu.mem_poke(0x1234, prefix);
        cpu.mem_poke(0x1235, 0xcb);
        cpu.mem_poke(0x1236, 0x05);
        cpu.mem_poke(0x1237, 0x40);
        cpu.set_reg(Reg::PC, 0x1234);
        cpu.set_reg(Reg::SP, 0x8000);
        cpu.set_reg(Reg::IR, 0x567e);
        cpu.set_iff1(false);
        cpu.set_iff2(true);

        assert_eq!(cpu.step(), 44, "{prefix:02x}");
        assert_eq!(cpu.instruction_pc(), 0x1234, "{prefix:02x}");
        assert_eq!(cpu.reg(Reg::PC), 0, "{prefix:02x}");
        assert_eq!(cpu.reg(Reg::SP), 0x7ffe, "{prefix:02x}");
        assert_eq!(cpu.mem_peek(0x7ffe), 0x36, "{prefix:02x}");
        assert_eq!(cpu.mem_peek(0x7fff), 0x12, "{prefix:02x}");
        assert_eq!(cpu.reg(Reg::IR), 0x5601, "{prefix:02x}");
        assert_eq!(cpu.itc(), 0xc1, "{prefix:02x}");
        assert!(!cpu.iff1(), "{prefix:02x}");
        assert!(cpu.iff2(), "{prefix:02x}");
        assert_eq!(cpu.cycle_count(), 44, "{prefix:02x}");
        assert_eq!(
            cpu.drain_events(),
            vec![Event::Trap {
                cycle: 0,
                pc: 0x1234,
                opcode: [prefix, 0xcb, 0x40],
                len: 3,
            }],
            "{prefix:02x}"
        );
    }
}

#[test]
fn ei_shadow_lasts_through_exactly_the_following_instruction() {
    let mut cpu = machine();
    cpu.mem_poke(0, 0xfb);
    cpu.mem_poke(1, 0x00);
    cpu.mem_poke(2, 0xfb);
    cpu.mem_poke(3, 0xfb);
    cpu.mem_poke(4, 0xf3);

    assert_eq!(cpu.step(), 6);
    assert!(cpu.iff1());
    assert!(cpu.iff2());
    assert!(cpu.ei_shadow);

    assert_eq!(cpu.step(), 6);
    assert!(!cpu.ei_shadow);

    cpu.step();
    assert!(cpu.ei_shadow);
    cpu.step();
    assert!(cpu.ei_shadow);

    cpu.step();
    assert!(!cpu.iff1());
    assert!(!cpu.iff2());
    assert!(!cpu.ei_shadow);
}

#[test]
fn interrupts_ei_shadow_defers_maskable_service_for_one_instruction() {
    let mut cpu = machine();
    cpu.write_internal_io(DCNTL, 0x00);
    cpu.mem_poke(0, 0xfb);
    cpu.mem_poke(1, 0x00);
    cpu.mem_poke(2, 0x00);
    cpu.set_reg(Reg::SP, 0x8000);
    cpu.set_irq(IrqLine::Int0, true);

    assert_eq!(cpu.step(), 3);
    assert!(cpu.ei_shadow);
    assert_eq!(cpu.step(), 3);
    assert_eq!(cpu.reg(Reg::PC), 2);
    assert!(!cpu.ei_shadow);
    assert_eq!(cpu.step(), 13);
    assert_eq!(cpu.reg(Reg::PC), 0x0038);
    assert_eq!(cpu.reg(Reg::SP), 0x7ffe);
    assert_eq!(cpu.mem_peek(0x7ffe), 0x02);
    assert_eq!(cpu.mem_peek(0x7fff), 0x00);
}

#[test]
fn interrupts_nmi_is_edge_latched_and_preserves_iff1_in_iff2() {
    let mut cpu = machine();
    cpu.write_internal_io(DCNTL, 0x00);
    cpu.mem_poke(0x1234, 0x00);
    cpu.mem_poke(0x0066, 0x00);
    cpu.set_reg(Reg::PC, 0x1234);
    cpu.set_reg(Reg::SP, 0x8000);
    cpu.set_reg(Reg::IR, 0x5600);
    cpu.set_iff1(true);
    cpu.set_iff2(false);

    cpu.set_nmi(true);
    assert_eq!(cpu.step(), 11);
    assert_eq!(cpu.reg(Reg::PC), 0x0066);
    assert_eq!(cpu.reg(Reg::SP), 0x7ffe);
    assert_eq!(cpu.mem_peek(0x7ffe), 0x34);
    assert_eq!(cpu.mem_peek(0x7fff), 0x12);
    assert!(!cpu.iff1());
    assert!(cpu.iff2());
    assert_eq!(cpu.reg(Reg::IR), 0x5601);

    cpu.set_nmi(true);
    assert_eq!(cpu.step(), 3);
    assert_eq!(cpu.reg(Reg::PC), 0x0067);

    cpu.set_nmi(false);
    cpu.set_nmi(true);
    assert_eq!(cpu.step(), 11);
    assert_eq!(cpu.reg(Reg::PC), 0x0066);
    assert_eq!(cpu.reg(Reg::SP), 0x7ffc);
}

#[test]
fn interrupts_int0_modes_use_fixed_ff_acknowledge_data() {
    for (mode, expected_cycles, expected_pc) in [(0, 13, 0x0038), (1, 11, 0x0038), (2, 18, 0x5678)]
    {
        let mut cpu = machine();
        cpu.write_internal_io(DCNTL, 0x00);
        cpu.mem_poke(0x12ff, 0x78);
        cpu.mem_poke(0x1300, 0x56);
        cpu.set_reg(Reg::PC, 0x3456);
        cpu.set_reg(Reg::SP, 0x8000);
        cpu.set_reg(Reg::IR, 0x1200);
        cpu.set_interrupt_mode(mode);
        cpu.set_iff1(true);
        cpu.set_iff2(true);
        cpu.set_irq(IrqLine::Int0, true);

        assert_eq!(cpu.step(), expected_cycles, "IM{mode}");
        assert_eq!(cpu.reg(Reg::PC), expected_pc, "IM{mode}");
        assert_eq!(cpu.reg(Reg::SP), 0x7ffe, "IM{mode}");
        assert_eq!(cpu.mem_peek(0x7ffe), 0x56, "IM{mode}");
        assert_eq!(cpu.mem_peek(0x7fff), 0x34, "IM{mode}");
        assert!(!cpu.iff1(), "IM{mode}");
        assert!(!cpu.iff2(), "IM{mode}");
        assert_eq!(cpu.reg(Reg::IR), 0x1201, "IM{mode}");
    }
}

#[test]
fn interrupts_vector_through_i_il_in_um0050_priority_order() {
    let mut external = machine();
    external.write_internal_io(DCNTL, 0x00);
    external.write_internal_io(IL, 0xa0);
    external.write_internal_io(ITC, 0x07);
    external.mem_poke(0x20a0, 0x11);
    external.mem_poke(0x20a1, 0x11);
    external.mem_poke(0x20a2, 0x22);
    external.mem_poke(0x20a3, 0x22);
    external.set_reg(Reg::IR, 0x2000);
    external.set_reg(Reg::SP, 0x8000);
    external.set_iff1(true);
    external.set_irq(IrqLine::Int1, true);
    external.set_irq(IrqLine::Int2, true);

    assert_eq!(external.step(), 18);
    assert_eq!(external.reg(Reg::PC), 0x1111, "INT1 precedes INT2");

    let mut internal = machine();
    internal.write_internal_io(DCNTL, 0x00);
    internal.write_internal_io(IL, 0xa0);
    internal.mem_poke(0x20a6, 0x33);
    internal.mem_poke(0x20a7, 0x33);
    internal.mem_poke(0x20a8, 0x44);
    internal.mem_poke(0x20a9, 0x44);
    internal.set_reg(Reg::IR, 0x2000);
    internal.set_reg(Reg::SP, 0x8000);
    internal.set_iff1(true);
    internal.internal_irq_pending = 0x02 | 0x04;

    assert_eq!(internal.step(), 18);
    assert_eq!(internal.reg(Reg::PC), 0x3333, "PRT1 precedes DMA0");
}

#[test]
fn peripheral_interrupts_vector_in_every_adjacent_priority_pair() {
    for (pair, name, higher_bit, lower_bit, higher_code, lower_code) in [
        (0_u8, "PRT0 > PRT1", 0x01_u8, 0x02_u8, 0x04_u8, 0x06_u8),
        (1, "PRT1 > DMA0", 0x02, 0x04, 0x06, 0x08),
        (2, "DMA0 > DMA1", 0x04, 0x08, 0x08, 0x0a),
        (3, "DMA1 > CSI/O", 0x08, 0x10, 0x0a, 0x0c),
        (4, "CSI/O > ASCI0", 0x10, 0x20, 0x0c, 0x0e),
        (5, "ASCI0 > ASCI1", 0x20, 0x40, 0x0e, 0x10),
    ] {
        let mut cpu = machine();
        cpu.write_internal_io(DCNTL, 0x00);
        cpu.write_internal_io(IL, 0xa0);
        cpu.set_reg(Reg::IR, 0x2000);
        cpu.set_reg(Reg::SP, 0x8000);

        let higher_target = 0x4000_u16 | (u16::from(pair) << 8) | u16::from(higher_code);
        let lower_target = 0x5000_u16 | (u16::from(pair) << 8) | u16::from(lower_code);
        let higher_vector = 0x20a0_u32 | u32::from(higher_code);
        let lower_vector = 0x20a0_u32 | u32::from(lower_code);
        cpu.mem_poke(higher_vector, higher_target as u8);
        cpu.mem_poke(higher_vector + 1, (higher_target >> 8) as u8);
        cpu.mem_poke(lower_vector, lower_target as u8);
        cpu.mem_poke(lower_vector + 1, (lower_target >> 8) as u8);

        match pair {
            0 => {
                cpu.write_internal_io(TMDR0L, 0x01);
                cpu.write_internal_io(TMDR0H, 0x00);
                cpu.write_internal_io(TMDR1L, 0x01);
                cpu.write_internal_io(TMDR1H, 0x00);
                cpu.write_internal_io(TCR, 0x33);
                cpu.finish_step(20);
            }
            1 => {
                cpu.write_internal_io(TMDR1L, 0x01);
                cpu.write_internal_io(TMDR1H, 0x00);
                cpu.write_internal_io(TCR, 0x22);
                cpu.finish_step(20);
                cpu.write_internal_io(DSTAT, 0x34);
            }
            2 => cpu.write_internal_io(DSTAT, 0x3c),
            3 => {
                cpu.write_internal_io(DSTAT, 0x38);
                cpu.write_internal_io(TRD, 0xa5);
                cpu.write_internal_io(CNTR, 0x50);
                cpu.finish_step(160);
            }
            4 => {
                cpu.write_internal_io(TRD, 0xa5);
                cpu.write_internal_io(CNTR, 0x50);
                cpu.finish_step(160);
                cpu.write_internal_io(STAT0, 0x01);
            }
            5 => {
                cpu.write_internal_io(STAT0, 0x01);
                cpu.write_internal_io(STAT1, 0x01);
            }
            _ => unreachable!(),
        }

        assert_eq!(
            cpu.internal_irq_pending & (higher_bit | lower_bit),
            higher_bit | lower_bit,
            "{name} real requests"
        );
        cpu.set_iff1(true);
        assert_eq!(cpu.step(), 18, "{name} higher acknowledge");
        assert_eq!(cpu.reg(Reg::PC), higher_target, "{name} higher vector");

        match pair {
            0 => {
                assert_eq!(cpu.read_internal_io(TCR) & 0xc0, 0xc0);
                let _ = cpu.read_internal_io(TMDR0L);
            }
            1 => {
                assert_eq!(cpu.read_internal_io(TCR) & 0x80, 0x80);
                let _ = cpu.read_internal_io(TMDR1L);
            }
            2 => cpu.write_internal_io(DSTAT, 0x38),
            3 => cpu.write_internal_io(DSTAT, 0x30),
            4 => {
                let _ = cpu.read_internal_io(TRD);
            }
            5 => cpu.write_internal_io(STAT0, 0x00),
            _ => unreachable!(),
        }

        assert_eq!(
            cpu.internal_irq_pending & (higher_bit | lower_bit),
            lower_bit,
            "{name} lower remains after real higher-source clear"
        );
        cpu.set_iff1(true);
        assert_eq!(cpu.step(), 18, "{name} lower acknowledge");
        assert_eq!(cpu.reg(Reg::PC), lower_target, "{name} lower vector");
    }
}

#[test]
fn interrupts_vector_dispatch_matrix_covers_every_source_gate_and_iff_state() {
    let sources = [
        (IrqSource::Nmi, None, 0x00, 0x00, None),
        (IrqSource::Int0, Some(IrqLine::Int0), 0x00, 0x01, None),
        (
            IrqSource::Int1,
            Some(IrqLine::Int1),
            0x00,
            0x02,
            Some(0x00_u8),
        ),
        (IrqSource::Int2, Some(IrqLine::Int2), 0x00, 0x04, Some(0x02)),
        (IrqSource::Prt0, None, 0x01, 0x00, Some(0x04)),
        (IrqSource::Prt1, None, 0x02, 0x00, Some(0x06)),
        (IrqSource::Dma0, None, 0x04, 0x00, Some(0x08)),
        (IrqSource::Dma1, None, 0x08, 0x00, Some(0x0a)),
        (IrqSource::Csio, None, 0x10, 0x00, Some(0x0c)),
        (IrqSource::Asci0, None, 0x20, 0x00, Some(0x0e)),
        (IrqSource::Asci1, None, 0x40, 0x00, Some(0x10)),
    ];

    for (source, external_line, internal_bit, itc_bit, fixed_code) in sources {
        for enabled in [false, true] {
            for iff1 in [false, true] {
                let mut cpu = machine();
                cpu.write_internal_io(DCNTL, 0x00);
                cpu.write_internal_io(IL, 0xa0);
                cpu.write_internal_io(ITC, if enabled { itc_bit } else { 0 });
                cpu.mem_poke(0x0100, 0x00);
                cpu.set_reg(Reg::PC, 0x0100);
                cpu.set_reg(Reg::SP, 0x8000);
                cpu.set_reg(Reg::IR, 0x2000);
                cpu.set_interrupt_mode(1);
                cpu.set_iff1(iff1);
                cpu.set_iff2(true);

                let expected_pc = match source {
                    IrqSource::Nmi => 0x0066,
                    IrqSource::Int0 => 0x0038,
                    _ => {
                        let code = fixed_code.expect("vectored source must have a fixed code");
                        let vector = 0x20a0 | u16::from(code);
                        let target = 0x4000 | u16::from(code);
                        cpu.mem_poke(u32::from(vector), target as u8);
                        cpu.mem_poke(u32::from(vector.wrapping_add(1)), (target >> 8) as u8);
                        target
                    }
                };

                if source == IrqSource::Nmi {
                    cpu.set_nmi(enabled);
                } else if let Some(line) = external_line {
                    cpu.set_irq(line, true);
                } else {
                    cpu.internal_irq_pending = if enabled { internal_bit } else { 0 };
                }

                let should_service = enabled && (source == IrqSource::Nmi || iff1);
                let cycles = cpu.step();
                if should_service {
                    let expected_cycles = match source {
                        IrqSource::Nmi | IrqSource::Int0 => 11,
                        _ => 18,
                    };
                    assert_eq!(
                        cycles, expected_cycles,
                        "{source:?} enabled={enabled} iff1={iff1}"
                    );
                    assert_eq!(cpu.reg(Reg::PC), expected_pc, "{source:?}");
                    assert_eq!(cpu.reg(Reg::SP), 0x7ffe, "{source:?}");
                    assert_eq!(cpu.mem_peek(0x7ffe), 0x00, "{source:?}");
                    assert_eq!(cpu.mem_peek(0x7fff), 0x01, "{source:?}");
                    assert!(!cpu.iff1(), "{source:?}");
                    if source == IrqSource::Nmi {
                        assert_eq!(cpu.iff2(), iff1, "{source:?}");
                    } else {
                        assert!(!cpu.iff2(), "{source:?}");
                    }
                } else {
                    assert_eq!(cycles, 3, "{source:?} enabled={enabled} iff1={iff1}");
                    assert_eq!(cpu.reg(Reg::PC), 0x0101, "{source:?}");
                    assert_eq!(cpu.reg(Reg::SP), 0x8000, "{source:?}");
                    assert_eq!(cpu.iff1(), iff1, "{source:?}");
                    assert!(cpu.iff2(), "{source:?}");
                }
            }
        }
    }
}

#[test]
fn interrupts_halt_and_sleep_follow_distinct_wake_rules() {
    let mut halted = machine();
    halted.write_internal_io(DCNTL, 0x00);
    halted.mem_poke(0, 0x76);
    halted.set_reg(Reg::SP, 0x8000);
    halted.set_iff1(true);
    assert_eq!(halted.step(), 3);
    assert!(halted.halted());
    halted.set_irq(IrqLine::Int0, true);
    assert_eq!(halted.step(), 13);
    assert!(!halted.halted());
    assert_eq!(halted.reg(Reg::PC), 0x0038);

    let mut sleeping = machine();
    sleeping.write_internal_io(DCNTL, 0x00);
    sleeping.mem_poke(0, 0xed);
    sleeping.mem_poke(1, 0x76);
    sleeping.mem_poke(2, 0x00);
    assert_eq!(sleeping.step(), 8);
    assert!(sleeping.sleeping());
    sleeping.set_irq(IrqLine::Int1, true);
    assert_eq!(
        sleeping.step(),
        3,
        "disabled INT1 is ignored while sleep time advances"
    );
    assert!(sleeping.sleeping());

    sleeping.write_internal_io(ITC, 0x03);
    assert_eq!(sleeping.step(), 3, "enabled INT1 wakes with IEF1 clear");
    assert!(!sleeping.sleeping());
    assert_eq!(sleeping.reg(Reg::PC), 3);

    let mut serviced = machine();
    serviced.write_internal_io(DCNTL, 0x00);
    serviced.write_internal_io(ITC, 0x03);
    serviced.mem_poke(0, 0xed);
    serviced.mem_poke(1, 0x76);
    serviced.mem_poke(0x2000, 0x56);
    serviced.mem_poke(0x2001, 0x34);
    serviced.set_reg(Reg::IR, 0x2000);
    serviced.set_reg(Reg::SP, 0x8000);
    serviced.set_iff1(true);
    assert_eq!(serviced.step(), 8);
    serviced.set_irq(IrqLine::Int1, true);
    assert_eq!(serviced.step(), 18);
    assert!(!serviced.sleeping());
    assert_eq!(serviced.reg(Reg::PC), 0x3456);
}

#[test]
fn reti_preserves_iffs_while_retn_restores_iff1() {
    let mut interrupt_return = machine();
    interrupt_return.mem_poke(0, 0xed);
    interrupt_return.mem_poke(1, 0x4d);
    interrupt_return.mem_poke(0x2000, 0x34);
    interrupt_return.mem_poke(0x2001, 0x12);
    interrupt_return.set_reg(Reg::SP, 0x2000);
    interrupt_return.set_iff1(false);
    interrupt_return.set_iff2(true);

    assert_eq!(interrupt_return.step(), 34);

    assert_eq!(interrupt_return.reg(Reg::PC), 0x1234);
    assert_eq!(interrupt_return.reg(Reg::SP), 0x2002);
    assert!(!interrupt_return.iff1());
    assert!(interrupt_return.iff2());

    let mut nmi_return = machine();
    nmi_return.mem_poke(0, 0xed);
    nmi_return.mem_poke(1, 0x45);
    nmi_return.mem_poke(0x2000, 0x78);
    nmi_return.mem_poke(0x2001, 0x56);
    nmi_return.set_reg(Reg::SP, 0x2000);
    nmi_return.set_iff1(false);
    nmi_return.set_iff2(true);

    assert_eq!(nmi_return.step(), 24);

    assert_eq!(nmi_return.reg(Reg::PC), 0x5678);
    assert_eq!(nmi_return.reg(Reg::SP), 0x2002);
    assert!(nmi_return.iff1());
    assert!(nmi_return.iff2());

    let config = MachineConfig {
        variant: Variant::Z8S180,
        regions: vec![RegionDef {
            base: 0,
            size: 0x1_0000,
            kind: RegionKind::Ram,
        }],
        ..MachineConfig::default()
    };
    let mut z8s180_reti =
        Z180::new(config, NullBus).expect("flat Z8S180 RAM configuration must be valid");
    z8s180_reti.mem_poke(0, 0xed);
    z8s180_reti.mem_poke(1, 0x4d);
    z8s180_reti.mem_poke(0x2000, 0x34);
    z8s180_reti.mem_poke(0x2001, 0x12);
    z8s180_reti.set_reg(Reg::SP, 0x2000);
    assert_eq!(z8s180_reti.step(), 24);
}
