use super::*;

#[test]
fn sleep_run_advances_time_until_prt_interrupt_wakes_the_cpu() {
    const PRT_PERIOD_CYCLES: u32 = 40;

    let mut cpu = machine();
    cpu.write_internal_io(DCNTL, 0x00);
    cpu.write_internal_io(IL, 0xa0);
    cpu.write_internal_io(TMDR0L, 0x01);
    cpu.write_internal_io(TMDR0H, 0x00);
    cpu.write_internal_io(RLDR0L, 0x01);
    cpu.write_internal_io(RLDR0H, 0x00);
    cpu.write_internal_io(TCR, 0x11);
    cpu.mem_poke(0, 0xed);
    cpu.mem_poke(1, 0x76);
    cpu.mem_poke(0x20a4, 0x56);
    cpu.mem_poke(0x20a5, 0x34);
    cpu.set_reg(Reg::IR, 0x2000);
    cpu.set_reg(Reg::SP, 0x8000);
    cpu.set_iff1(true);

    assert_eq!(cpu.step(), 8);
    assert!(cpu.sleeping());
    let before = cpu.cycle_count();

    let consumed = cpu.run(2 * PRT_PERIOD_CYCLES);

    assert!(consumed >= 2 * PRT_PERIOD_CYCLES);
    assert_eq!(cpu.cycle_count(), before + u64::from(consumed));
    assert!(!cpu.sleeping());
    assert_eq!(cpu.reg(Reg::SP), 0x7ffe, "PRT0 interrupt was taken");
    assert_eq!(cpu.mem_peek(0x7ffe), 0x02, "return PC is past SLP");
    assert_eq!(cpu.mem_peek(0x7fff), 0x00);
}

#[test]
fn sleep_run_consumes_budget_without_waking_when_interrupts_are_masked() {
    const BUDGET: u32 = 96;

    let mut cpu = machine();
    cpu.write_internal_io(DCNTL, 0x00);
    cpu.mem_poke(0, 0xed);
    cpu.mem_poke(1, 0x76);

    assert_eq!(cpu.step(), 8);
    assert!(cpu.sleeping());
    let before = cpu.cycle_count();

    assert_eq!(cpu.run(BUDGET), BUDGET);
    assert_eq!(cpu.cycle_count(), before + u64::from(BUDGET));
    assert!(cpu.sleeping());
    assert_eq!(cpu.reg(Reg::PC), 2);
}

#[test]
fn timing_selects_conditional_and_repeat_paths() {
    let mut call_not_taken = machine();
    call_not_taken.mem_poke(0, 0xc4);
    call_not_taken.mem_poke(1, 0x34);
    call_not_taken.mem_poke(2, 0x12);
    call_not_taken.set_reg(Reg::AF, u16::from(FLAG_Z));
    assert_eq!(call_not_taken.step(), 15);
    assert_eq!(call_not_taken.reg(Reg::PC), 3);

    let mut call_taken = machine();
    call_taken.mem_poke(0, 0xc4);
    call_taken.mem_poke(1, 0x34);
    call_taken.mem_poke(2, 0x12);
    call_taken.set_reg(Reg::SP, 0x2000);
    assert_eq!(call_taken.step(), 31);
    assert_eq!(call_taken.reg(Reg::PC), 0x1234);

    let mut ldir_terminal = machine();
    ldir_terminal.mem_poke(0, 0xed);
    ldir_terminal.mem_poke(1, 0xb0);
    ldir_terminal.mem_poke(0x1000, 0x5a);
    ldir_terminal.set_reg(Reg::BC, 1);
    ldir_terminal.set_reg(Reg::DE, 0x2000);
    ldir_terminal.set_reg(Reg::HL, 0x1000);
    assert_eq!(ldir_terminal.step(), 24);
    assert_eq!(ldir_terminal.reg(Reg::PC), 2);

    let mut ldir_repeating = machine();
    ldir_repeating.mem_poke(0, 0xed);
    ldir_repeating.mem_poke(1, 0xb0);
    ldir_repeating.mem_poke(0x1000, 0x5a);
    ldir_repeating.set_reg(Reg::BC, 2);
    ldir_repeating.set_reg(Reg::DE, 0x2000);
    ldir_repeating.set_reg(Reg::HL, 0x1000);
    assert_eq!(ldir_repeating.step(), 26);
    assert_eq!(ldir_repeating.reg(Reg::PC), 0);
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one numbered test preserves the complete hand-computed timing checklist from the plan"
)]
fn timing_spot_checks_hand_computed_program_totals() {
    let mut checked = 0_u8;

    // 1. Two NOPs: 2 * (3 base + 1 memory access * 3 waits) = 12.
    {
        let mut cpu = machine();
        cpu.mem_poke(0, 0x00);
        cpu.mem_poke(1, 0x00);
        cpu.step();
        cpu.step();
        assert_eq!(cpu.cycle_count(), 12, "NOP; NOP");
        checked += 1;
    }

    // 2. LD BC,nn; INC BC: (9 + 3*3) + (4 + 1*3) = 25.
    {
        let mut cpu = machine();
        cpu.mem_poke(0, 0x01);
        cpu.mem_poke(1, 0x34);
        cpu.mem_poke(2, 0x12);
        cpu.mem_poke(3, 0x03);
        cpu.step();
        cpu.step();
        assert_eq!(cpu.cycle_count(), 25, "LD BC,1234h; INC BC");
        checked += 1;
    }

    // 3. LD B,n; LD C,B: (6 + 2*3) + (4 + 1*3) = 19.
    {
        let mut cpu = machine();
        cpu.mem_poke(0, 0x06);
        cpu.mem_poke(1, 0x5a);
        cpu.mem_poke(2, 0x48);
        cpu.step();
        cpu.step();
        assert_eq!(cpu.cycle_count(), 19, "LD B,5Ah; LD C,B");
        checked += 1;
    }

    // 4. LD HL,nn; LD A,(HL): (9 + 3*3) + (6 + 2*3) = 30.
    {
        let mut cpu = machine();
        cpu.mem_poke(0, 0x21);
        cpu.mem_poke(1, 0x00);
        cpu.mem_poke(2, 0x20);
        cpu.mem_poke(3, 0x7e);
        cpu.mem_poke(0x2000, 0x5a);
        cpu.step();
        cpu.step();
        assert_eq!(cpu.cycle_count(), 30, "LD HL,2000h; LD A,(HL)");
        checked += 1;
    }

    // 5. LD HL,nn; LD (HL),n: (9 + 3*3) + (9 + 3*3) = 36.
    {
        let mut cpu = machine();
        cpu.mem_poke(0, 0x21);
        cpu.mem_poke(1, 0x00);
        cpu.mem_poke(2, 0x20);
        cpu.mem_poke(3, 0x36);
        cpu.mem_poke(4, 0xa5);
        cpu.step();
        cpu.step();
        assert_eq!(cpu.cycle_count(), 36, "LD HL,2000h; LD (HL),A5h");
        checked += 1;
    }

    // 6. INC (HL): 10 base + 3 memory accesses * 3 waits = 19.
    {
        let mut cpu = machine();
        cpu.mem_poke(0, 0x34);
        cpu.mem_poke(0x2000, 0x7f);
        cpu.set_reg(Reg::HL, 0x2000);
        cpu.step();
        assert_eq!(cpu.cycle_count(), 19, "INC (HL)");
        checked += 1;
    }

    // 7. JR NZ untaken: 6 base + 2 memory accesses * 3 waits = 12.
    {
        let mut cpu = machine();
        cpu.mem_poke(0, 0x20);
        cpu.mem_poke(1, 0x02);
        cpu.set_reg(Reg::AF, u16::from(FLAG_Z));
        cpu.step();
        assert_eq!(cpu.cycle_count(), 12, "JR NZ untaken");
        checked += 1;
    }

    // 8. JR NZ taken: 8 base + 2 memory accesses * 3 waits = 14.
    {
        let mut cpu = machine();
        cpu.mem_poke(0, 0x20);
        cpu.mem_poke(1, 0x02);
        cpu.step();
        assert_eq!(cpu.cycle_count(), 14, "JR NZ taken");
        checked += 1;
    }

    // 9. DJNZ untaken: 7 base + 2 memory accesses * 3 waits = 13.
    {
        let mut cpu = machine();
        cpu.mem_poke(0, 0x10);
        cpu.mem_poke(1, 0x02);
        cpu.set_reg(Reg::BC, 0x0100);
        cpu.step();
        assert_eq!(cpu.cycle_count(), 13, "DJNZ untaken");
        checked += 1;
    }

    // 10. DJNZ taken: 9 base + 2 memory accesses * 3 waits = 15.
    {
        let mut cpu = machine();
        cpu.mem_poke(0, 0x10);
        cpu.mem_poke(1, 0x02);
        cpu.set_reg(Reg::BC, 0x0200);
        cpu.step();
        assert_eq!(cpu.cycle_count(), 15, "DJNZ taken");
        checked += 1;
    }

    // 11. JP NZ untaken: 6 base + 3 memory accesses * 3 waits = 15.
    {
        let mut cpu = machine();
        cpu.mem_poke(0, 0xc2);
        cpu.mem_poke(1, 0x34);
        cpu.mem_poke(2, 0x12);
        cpu.set_reg(Reg::AF, u16::from(FLAG_Z));
        cpu.step();
        assert_eq!(cpu.cycle_count(), 15, "JP NZ untaken");
        checked += 1;
    }

    // 12. JP NZ taken: 9 base + 3 memory accesses * 3 waits = 18.
    {
        let mut cpu = machine();
        cpu.mem_poke(0, 0xc2);
        cpu.mem_poke(1, 0x34);
        cpu.mem_poke(2, 0x12);
        cpu.step();
        assert_eq!(cpu.cycle_count(), 18, "JP NZ taken");
        checked += 1;
    }

    // 13. CALL NZ untaken: 6 base + 3 memory accesses * 3 waits = 15.
    {
        let mut cpu = machine();
        cpu.mem_poke(0, 0xc4);
        cpu.mem_poke(1, 0x34);
        cpu.mem_poke(2, 0x12);
        cpu.set_reg(Reg::AF, u16::from(FLAG_Z));
        cpu.step();
        assert_eq!(cpu.cycle_count(), 15, "CALL NZ untaken");
        checked += 1;
    }

    // 14. CALL NZ taken: 16 base + 5 memory accesses * 3 waits = 31.
    {
        let mut cpu = machine();
        cpu.mem_poke(0, 0xc4);
        cpu.mem_poke(1, 0x34);
        cpu.mem_poke(2, 0x12);
        cpu.set_reg(Reg::SP, 0x2000);
        cpu.step();
        assert_eq!(cpu.cycle_count(), 31, "CALL NZ taken");
        checked += 1;
    }

    // 15. RET NZ untaken: 5 base + 1 memory access * 3 waits = 8.
    {
        let mut cpu = machine();
        cpu.mem_poke(0, 0xc0);
        cpu.set_reg(Reg::AF, u16::from(FLAG_Z));
        cpu.step();
        assert_eq!(cpu.cycle_count(), 8, "RET NZ untaken");
        checked += 1;
    }

    // 16. RET NZ taken: 10 base + 3 memory accesses * 3 waits = 19.
    {
        let mut cpu = machine();
        cpu.mem_poke(0, 0xc0);
        cpu.mem_poke(0x2000, 0x34);
        cpu.mem_poke(0x2001, 0x12);
        cpu.set_reg(Reg::SP, 0x2000);
        cpu.step();
        assert_eq!(cpu.cycle_count(), 19, "RET NZ taken");
        checked += 1;
    }

    // 17. EX DE,HL; EXX: (3 + 1*3) + (3 + 1*3) = 12.
    {
        let mut cpu = machine();
        cpu.mem_poke(0, 0xeb);
        cpu.mem_poke(1, 0xd9);
        cpu.step();
        cpu.step();
        assert_eq!(cpu.cycle_count(), 12, "EX DE,HL; EXX");
        checked += 1;
    }

    // 18. EX (SP),HL: 16 base + 5 memory accesses * 3 waits = 31.
    {
        let mut cpu = machine();
        cpu.mem_poke(0, 0xe3);
        cpu.mem_poke(0x2000, 0x34);
        cpu.mem_poke(0x2001, 0x12);
        cpu.set_reg(Reg::SP, 0x2000);
        cpu.step();
        assert_eq!(cpu.cycle_count(), 31, "EX (SP),HL");
        checked += 1;
    }

    // 19. MLT BC: 17 base + 2 memory accesses * 3 waits = 23.
    {
        let mut cpu = machine();
        cpu.mem_poke(0, 0xed);
        cpu.mem_poke(1, 0x4c);
        cpu.set_reg(Reg::BC, 0x1234);
        cpu.step();
        assert_eq!(cpu.cycle_count(), 23, "MLT BC");
        checked += 1;
    }

    // 20. LDI: 12 base + 4 memory accesses * 3 waits = 24.
    {
        let mut cpu = machine();
        cpu.mem_poke(0, 0xed);
        cpu.mem_poke(1, 0xa0);
        cpu.mem_poke(0x2000, 0x5a);
        cpu.set_reg(Reg::BC, 1);
        cpu.set_reg(Reg::DE, 0x3000);
        cpu.set_reg(Reg::HL, 0x2000);
        cpu.step();
        assert_eq!(cpu.cycle_count(), 24, "LDI");
        checked += 1;
    }

    // 21. Terminal LDIR: 12 base + 4 memory accesses * 3 waits = 24.
    {
        let mut cpu = machine();
        cpu.mem_poke(0, 0xed);
        cpu.mem_poke(1, 0xb0);
        cpu.mem_poke(0x2000, 0x5a);
        cpu.set_reg(Reg::BC, 1);
        cpu.set_reg(Reg::DE, 0x3000);
        cpu.set_reg(Reg::HL, 0x2000);
        cpu.step();
        assert_eq!(cpu.cycle_count(), 24, "LDIR terminal");
        checked += 1;
    }

    // 22. Repeating LDIR: 14 base + 4 memory accesses * 3 waits = 26.
    {
        let mut cpu = machine();
        cpu.mem_poke(0, 0xed);
        cpu.mem_poke(1, 0xb0);
        cpu.mem_poke(0x2000, 0x5a);
        cpu.set_reg(Reg::BC, 2);
        cpu.set_reg(Reg::DE, 0x3000);
        cpu.set_reg(Reg::HL, 0x2000);
        cpu.step();
        assert_eq!(cpu.cycle_count(), 26, "LDIR repeating");
        checked += 1;
    }

    // 23. OTIM: 14 base + 3 memory accesses * 3 waits + 4 I/O waits = 27.
    {
        let mut cpu = machine();
        cpu.mem_poke(0, 0xed);
        cpu.mem_poke(1, 0x83);
        cpu.mem_poke(0x2000, 0x5a);
        cpu.set_reg(Reg::BC, 0x0140);
        cpu.set_reg(Reg::HL, 0x2000);
        cpu.step();
        assert_eq!(cpu.cycle_count(), 27, "OTIM");
        checked += 1;
    }

    // 24. OTIMR B=2: 30 base + 4 memory accesses * 3 waits + 2*4 I/O waits = 50.
    {
        let mut cpu = machine();
        cpu.mem_poke(0, 0xed);
        cpu.mem_poke(1, 0x93);
        cpu.mem_poke(0x2000, 0x5a);
        cpu.mem_poke(0x2001, 0xa5);
        cpu.set_reg(Reg::BC, 0x0240);
        cpu.set_reg(Reg::HL, 0x2000);
        cpu.step();
        assert_eq!(cpu.cycle_count(), 50, "OTIMR B=2");
        checked += 1;
    }

    // 25. IN A,(n): 9 base + 2 memory accesses * 3 waits + 4 I/O waits = 19.
    {
        let mut cpu = machine();
        cpu.mem_poke(0, 0xdb);
        cpu.mem_poke(1, 0x40);
        cpu.step();
        assert_eq!(cpu.cycle_count(), 19, "IN A,(40h)");
        checked += 1;
    }

    // 26. OUT (C),B: 10 base + 2 memory accesses * 3 waits + 4 I/O waits = 20.
    {
        let mut cpu = machine();
        cpu.mem_poke(0, 0xed);
        cpu.mem_poke(1, 0x41);
        cpu.set_reg(Reg::BC, 0x5a40);
        cpu.step();
        assert_eq!(cpu.cycle_count(), 20, "OUT (C),B");
        checked += 1;
    }

    assert_eq!(checked, 26);
}

#[test]
fn timing_applies_dcntl_memory_and_external_io_waits() {
    let mut reset_nop = machine();
    reset_nop.mem_poke(0, 0x00);
    assert_eq!(reset_nop.io_reg_peek(DCNTL as u8), 0xf0);
    assert_eq!(reset_nop.step(), 6);

    let mut minimum_nop = machine();
    minimum_nop.io_regs[DCNTL] = 0x00;
    minimum_nop.mem_poke(0, 0x00);
    assert_eq!(minimum_nop.step(), 3);
    minimum_nop.reset();
    assert_eq!(minimum_nop.io_reg_peek(DCNTL as u8), 0xf0);

    let mut reset_in = machine();
    reset_in.mem_poke(0, 0xdb);
    reset_in.mem_poke(1, 0x40);
    assert_eq!(reset_in.step(), 19);

    let mut programmed_in = machine();
    programmed_in.io_regs[DCNTL] = 0x50;
    programmed_in.mem_poke(0, 0xdb);
    programmed_in.mem_poke(1, 0x40);
    assert_eq!(programmed_in.step(), 13);
}
