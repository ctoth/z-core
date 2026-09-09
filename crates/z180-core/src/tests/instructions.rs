use super::*;

#[test]
fn stack_push_writes_high_byte_before_low_byte() {
    let mut cpu = machine();
    cpu.mem_poke(0, 0xc5);
    cpu.set_reg(Reg::BC, 0x1234);
    cpu.set_reg(Reg::SP, 0x2000);
    let _watch = cpu.add_mem_watch(0, 0x1_0000, WatchKind::Write);

    assert_ne!(cpu.step(), 0);
    assert_eq!(
        cpu.drain_events(),
        vec![
            Event::MemWrite {
                cycle: 0,
                pc: 0,
                phys: 0x1fff,
                val: 0x12,
            },
            Event::MemWrite {
                cycle: 0,
                pc: 0,
                phys: 0x1ffe,
                val: 0x34,
            },
        ]
    );
}

#[test]
fn indexed_bit_fetches_displacement_before_sub_opcode_without_duplicate_reads() {
    for (prefix, index) in [(0xdd, Reg::IX), (0xfd, Reg::IY)] {
        let mut cpu = machine();
        for (address, byte) in [prefix, 0xcb, 0x01, 0x46].into_iter().enumerate() {
            cpu.mem_poke(address as u32, byte);
        }
        cpu.mem_poke(0x1001, 0xa5);
        cpu.set_reg(index, 0x1000);
        let _watch = cpu.add_mem_watch(0, 0x1_0000, WatchKind::Read);

        assert_ne!(cpu.step(), 0, "prefix {prefix:02x}");
        let reads = cpu
            .drain_events()
            .into_iter()
            .map(|event| match event {
                Event::MemRead { phys, .. } => phys,
                other => panic!("unexpected event for prefix {prefix:02x}: {other:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(reads, [0, 1, 2, 3, 0x1001], "prefix {prefix:02x}");
    }
}

#[test]
fn implemented_set_is_every_documented_unprefixed_opcode() {
    for opcode in 0_u8..=u8::MAX {
        let expected = !matches!(opcode, 0xcb | 0xdd | 0xed | 0xfd);
        assert_eq!(
            Z180::<NullBus>::is_instruction_implemented(&[opcode]),
            expected,
            "opcode {opcode:02x}"
        );
    }

    for opcode in 0_u8..=u8::MAX {
        let expected = !(0x30..=0x37).contains(&opcode);
        assert_eq!(
            Z180::<NullBus>::is_instruction_implemented(&[0xcb, opcode]),
            expected,
            "CB {opcode:02x}"
        );
    }

    for prefix in [0xdd, 0xfd] {
        for opcode in 0_u8..=u8::MAX {
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
            assert_eq!(
                Z180::<NullBus>::is_instruction_implemented(&[prefix, opcode]),
                expected,
                "{prefix:02x} {opcode:02x}"
            );
        }
    }

    for prefix in [0xdd, 0xfd] {
        for opcode in 0_u8..=u8::MAX {
            let expected = opcode & 0x07 == 6 && !(0x30..=0x37).contains(&opcode);
            assert_eq!(
                Z180::<NullBus>::is_instruction_implemented(&[prefix, 0xcb, opcode]),
                expected,
                "{prefix:02x} cb __ {opcode:02x}"
            );
        }
    }
}

#[test]
fn nop_advances_pc_and_r_without_changing_registers() {
    let mut cpu = machine();
    cpu.mem_poke(0x1234, 0x00);
    cpu.set_reg(Reg::PC, 0x1234);
    cpu.set_reg(Reg::AF, 0x56d7);
    cpu.set_reg(Reg::BC, 0x89ab);
    cpu.set_reg(Reg::IR, 0x34ff);

    assert_eq!(cpu.step(), 6);

    assert_eq!(cpu.instruction_pc(), 0x1234);
    assert_eq!(cpu.reg(Reg::PC), 0x1235);
    assert_eq!(cpu.reg(Reg::AF), 0x56d7);
    assert_eq!(cpu.reg(Reg::BC), 0x89ab);
    assert_eq!(cpu.reg(Reg::IR), 0x3480);
    assert_eq!(cpu.cycle_count(), 6);
    assert!(!cpu.halted());
}

#[test]
fn halt_enters_halted_state_and_leaves_flags_unchanged() {
    let mut cpu = machine();
    cpu.mem_poke(0, 0x76);
    cpu.set_reg(Reg::AF, 0x12a5);

    assert_eq!(cpu.step(), 6);
    assert_eq!(cpu.step(), 6);

    assert!(cpu.halted());
    assert_eq!(cpu.reg(Reg::PC), 1);
    assert_eq!(cpu.reg(Reg::AF), 0x12a5);
}

#[test]
fn ld_register_to_register_preserves_flags() {
    let mut cpu = machine();
    cpu.mem_poke(0, 0x78);
    cpu.set_reg(Reg::AF, 0x11d5);
    cpu.set_reg(Reg::BC, 0x42ee);

    cpu.step();

    assert_eq!(cpu.reg(Reg::AF), 0x42d5);
}

#[test]
fn ld_reads_and_writes_through_hl() {
    let mut cpu = machine();
    cpu.mem_poke(0, 0x46);
    cpu.mem_poke(1, 0x70);
    cpu.mem_poke(0x2000, 0x5a);
    cpu.set_reg(Reg::HL, 0x2000);

    cpu.step();
    assert_eq!(cpu.reg(Reg::BC).to_be_bytes()[0], 0x5a);

    cpu.set_reg(Reg::BC, 0xa5ff);
    cpu.step();
    assert_eq!(cpu.mem_peek(0x2000), 0xa5);
}

#[test]
fn opcode_76_is_halt_not_ld_hl_hl() {
    let mut cpu = machine();
    cpu.mem_poke(0, 0x76);
    cpu.mem_poke(0x2222, 0x7c);
    cpu.set_reg(Reg::HL, 0x2222);

    cpu.step();

    assert!(cpu.halted());
    assert_eq!(cpu.mem_peek(0x2222), 0x7c);
}

#[test]
fn reset_preserves_memory_and_clears_r_and_halt() {
    let mut cpu = machine();
    cpu.mem_poke(0, 0x76);
    cpu.set_reg(Reg::IR, 0xab7f);
    cpu.set_iff1(true);
    cpu.set_iff2(true);
    cpu.set_interrupt_mode(2);
    cpu.step();
    cpu.mem_poke(0x4000, 0xcc);

    cpu.reset();

    assert_eq!(cpu.reg(Reg::IR), 0);
    assert_eq!(cpu.reg(Reg::PC), 0);
    assert!(!cpu.halted());
    assert!(!cpu.iff1());
    assert!(!cpu.iff2());
    assert_eq!(cpu.interrupt_mode(), 0);
    assert_eq!(cpu.mem_peek(0x4000), 0xcc);
}

#[test]
fn z180_repeat_block_output_sets_terminal_flags() {
    let cases: [(u8, u16, u16, u16, u8, u8); 2] = [
        (0x93, 0x2000, 0x2001, 0x2002, 0x40, 0x42),
        (0x9b, 0x2001, 0x2000, 0x1fff, 0x42, 0x40),
    ];
    for (opcode, initial_hl, final_value_address, final_hl, initial_c, final_c) in cases {
        let mut cpu = machine();
        cpu.mem_poke(0, 0xed);
        cpu.mem_poke(1, opcode);
        cpu.mem_poke(initial_hl.into(), 0x01);
        cpu.mem_poke(final_value_address.into(), 0x80);
        cpu.set_reg(Reg::AF, 0x55ff);
        cpu.set_reg(Reg::BC, u16::from_be_bytes([2, initial_c]));
        cpu.set_reg(Reg::HL, initial_hl);

        assert_eq!(cpu.step(), 50, "ED {opcode:02x}");
        assert_eq!(cpu.reg(Reg::AF), 0x5546, "ED {opcode:02x}");
        assert_eq!(cpu.reg(Reg::BC), u16::from(final_c), "ED {opcode:02x}");
        assert_eq!(cpu.reg(Reg::HL), final_hl, "ED {opcode:02x}");
    }
}
