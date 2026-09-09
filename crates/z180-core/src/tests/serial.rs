use super::*;

#[test]
fn asci_standard_divisors_and_frame_formats_use_hand_computed_cycle_counts() {
    let mut cpu = machine();

    cpu.write_internal_io(CNTLA0, 0x44); // RE, 8 data, no parity, 1 stop.
    cpu.write_internal_io(CNTLB0, 0x00); // /10, /16, SS=0: 160 phi/bit.
    assert!(cpu.asci_rx_push(0, 0xa5));
    cpu.finish_step(1_599); // (1 start + 8 data + 1 stop) * 160 - 1.
    assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0x80, 0);
    cpu.finish_step(1);
    assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0x80, 0x80);
    assert_eq!(cpu.read_internal_io(RDR0), 0xa5);

    cpu.write_internal_io(CNTLA0, 0x47); // RE, 8 data, parity, 2 stop.
    cpu.write_internal_io(CNTLB0, 0x22); // /30, /16, SS=2: 1920 phi/bit.
    assert!(cpu.asci_rx_push(0, 0x5a));
    cpu.finish_step(23_039); // (1 + 8 + 1 + 2) * 1920 - 1.
    assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0x80, 0);
    cpu.finish_step(1);
    assert_eq!(cpu.read_internal_io(RDR0), 0x5a);

    cpu.write_internal_io(CNTLA0, 0x40); // RE, 7 data, no parity, 1 stop.
    cpu.write_internal_io(CNTLB0, 0x09); // /10, /64, SS=1: 1280 phi/bit.
    assert!(cpu.asci_rx_push(0, 0x33));
    cpu.finish_step(11_519); // (1 + 7 + 1) * 1280 - 1.
    assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0x80, 0);
    cpu.finish_step(1);
    assert_eq!(cpu.read_internal_io(RDR0), 0x33);
}

#[test]
fn asci_receive_started_on_external_clock_resumes_when_internal_clock_is_selected() {
    let mut cpu = machine();
    cpu.write_internal_io(CNTLA0, 0x64);
    assert!(cpu.asci_rx_push(0, 0x5a));

    cpu.write_internal_io(CNTLB0, 0x02);
    cpu.finish_step(6_400);

    assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0x80, 0x80);
    assert_eq!(cpu.read_internal_io(RDR0), 0x5a);
}

#[test]
fn asci_tdr_tsr_double_buffering_and_tdre_follow_transmit_progress() {
    let mut cpu = machine();
    cpu.write_internal_io(CNTLB0, 0x00);
    cpu.write_internal_io(CNTLA0, 0x24); // TE, 8N1.

    cpu.write_internal_io(TDR0, 0x11);
    assert_eq!(
        cpu.io_reg_peek(STAT0 as u8) & 0x02,
        0x02,
        "TDR moved to TSR"
    );
    cpu.write_internal_io(TDR0, 0x22);
    assert_eq!(
        cpu.io_reg_peek(STAT0 as u8) & 0x02,
        0x00,
        "second byte fills TDR"
    );

    cpu.finish_step(1_599);
    assert_eq!(cpu.asci_tx_pop(0), None);
    cpu.finish_step(1);
    assert_eq!(cpu.asci_tx_pop(0), Some(0x11));
    assert_eq!(
        cpu.io_reg_peek(STAT0 as u8) & 0x02,
        0x02,
        "second byte moved to TSR"
    );

    cpu.finish_step(1_600);
    assert_eq!(cpu.asci_tx_pop(0), Some(0x22));
    assert_eq!(cpu.asci_tx_pop(0), None);
}

#[test]
fn asci_rdr_rsr_double_buffering_sets_overrun_without_replacing_rdr() {
    let mut cpu = machine();
    cpu.write_internal_io(CNTLB0, 0x00);
    cpu.write_internal_io(CNTLA0, 0x44);

    assert!(cpu.asci_rx_push(0, 0x11));
    cpu.finish_step(1_600);
    assert!(
        cpu.asci_rx_push(0, 0x22),
        "RSR remains available while RDR is full"
    );
    cpu.finish_step(1_600);

    assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0xc0, 0xc0);
    assert_eq!(
        cpu.read_internal_io(RDR0),
        0x11,
        "overrun preserves the prior RDR byte"
    );
    assert_eq!(
        cpu.io_reg_peek(STAT0 as u8) & 0xc0,
        0x40,
        "RDR read leaves OVRN set"
    );
    cpu.write_internal_io(CNTLA0, 0x44); // EFR=0 clears OVRN/PE/FE.
    assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0x70, 0);
}

#[test]
fn s180_asci_fifo_and_astc_brg_have_the_documented_depth_and_timing() {
    let mut cpu = recording_machine(Variant::Z8S180);
    cpu.write_internal_io(CNTLB0, 0x00);
    cpu.write_internal_io(CNTLA0, 0x44);

    for byte in 0_u8..4 {
        assert!(cpu.asci_rx_push(0, byte));
        cpu.finish_step(1_600);
    }
    assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0xc0, 0x80);
    assert!(cpu.asci_rx_push(0, 4));
    cpu.finish_step(1_600);
    assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0xc0, 0xc0);
    for byte in 0_u8..4 {
        assert_eq!(cpu.read_internal_io(RDR0), byte);
    }
    assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0x80, 0);

    cpu.write_internal_io(0x12, 0x18); // X1 bit clock + 16-bit BRG.
    cpu.write_internal_io(ASTC0L, 0x03);
    cpu.write_internal_io(ASTC0H, 0x00);
    assert!(cpu.asci_rx_push(0, 0x55));
    cpu.finish_step(99); // 8N1 * [2 * (3 + 2) * 1] - 1.
    assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0x80, 0);
    cpu.finish_step(1);
    assert_eq!(cpu.read_internal_io(RDR0), 0x55);
}

#[test]
fn asci_rie_tie_and_s180_rdrf_inhibit_qualify_internal_requests() {
    let mut cpu = machine();
    cpu.write_internal_io(CNTLB0, 0x00);
    cpu.write_internal_io(CNTLA0, 0x44);
    cpu.write_internal_io(STAT0, 0x08);
    assert!(cpu.asci_rx_push(0, 0xa5));
    cpu.finish_step(1_599);
    assert_eq!(cpu.internal_irq_pending & 0x20, 0);
    cpu.finish_step(1);
    assert_eq!(cpu.internal_irq_pending & 0x20, 0x20);
    assert_eq!(cpu.read_internal_io(RDR0), 0xa5);
    assert_eq!(cpu.internal_irq_pending & 0x20, 0);

    cpu.write_internal_io(STAT0, 0x01);
    assert_eq!(
        cpu.internal_irq_pending & 0x20,
        0x20,
        "TIE with TDRE requests"
    );
    cpu.write_internal_io(TDR0, 0x5a);
    assert_eq!(
        cpu.internal_irq_pending & 0x20,
        0,
        "TE is off, so TDR remains full"
    );

    let mut s180 = recording_machine(Variant::Z8S180);
    s180.write_internal_io(CNTLB0, 0x00);
    s180.write_internal_io(CNTLA0, 0x44);
    s180.write_internal_io(STAT0, 0x08);
    assert!(s180.asci_rx_push(0, 0x33));
    s180.finish_step(1_600);
    assert_eq!(
        s180.internal_irq_pending & 0x20,
        0,
        "reset ASEXT inhibits RDRF IRQ"
    );
    s180.write_internal_io(0x12, 0x80);
    assert_eq!(
        s180.internal_irq_pending & 0x20,
        0x20,
        "ASEXT bit 7 removes inhibit"
    );
}

#[test]
fn both_asci_channels_deliver_their_internal_vectors_from_halt() {
    for (channel, pending, vector) in [(0_usize, 0x20_u8, 0x20ae_u32), (1, 0x40, 0x20b0)] {
        let mut cpu = machine();
        cpu.write_internal_io(DCNTL, 0x00);
        cpu.write_internal_io(IL, 0xa0);
        cpu.mem_poke(0, 0x76);
        cpu.mem_poke(vector, 0x56);
        cpu.mem_poke(vector + 1, 0x34);
        cpu.set_reg(Reg::IR, 0x2000);
        cpu.set_reg(Reg::SP, 0x8000);
        cpu.set_iff1(true);

        assert_eq!(cpu.step(), 3, "channel {channel} executes HALT");
        assert!(cpu.halted());
        cpu.write_internal_io(STAT0 + channel, 0x01);
        assert_eq!(cpu.internal_irq_pending & pending, pending);
        assert_eq!(cpu.step(), 18, "channel {channel} acknowledges ASCI IRQ");
        assert_eq!(cpu.reg(Reg::PC), 0x3456);
        assert_eq!(cpu.reg(Reg::SP), 0x7ffe);
    }
}

#[test]
fn asci_cts_suppresses_only_the_documented_tdre_surfaces() {
    let mut cpu = machine();
    assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0x02, 0x02);
    cpu.write_internal_io(CNTLB0, 0x00);
    cpu.write_internal_io(CNTLA0, 0x24);
    cpu.write_internal_io(TDR0, 0xa5);
    cpu.set_asci_cts(0, true);
    assert_eq!(cpu.io_reg_peek(CNTLB0 as u8) & 0x20, 0x20);
    assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0x02, 0);
    cpu.finish_step(1_600);
    assert_eq!(cpu.asci_tx_pop(0), Some(0xa5), "CTS does not stop TSR");
    assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0x02, 0);
    cpu.set_asci_cts(0, false);
    assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0x02, 0x02);

    cpu.set_asci_cts(1, true);
    assert_eq!(cpu.io_reg_peek(STAT1 as u8) & 0x02, 0x02, "CTS1E is clear");
    cpu.write_internal_io(STAT1, 0x04);
    assert_eq!(cpu.io_reg_peek(CNTLB0 as u8 + 1) & 0x20, 0x20);
    assert_eq!(
        cpu.io_reg_peek(STAT1 as u8) & 0x02,
        0,
        "CTS1E enables gating"
    );

    let mut s180 = recording_machine(Variant::Z8S180);
    s180.set_asci_cts(0, true);
    assert_eq!(s180.io_reg_peek(STAT0 as u8) & 0x02, 0);
    s180.write_internal_io(0x12, 0x20);
    assert_eq!(
        s180.io_reg_peek(STAT0 as u8) & 0x02,
        0x02,
        "ASEXT CTS0 disable makes the pin advisory"
    );
}

#[test]
fn asci_dcd_transition_latch_gates_receive_and_requests_until_stat0_read() {
    let mut cpu = machine();
    cpu.write_internal_io(CNTLB0, 0x00);
    cpu.write_internal_io(CNTLA0, 0x44);
    cpu.write_internal_io(STAT0, 0x08);
    assert!(cpu.asci_rx_push(0, 0x11));

    cpu.set_asci_dcd(0, true);
    assert_eq!(cpu.internal_irq_pending & 0x20, 0x20);
    assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0xf4, 0x04);
    assert!(!cpu.asci_rx_push(0, 0x22));
    cpu.set_asci_dcd(0, false);
    assert!(!cpu.asci_rx_push(0, 0x33), "latched DCD remains inhibiting");

    assert_eq!(
        cpu.read_internal_io(STAT0) & 0x04,
        0x04,
        "first read reports prior high"
    );
    assert_eq!(cpu.internal_irq_pending & 0x20, 0);
    assert_eq!(
        cpu.read_internal_io(STAT0) & 0x04,
        0,
        "second read reports low"
    );
    assert!(cpu.asci_rx_push(0, 0x44));

    let mut s180 = recording_machine(Variant::Z8S180);
    s180.write_internal_io(CNTLB0, 0x00);
    s180.write_internal_io(CNTLA0, 0x44);
    s180.write_internal_io(0x12, 0x40);
    s180.set_asci_dcd(0, true);
    assert!(
        s180.asci_rx_push(0, 0x55),
        "ASEXT DCD disable makes the pin advisory"
    );
    s180.finish_step(1_600);
    assert_eq!(s180.read_internal_io(RDR0), 0x55);
}

#[test]
fn asci_reset_and_iostop_preserve_data_registers_while_stopping_operations() {
    let mut cpu = machine();
    cpu.write_internal_io(TDR0, 0xa5);
    cpu.write_internal_io(RDR0, 0x5a);
    cpu.reset();
    assert_eq!(cpu.io_reg_peek(TDR0 as u8), 0xa5);
    assert_eq!(cpu.io_reg_peek(RDR0 as u8), 0x5a);
    assert_eq!(cpu.io_reg_peek(STAT0 as u8), 0x02);

    cpu.write_internal_io(CNTLB0, 0x00);
    cpu.write_internal_io(CNTLA0, 0x64);
    cpu.write_internal_io(TDR0, 0x11);
    assert!(cpu.asci_rx_push(0, 0x22));
    cpu.io_regs[STAT0] |= 0x70;
    cpu.write_internal_io(ICR, 0x20);
    assert_eq!(cpu.io_reg_peek(CNTLA0 as u8) & 0x60, 0);
    assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0xf2, 0x02);
    cpu.write_internal_io(CNTLA0, 0x64);
    cpu.write_internal_io(TDR0, 0x33);
    assert_eq!(cpu.io_reg_peek(CNTLA0 as u8) & 0x60, 0);
    assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0x02, 0x02);
    assert!(!cpu.asci_rx_push(0, 0x44));
    cpu.finish_step(3_200);
    assert_eq!(cpu.asci_tx_pop(0), None);
    assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0x80, 0);
    assert_eq!(cpu.io_reg_peek(TDR0 as u8), 0x33);
    assert_eq!(cpu.io_reg_peek(RDR0 as u8), 0x5a);
}

#[test]
fn csio_internal_speed_selects_match_table_22_byte_timings() {
    let mut cpu = machine();
    for speed in 0_u8..=6 {
        let expected = 160_u32 << speed;
        cpu.write_internal_io(TRD, 0x80 | speed);
        cpu.write_internal_io(CNTR, 0x10 | speed);

        cpu.finish_step(expected - 1);
        assert_eq!(cpu.io_reg_peek(CNTR as u8) & 0x90, 0x10, "SS={speed}");
        assert_eq!(cpu.csio_tx_pop(), None, "SS={speed} before completion");
        cpu.finish_step(1);
        assert_eq!(cpu.io_reg_peek(CNTR as u8) & 0x90, 0x80, "SS={speed}");
        assert_eq!(cpu.csio_tx_pop(), Some(0x80 | speed), "SS={speed} byte");
    }
}

#[test]
fn csio_receive_is_half_duplex_unbuffered_and_clears_ef_on_trd_access() {
    let mut cpu = machine();
    cpu.write_internal_io(CNTR, 0x20);
    assert!(cpu.csio_rx_push(0xa5));
    assert!(!cpu.csio_rx_push(0x5a), "one RXS shift operation at a time");
    cpu.finish_step(159);
    assert_eq!(cpu.io_reg_peek(CNTR as u8) & 0xa0, 0x20);
    cpu.finish_step(1);
    assert_eq!(cpu.io_reg_peek(CNTR as u8) & 0xa0, 0x80);
    assert_eq!(cpu.read_internal_io(TRD), 0xa5);
    assert_eq!(cpu.io_reg_peek(CNTR as u8) & 0x80, 0);

    cpu.write_internal_io(CNTR, 0x30);
    assert_eq!(
        cpu.io_reg_peek(CNTR as u8) & 0x30,
        0,
        "invalid duplex request stops"
    );

    cpu.write_internal_io(STAT1, 0x04);
    cpu.write_internal_io(CNTR, 0x20);
    assert_eq!(
        cpu.io_reg_peek(CNTR as u8) & 0x20,
        0,
        "CTS1E owns the RXS pin"
    );
    assert!(!cpu.csio_rx_push(0x33));
}

#[test]
fn csio_is_unbuffered_and_software_clears_abort_active_operations() {
    let mut cpu = machine();
    cpu.write_internal_io(TRD, 0x11);
    cpu.write_internal_io(CNTR, 0x10);
    cpu.finish_step(80);
    cpu.write_internal_io(TRD, 0x22);
    cpu.finish_step(80);
    assert_eq!(
        cpu.csio_tx_pop(),
        Some(0x22),
        "TRD write updates active shift data"
    );

    cpu.write_internal_io(TRD, 0x33);
    cpu.write_internal_io(CNTR, 0x10);
    cpu.finish_step(80);
    cpu.write_internal_io(CNTR, 0x00);
    cpu.finish_step(160);
    assert_eq!(cpu.csio_tx_pop(), None);
    assert_eq!(cpu.io_reg_peek(CNTR as u8) & 0x90, 0);

    cpu.write_internal_io(CNTR, 0x20);
    assert!(cpu.csio_rx_push(0x44));
    cpu.finish_step(80);
    cpu.write_internal_io(CNTR, 0x00);
    cpu.finish_step(160);
    assert_eq!(cpu.io_reg_peek(CNTR as u8) & 0xa0, 0);
    assert_eq!(cpu.io_reg_peek(TRD as u8), 0x33);
}

#[test]
fn csio_ef_eie_interrupt_protocol_and_vector_delivery_match_um0050() {
    let mut cpu = machine();
    cpu.write_internal_io(DCNTL, 0x00);
    cpu.write_internal_io(IL, 0xa0);
    cpu.mem_poke(0, 0x76);
    cpu.mem_poke(0x20ac, 0x56);
    cpu.mem_poke(0x20ad, 0x34);
    cpu.set_reg(Reg::IR, 0x2000);
    cpu.set_reg(Reg::SP, 0x8000);
    cpu.set_iff1(true);

    assert_eq!(cpu.step(), 3);
    assert!(cpu.halted());
    cpu.write_internal_io(TRD, 0xa5);
    cpu.write_internal_io(CNTR, 0x50);
    cpu.finish_step(160);
    assert_eq!(cpu.io_reg_peek(CNTR as u8) & 0xd0, 0xc0);
    assert_eq!(cpu.internal_irq_pending & 0x10, 0x10);

    assert_eq!(cpu.step(), 18);
    assert_eq!(cpu.reg(Reg::PC), 0x3456);
    assert_eq!(cpu.reg(Reg::SP), 0x7ffe);
    assert_eq!(cpu.read_internal_io(TRD), 0xa5);
    assert_eq!(cpu.internal_irq_pending & 0x10, 0);
}

#[test]
fn csio_external_clock_waits_and_reset_iostop_preserve_trd() {
    let mut cpu = machine();
    cpu.write_internal_io(TRD, 0xa5);
    cpu.write_internal_io(CNTR, 0x57);
    cpu.finish_step(1_000_000);
    assert_eq!(cpu.csio_tx_pop(), None);
    assert_eq!(cpu.io_reg_peek(CNTR as u8) & 0x90, 0x10);

    cpu.write_internal_io(ICR, 0x20);
    assert_eq!(cpu.io_reg_peek(CNTR as u8), 0x47, "IOSTOP preserves EIE");
    cpu.write_internal_io(CNTR, 0x50);
    cpu.write_internal_io(TRD, 0x5a);
    assert_eq!(cpu.io_reg_peek(CNTR as u8) & 0xb0, 0);
    cpu.finish_step(160);
    assert_eq!(cpu.csio_tx_pop(), None);

    cpu.reset();
    assert_eq!(cpu.io_reg_peek(CNTR as u8), 0x07);
    assert_eq!(cpu.io_reg_peek(TRD as u8), 0x5a);
}
