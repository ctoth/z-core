use super::*;

#[cfg(feature = "state")]
#[test]
fn save_state_version_and_decode_errors_are_atomic() {
    let mut cpu = machine();
    cpu.set_reg(Reg::AF, 0xa55a);
    cpu.mem_poke(0x1234, 0x66);
    let original = cpu.save_state();
    assert_eq!(original.first(), Some(&STATE_VERSION));
    assert_eq!(cpu.save_state(), original, "repeated saves are identical");

    assert_eq!(cpu.load_state(&[]), Err(StateError::MissingVersion));
    assert_eq!(
        cpu.load_state(&[STATE_VERSION.wrapping_add(1)]),
        Err(StateError::UnsupportedVersion(
            STATE_VERSION.wrapping_add(1)
        ))
    );
    assert_eq!(cpu.load_state(&[STATE_VERSION]), Err(StateError::Decode));
    assert_eq!(
        cpu.save_state(),
        original,
        "decode errors do not mutate state"
    );

    let mut decoded: SavedState = postcard::from_bytes(&original[1..])
        .expect("freshly saved payload must decode in its own test");
    decoded.io_regs.to_mut().pop();
    let payload = postcard::to_allocvec(&decoded)
        .expect("the deliberately short register file must serialize");
    let mut wrong_register_count = vec![STATE_VERSION];
    wrong_register_count.extend_from_slice(&payload);
    assert_eq!(
        cpu.load_state(&wrong_register_count),
        Err(StateError::Decode)
    );
    assert_eq!(cpu.save_state(), original, "length errors are also atomic");
}

#[cfg(feature = "state")]
#[test]
fn load_state_rejects_invalid_interrupt_mode_atomically() {
    let mut cpu = machine();
    cpu.set_interrupt_mode(1);
    let original = cpu.save_state();
    let mut decoded: SavedState = postcard::from_bytes(&original[1..])
        .expect("freshly saved payload must decode in its own test");
    decoded.interrupt_mode = 3;
    let payload =
        postcard::to_allocvec(&decoded).expect("invalid interrupt mode must still serialize");
    let mut malformed = vec![STATE_VERSION];
    malformed.extend_from_slice(&payload);

    assert_eq!(cpu.load_state(&malformed), Err(StateError::Decode));
    assert_eq!(
        cpu.save_state(),
        original,
        "invalid interrupt mode must not mutate state"
    );
}

#[cfg(feature = "state")]
#[test]
fn save_load_resume_demonstration_transcript() {
    let mut uninterrupted = machine();
    uninterrupted.write_internal_io(DCNTL, 0x00);
    uninterrupted.set_reg(Reg::AF, 0x1234);
    assert_eq!(uninterrupted.run(6), 6);
    let saved_cycle = uninterrupted.cycle_count();
    let saved_pc = uninterrupted.reg(Reg::PC);
    let saved_af = uninterrupted.reg(Reg::AF);
    let saved = uninterrupted.save_state();

    let mut resumed = machine();
    resumed.write_internal_io(DCNTL, 0x00);
    resumed.set_reg(Reg::AF, 0xa55a);
    assert_eq!(resumed.run(3), 3);
    let divergent_cycle = resumed.cycle_count();
    let divergent_pc = resumed.reg(Reg::PC);
    let divergent_af = resumed.reg(Reg::AF);

    resumed
        .load_state(&saved)
        .expect("the demonstration snapshot must load");
    let loaded_cycle = resumed.cycle_count();
    let loaded_pc = resumed.reg(Reg::PC);
    let loaded_af = resumed.reg(Reg::AF);
    assert_eq!(uninterrupted.run(9), 9);
    assert_eq!(resumed.run(9), 9);
    let uninterrupted_final = uninterrupted.save_state();
    let resumed_final = resumed.save_state();

    std::println!(
        "SAVE cycle={saved_cycle} pc={saved_pc:04X} af={saved_af:04X} state_bytes={}",
        saved.len()
    );
    std::println!("DIVERGE cycle={divergent_cycle} pc={divergent_pc:04X} af={divergent_af:04X}");
    std::println!("LOAD cycle={loaded_cycle} pc={loaded_pc:04X} af={loaded_af:04X}");
    std::println!(
        "UNINTERRUPTED cycle={} pc={:04X} af={:04X}",
        uninterrupted.cycle_count(),
        uninterrupted.reg(Reg::PC),
        uninterrupted.reg(Reg::AF)
    );
    std::println!(
        "RESUMED cycle={} pc={:04X} af={:04X}",
        resumed.cycle_count(),
        resumed.reg(Reg::PC),
        resumed.reg(Reg::AF)
    );
    std::println!("MATCH state_bytes={}", uninterrupted_final == resumed_final);

    assert_eq!(uninterrupted_final, resumed_final);
}

#[cfg(feature = "state")]
proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn save_state_round_trip_resume_matches_uninterrupted_execution(
        pre_steps in 0_u8..12,
        run_budget in 0_u32..4_000,
        dma_count in 1_u8..5,
        timer_count in 1_u8..8,
        tx_byte in any::<u8>(),
        rx_byte in any::<u8>(),
    ) {
        let mut original = machine();
        original.write_internal_io(DCNTL, 0x00);
        original.set_reg(Reg::AF, u16::from(tx_byte) << 8 | u16::from(rx_byte));
        original.set_reg(Reg::SP, 0x8000);

        original.write_internal_io(TMDR0L, timer_count);
        original.write_internal_io(TMDR0H, 0x00);
        original.write_internal_io(RLDR0L, timer_count.wrapping_add(1));
        original.write_internal_io(RLDR0H, 0x00);
        original.write_internal_io(TCR, 0x01);

        original.write_internal_io(CNTLB0, 0x00);
        original.write_internal_io(CNTLA0, 0x64);
        original.write_internal_io(TDR0, tx_byte);
        prop_assert!(original.asci_rx_push(0, rx_byte));

        original.write_internal_io(TRD, tx_byte ^ rx_byte);
        original.write_internal_io(CNTR, 0x10);

        for offset in 0..dma_count {
            original.mem_poke(0x0100 + u32::from(offset), tx_byte.wrapping_add(offset));
        }
        original.write_internal_io(SAR0L, 0x00);
        original.write_internal_io(SAR0H, 0x01);
        original.write_internal_io(DAR0L, 0x00);
        original.write_internal_io(DAR0H, 0x02);
        original.write_internal_io(BCR0L, dma_count);
        original.write_internal_io(DMODE, 0x00);
        original.write_internal_io(DSTAT, 0x60);

        for _ in 0..pre_steps {
            prop_assert_ne!(original.step(), 0);
        }

        let saved = original.save_state();
        let mut resumed = machine();
        prop_assert_eq!(resumed.load_state(&saved), Ok(()));
        prop_assert_eq!(resumed.save_state(), saved);

        prop_assert_eq!(original.run(run_budget), resumed.run(run_budget));
        prop_assert_eq!(original.asci_tx_pop(0), resumed.asci_tx_pop(0));
        prop_assert_eq!(original.csio_tx_pop(), resumed.csio_tx_pop());
        prop_assert_eq!(original.drain_events(), resumed.drain_events());
        prop_assert_eq!(original.save_state(), resumed.save_state());
    }
}

#[cfg(feature = "state")]
#[test]
fn determinism_timer_asci_dma_matches_after_ten_million_cycles() {
    const RUN_CYCLES: u32 = 10_000_000;

    let mut first = mmu_machine();
    let mut second = mmu_machine();
    for cpu in [&mut first, &mut second] {
        cpu.write_internal_io(DCNTL, 0x80);

        cpu.set_reg(Reg::PC, 0xffff);
        cpu.mem_poke(0xffff, 0xdd);
        cpu.mem_poke(0x0000, 0x76);

        cpu.write_internal_io(TMDR0L, 0x07);
        cpu.write_internal_io(TMDR0H, 0x00);
        cpu.write_internal_io(RLDR0L, 0x0b);
        cpu.write_internal_io(RLDR0H, 0x00);
        cpu.write_internal_io(TCR, 0x01);

        cpu.write_internal_io(CNTLB0, 0x00);
        cpu.write_internal_io(CNTLA0, 0x64);
        cpu.write_internal_io(TDR0, 0x5a);
        assert!(cpu.asci_rx_push(0, 0xa5));

        for offset in 0_u32..0x100 {
            cpu.mem_poke(0x1_0000 + offset, offset as u8 ^ 0xa5);
        }
        cpu.write_internal_io(SAR0L, 0x00);
        cpu.write_internal_io(SAR0H, 0x00);
        cpu.write_internal_io(SAR0B, 0x01);
        cpu.write_internal_io(DAR0L, 0x00);
        cpu.write_internal_io(DAR0H, 0x00);
        cpu.write_internal_io(DAR0B, 0x02);
        cpu.write_internal_io(BCR0L, 0x00);
        cpu.write_internal_io(BCR0H, 0x01);
        cpu.write_internal_io(DMODE, 0x00);
        cpu.write_internal_io(DSTAT, 0x60);
    }

    assert_eq!(first.run(RUN_CYCLES), RUN_CYCLES);
    assert_eq!(second.run(RUN_CYCLES), RUN_CYCLES);
    assert_eq!(first.cycle_count(), u64::from(RUN_CYCLES));
    assert_eq!(second.cycle_count(), u64::from(RUN_CYCLES));

    let first_state = first.save_state();
    let second_state = second.save_state();
    let first_events = first.drain_events();
    let second_events = second.drain_events();

    assert_eq!(
        first_events.len(),
        1,
        "the scripted TRAP makes the stream nonempty"
    );
    assert_eq!(first_state, second_state);
    assert_eq!(first_events, second_events);
}
