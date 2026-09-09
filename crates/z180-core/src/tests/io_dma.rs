use super::*;

#[test]
fn ioregs_reset_masks_and_variants_match_um0050() {
    let mut baseline = machine();
    assert_eq!(baseline.io_reg_peek(0x00), 0x10, "CNTLA0");
    assert_eq!(baseline.io_reg_peek(0x02), 0x07, "CNTLB0");
    assert_eq!(baseline.io_reg_peek(0x04), 0x02, "STAT0 TDRE");
    assert_eq!(baseline.io_reg_peek(TMDR0L as u8), 0xff, "TMDR0L");
    assert_eq!(baseline.io_reg_peek(TMDR0H as u8), 0xff, "TMDR0H");
    assert_eq!(baseline.io_reg_peek(0x0e), 0xff, "RLDR0L");
    assert_eq!(baseline.io_reg_peek(TMDR1L as u8), 0xff, "TMDR1L");
    assert_eq!(baseline.io_reg_peek(TMDR1H as u8), 0xff, "TMDR1H");
    assert_eq!(baseline.io_reg_peek(0x18), 0xff, "FRC");
    assert_eq!(baseline.io_reg_peek(0x30), 0x30, "DSTAT");
    assert_eq!(baseline.io_reg_peek(0x32), 0xf0, "DCNTL");
    assert_eq!(baseline.io_reg_peek(0x34), 0x01, "ITC");
    assert_eq!(baseline.io_reg_peek(0x36), 0xc0, "RCR");
    assert_eq!(baseline.io_reg_peek(0x3a), 0xf0, "CBAR");
    assert_eq!(baseline.io_reg_peek(0x3e), 0xa0, "OMCR read mask");
    assert_eq!(baseline.io_regs[0x3e], 0xe0, "OMCR raw reset");
    assert_eq!(baseline.io_reg_peek(0x3f), 0x00, "ICR");
    assert_eq!(baseline.io_reg_peek(0x12), 0x00, "S180 register reserved");
    assert_eq!(baseline.io_reg_peek(0x80), 0x00, "out of range");

    baseline.io_regs.fill(0x5a);
    baseline.reset();
    assert_eq!(baseline.io_reg_peek(0x32), 0xf0);
    assert_eq!(baseline.io_reg_peek(0x34), 0x01);
    assert_eq!(baseline.io_reg_peek(0x3a), 0xf0);
    assert_eq!(baseline.io_reg_peek(0x12), 0x00);

    let s180 = recording_machine(Variant::Z8S180);
    assert_eq!(s180.io_reg_peek(0x12), 0x00, "ASEXT0");
    assert_eq!(s180.io_reg_peek(0x1e), 0x7f, "CMR fixed bits");
    assert_eq!(s180.io_reg_peek(0x1f), 0x00, "CCR");
    assert_eq!(s180.io_reg_peek(0x2d), 0x00, "IAR1B");
}

#[test]
fn ioregs_write_masks_and_special_effects_match_um0050() {
    let mut cpu = machine();
    cpu.write_internal_io(0x33, 0xff);
    assert_eq!(cpu.io_reg_peek(0x33), 0xe0, "IL low bits are not stored");

    cpu.write_internal_io(0x3e, 0xff);
    assert_eq!(cpu.io_regs[0x3e], 0xe0, "OMCR stores writable bits");
    assert_eq!(cpu.io_reg_peek(0x3e), 0xa0, "OMCR M1TE is write-only");

    cpu.write_internal_io(ITC, 0xff);
    assert_eq!(cpu.io_reg_peek(ITC as u8), 0x07, "software cannot set TRAP");
    cpu.io_regs[ITC] = 0xc1;
    cpu.write_internal_io(ITC, 0x87);
    assert_eq!(cpu.io_reg_peek(ITC as u8), 0xc7, "UFO is read-only");
    cpu.write_internal_io(ITC, 0x00);
    assert_eq!(cpu.io_reg_peek(ITC as u8), 0x40, "zero clears TRAP");

    cpu.write_internal_io(0x30, 0xff);
    assert_eq!(cpu.io_reg_peek(0x30), 0x3c, "DWE high blocks DE writes");
    cpu.write_internal_io(0x30, 0xcf);
    assert_eq!(cpu.io_reg_peek(0x30), 0xfd, "DWE low enables DE writes");
    cpu.write_internal_io(0x30, 0x20);
    assert_eq!(
        cpu.io_reg_peek(0x30),
        0xb1,
        "DE0 clears without clearing DME"
    );

    let mut s180 = recording_machine(Variant::Z8S180);
    s180.write_internal_io(0x08, 0xa5);
    assert_eq!(s180.io_reg_peek(0x08), 0xa5);
    s180.io_regs[0x04] |= 0x80;
    s180.write_internal_io(0x08, 0x5a);
    assert_eq!(s180.io_reg_peek(0x08), 0xa5, "RDRF blocks S180 RDR writes");
}

#[test]
fn dma_dstat_enable_protocol_and_level_interrupts_match_um0050() {
    let mut cpu = machine();

    cpu.write_internal_io(DSTAT, 0xff);
    assert_eq!(cpu.io_reg_peek(DSTAT as u8), 0x3c, "high DWE blocks DE");
    assert_eq!(cpu.internal_irq_pending & 0x0c, 0x0c, "inactive DE + DIE");

    cpu.write_internal_io(DSTAT, 0xcf);
    assert_eq!(cpu.io_reg_peek(DSTAT as u8), 0xfd, "low DWE writes both DE");
    assert_eq!(cpu.internal_irq_pending & 0x0c, 0, "enabled channels");

    cpu.write_internal_io(DSTAT, 0x2c);
    assert_eq!(cpu.io_reg_peek(DSTAT as u8), 0xbd, "only DE0 clears");
    assert_eq!(cpu.internal_irq_pending & 0x0c, 0x04, "DMA0 level request");

    cpu.write_internal_io(DSTAT, 0x18);
    assert_eq!(cpu.io_reg_peek(DSTAT as u8), 0x39, "only DE1 clears");
    assert_eq!(cpu.internal_irq_pending & 0x0c, 0x08, "DMA1 level request");
}

#[test]
fn dma0_memory_copy_modes_and_cycle_costs_match_um0050() {
    let mut burst = machine();
    for (offset, byte) in [0x11_u8, 0x22, 0x33].into_iter().enumerate() {
        burst.mem_poke(0x0100 + offset as u32, byte);
    }
    burst.write_internal_io(SAR0L, 0x00);
    burst.write_internal_io(SAR0H, 0x01);
    burst.write_internal_io(SAR0B, 0x00);
    burst.write_internal_io(DAR0L, 0x00);
    burst.write_internal_io(DAR0H, 0x02);
    burst.write_internal_io(DAR0B, 0x00);
    burst.write_internal_io(BCR0L, 0x03);
    burst.write_internal_io(BCR0H, 0x00);
    burst.write_internal_io(DMODE, 0x02);
    burst.write_internal_io(DCNTL, 0x80);
    burst.write_internal_io(DSTAT, 0x64);

    assert_eq!(burst.step(), 35, "3 * (6 + 2 + 2) DMA + 3 + 2 NOP");
    assert_eq!(burst.mem_peek(0x0200), 0x11);
    assert_eq!(burst.mem_peek(0x0201), 0x22);
    assert_eq!(burst.mem_peek(0x0202), 0x33);
    assert_eq!(burst.io_reg_peek(SAR0L as u8), 0x03);
    assert_eq!(burst.io_reg_peek(DAR0L as u8), 0x03);
    assert_eq!(burst.io_reg_peek(BCR0L as u8), 0x00);
    assert_eq!(
        burst.io_reg_peek(DSTAT as u8),
        0x35,
        "DE0 clears, DIE0 stays"
    );
    assert_eq!(burst.internal_irq_pending & 0x04, 0x04);

    let mut steal = machine();
    steal.mem_poke(0x0101, 0xa1);
    steal.mem_poke(0x0100, 0xa0);
    steal.write_internal_io(SAR0L, 0x01);
    steal.write_internal_io(SAR0H, 0x01);
    steal.write_internal_io(DAR0L, 0x01);
    steal.write_internal_io(DAR0H, 0x02);
    steal.write_internal_io(BCR0L, 0x02);
    steal.write_internal_io(DMODE, 0x14);
    steal.write_internal_io(DCNTL, 0x00);
    steal.write_internal_io(DSTAT, 0x60);

    assert_eq!(steal.step(), 9, "one 6-cycle DMA byte + one 3-cycle NOP");
    assert_eq!(steal.mem_peek(0x0201), 0xa1);
    assert_eq!(steal.mem_peek(0x0200), 0x00);
    assert_eq!(steal.io_reg_peek(BCR0L as u8), 0x01);
    assert_eq!(steal.step(), 9);
    assert_eq!(steal.mem_peek(0x0200), 0xa0);
    assert_eq!(steal.io_reg_peek(BCR0L as u8), 0x00);

    let mut crossing = mmu_machine();
    crossing.mem_poke(0x0fffe, 0xd0);
    crossing.mem_poke(0x0ffff, 0xd1);
    crossing.mem_poke(0x10000, 0xd2);
    crossing.write_internal_io(SAR0L, 0xfe);
    crossing.write_internal_io(SAR0H, 0xff);
    crossing.write_internal_io(SAR0B, 0x00);
    crossing.write_internal_io(DAR0L, 0xfe);
    crossing.write_internal_io(DAR0H, 0xff);
    crossing.write_internal_io(DAR0B, 0x01);
    crossing.write_internal_io(BCR0L, 0x03);
    crossing.write_internal_io(DMODE, 0x02);
    crossing.write_internal_io(DCNTL, 0x00);
    crossing.write_internal_io(DSTAT, 0x60);
    assert_eq!(
        crossing.step(),
        23,
        "3 * 6 DMA + two A15/A16 carry states + 3-cycle NOP"
    );
    assert_eq!(crossing.mem_peek(0x1fffe), 0xd0);
    assert_eq!(crossing.mem_peek(0x1ffff), 0xd1);
    assert_eq!(crossing.mem_peek(0x20000), 0xd2);
}

#[test]
fn dma0_edge_sense_transfers_memory_and_io_in_both_directions() {
    let mut cpu = recording_machine(Variant::Z80180);
    cpu.mem_poke(0x0300, 0x41);
    cpu.mem_poke(0x0301, 0x42);
    cpu.write_internal_io(SAR0L, 0x00);
    cpu.write_internal_io(SAR0H, 0x03);
    cpu.write_internal_io(DAR0L, 0x34);
    cpu.write_internal_io(DAR0H, 0x12);
    cpu.write_internal_io(BCR0L, 0x02);
    cpu.write_internal_io(DMODE, 0x30);
    cpu.write_internal_io(DCNTL, 0x08);
    cpu.write_internal_io(DSTAT, 0x60);
    cpu.set_dreq(0, true);

    assert_eq!(cpu.step(), 10, "6 + one I/O wait + 3-cycle NOP");
    assert_eq!(cpu.bus.writes, vec![(0x1234, 0x41)]);
    assert_eq!(cpu.io_reg_peek(BCR0L as u8), 0x01);
    assert_eq!(cpu.step(), 3, "held edge-sense DREQ does not retrigger");
    assert_eq!(cpu.bus.writes, vec![(0x1234, 0x41)]);
    cpu.set_dreq(0, false);
    cpu.set_dreq(0, true);
    assert_eq!(cpu.step(), 10);
    assert_eq!(cpu.bus.writes, vec![(0x1234, 0x41), (0x1234, 0x42)]);

    cpu.bus.read_value = 0x5a;
    cpu.write_internal_io(SAR0L, 0x78);
    cpu.write_internal_io(SAR0H, 0x56);
    cpu.write_internal_io(DAR0L, 0x00);
    cpu.write_internal_io(DAR0H, 0x05);
    cpu.write_internal_io(BCR0L, 0x01);
    cpu.write_internal_io(DMODE, 0x0c);
    cpu.write_internal_io(DSTAT, 0x60);
    cpu.set_dreq(0, false);
    cpu.set_dreq(0, true);
    assert_eq!(cpu.step(), 10);
    assert_eq!(cpu.bus.reads, vec![0x5678]);
    assert_eq!(cpu.mem_peek(0x0500), 0x5a);
}

#[test]
fn dma1_level_sense_uses_scripted_host_bus_in_both_directions() {
    let mut cpu = recording_machine(Variant::Z80180);
    cpu.mem_poke(0x0300, 0x71);
    cpu.mem_poke(0x0301, 0x72);
    cpu.write_internal_io(MAR1L, 0x00);
    cpu.write_internal_io(MAR1H, 0x03);
    cpu.write_internal_io(IAR1L, 0x34);
    cpu.write_internal_io(IAR1H, 0x12);
    cpu.write_internal_io(BCR1L, 0x02);
    cpu.write_internal_io(DCNTL, 0x10);
    cpu.write_internal_io(DSTAT, 0x90);
    cpu.set_dreq(1, true);

    assert_eq!(cpu.step(), 19, "2 * (6 + 2 I/O waits) + 3-cycle NOP");
    assert_eq!(cpu.bus.writes, vec![(0x1234, 0x71), (0x1234, 0x72)]);
    assert_eq!(cpu.io_reg_peek(MAR1L as u8), 0x02);
    assert_eq!(cpu.io_reg_peek(BCR1L as u8), 0x00);

    cpu.bus.read_value = 0xa5;
    cpu.write_internal_io(MAR1L, 0x01);
    cpu.write_internal_io(MAR1H, 0x04);
    cpu.write_internal_io(IAR1L, 0x78);
    cpu.write_internal_io(IAR1H, 0x56);
    cpu.write_internal_io(BCR1L, 0x02);
    cpu.write_internal_io(DCNTL, 0x13);
    cpu.write_internal_io(DSTAT, 0x90);

    assert_eq!(cpu.step(), 19, "2 * (6 + 2 I/O waits) + 3-cycle NOP");
    assert_eq!(cpu.bus.reads, vec![0x5678, 0x5678]);
    assert_eq!(cpu.mem_peek(0x0401), 0xa5);
    assert_eq!(cpu.mem_peek(0x0400), 0xa5);
    assert_eq!(cpu.io_reg_peek(MAR1L as u8), 0xff);
    assert_eq!(cpu.io_reg_peek(MAR1H as u8), 0x03);
}

#[test]
fn dma0_has_priority_over_dma1_when_both_requests_are_ready() {
    let mut cpu = recording_machine(Variant::Z80180);
    cpu.mem_poke(0x0300, 0xa0);
    cpu.mem_poke(0x0400, 0xb1);
    cpu.write_internal_io(SAR0L, 0x00);
    cpu.write_internal_io(SAR0H, 0x03);
    cpu.write_internal_io(DAR0L, 0x11);
    cpu.write_internal_io(DAR0H, 0x11);
    cpu.write_internal_io(BCR0L, 0x01);
    cpu.write_internal_io(DMODE, 0x30);
    cpu.write_internal_io(MAR1L, 0x00);
    cpu.write_internal_io(MAR1H, 0x04);
    cpu.write_internal_io(IAR1L, 0x22);
    cpu.write_internal_io(IAR1H, 0x22);
    cpu.write_internal_io(BCR1L, 0x01);
    cpu.write_internal_io(DCNTL, 0x00);
    cpu.write_internal_io(DSTAT, 0xc0);
    cpu.set_dreq(0, true);
    cpu.set_dreq(1, true);

    assert_eq!(cpu.step(), 10);
    assert_eq!(cpu.bus.writes, vec![(0x1111, 0xa0)]);
    assert_eq!(cpu.io_reg_peek(DSTAT as u8) & 0xc0, 0x80);
    assert_eq!(cpu.step(), 10);
    assert_eq!(cpu.bus.writes, vec![(0x1111, 0xa0), (0x2222, 0xb1)]);
    assert_eq!(cpu.io_reg_peek(DSTAT as u8) & 0xc0, 0x00);
}

#[test]
fn nmi_stops_dma_until_de_is_rewritten_and_reset_preserves_progress() {
    let mut cpu = machine();
    cpu.mem_poke(0x0100, 0xc1);
    cpu.mem_poke(0x0101, 0xc2);
    cpu.write_internal_io(SAR0L, 0x00);
    cpu.write_internal_io(SAR0H, 0x01);
    cpu.write_internal_io(DAR0L, 0x00);
    cpu.write_internal_io(DAR0H, 0x02);
    cpu.write_internal_io(BCR0L, 0x02);
    cpu.write_internal_io(DMODE, 0x00);
    cpu.write_internal_io(DCNTL, 0x00);
    cpu.write_internal_io(DSTAT, 0x60);

    cpu.set_nmi(true);
    assert_eq!(cpu.io_reg_peek(DSTAT as u8), 0x70, "NMI clears only DME");
    assert_eq!(cpu.step(), 11);
    assert_eq!(cpu.mem_peek(0x0200), 0x00);
    assert_eq!(cpu.io_reg_peek(BCR0L as u8), 0x02);

    cpu.set_nmi(false);
    cpu.write_internal_io(DSTAT, 0x60);
    assert_eq!(cpu.step(), 9);
    assert_eq!(cpu.mem_peek(0x0200), 0xc1);
    assert_eq!(cpu.io_reg_peek(BCR0L as u8), 0x01);

    let preserved = [
        cpu.io_reg_peek(SAR0L as u8),
        cpu.io_reg_peek(SAR0H as u8),
        cpu.io_reg_peek(DAR0L as u8),
        cpu.io_reg_peek(DAR0H as u8),
        cpu.io_reg_peek(BCR0L as u8),
    ];
    cpu.reset();
    assert_eq!(cpu.io_reg_peek(DSTAT as u8), 0x30);
    assert_eq!(
        [
            cpu.io_reg_peek(SAR0L as u8),
            cpu.io_reg_peek(SAR0H as u8),
            cpu.io_reg_peek(DAR0L as u8),
            cpu.io_reg_peek(DAR0H as u8),
            cpu.io_reg_peek(BCR0L as u8),
        ],
        preserved
    );
}

#[test]
fn ioregs_decode_relocation_and_duplicate_bus_cycles_match_um0050() {
    let mut cpu = recording_machine(Variant::Z80180);
    cpu.mem_poke(0, 0xed);
    cpu.mem_poke(1, 0x01);
    cpu.mem_poke(2, 0x3f);
    cpu.set_reg(Reg::BC, 0x4000);
    cpu.step();
    assert_eq!(cpu.io_reg_peek(ICR as u8), 0x40);
    assert_eq!(cpu.bus.writes, vec![(0x003f, 0x40)]);

    cpu.mem_poke(3, 0xed);
    cpu.mem_poke(4, 0x01);
    cpu.mem_poke(5, 0x72);
    cpu.set_reg(Reg::BC, 0x5a00);
    cpu.step();
    assert_eq!(cpu.io_reg_peek(DCNTL as u8), 0x5a);
    assert_eq!(cpu.bus.writes[1], (0x0072, 0x5a));

    cpu.mem_poke(6, 0xed);
    cpu.mem_poke(7, 0x01);
    cpu.mem_poke(8, 0x32);
    cpu.set_reg(Reg::BC, 0xa500);
    cpu.step();
    assert_eq!(cpu.io_reg_peek(DCNTL as u8), 0x5a, "old window is external");
    assert_eq!(cpu.bus.writes[2], (0x0032, 0xa5));

    cpu.mem_poke(9, 0xed);
    cpu.mem_poke(10, 0x41);
    cpu.set_reg(Reg::BC, 0xa572);
    cpu.step();
    assert_eq!(
        cpu.io_reg_peek(DCNTL as u8),
        0x5a,
        "nonzero high byte is external"
    );
    assert_eq!(cpu.bus.writes[3], (0xa572, 0xa5));
}

#[test]
fn ioregs_icr_relocation_round_trip_uses_each_active_window() {
    let mut cpu = recording_machine(Variant::Z80180);
    for (address, bytes) in [
        (0_u32, [0xed, 0x01, 0x3f]),
        (3, [0xed, 0x01, 0x7f]),
        (6, [0xed, 0x01, 0x32]),
        (9, [0xed, 0x01, 0x72]),
    ] {
        for (offset, byte) in bytes.into_iter().enumerate() {
            cpu.mem_poke(address + offset as u32, byte);
        }
    }

    cpu.set_reg(Reg::BC, 0x4000);
    assert_ne!(cpu.step(), 0);
    assert_eq!(cpu.io_reg_peek(ICR as u8), 0x40);

    cpu.set_reg(Reg::BC, 0x0000);
    assert_ne!(cpu.step(), 0);
    assert_eq!(cpu.io_reg_peek(ICR as u8), 0x00);

    cpu.set_reg(Reg::BC, 0xa500);
    assert_ne!(cpu.step(), 0);
    assert_eq!(cpu.io_reg_peek(DCNTL as u8), 0xa5);

    cpu.set_reg(Reg::BC, 0x5a00);
    assert_ne!(cpu.step(), 0);
    assert_eq!(cpu.io_reg_peek(DCNTL as u8), 0xa5);
    assert_eq!(
        cpu.bus.writes,
        vec![
            (0x003f, 0x40),
            (0x007f, 0x00),
            (0x0032, 0xa5),
            (0x0072, 0x5a)
        ]
    );
}

#[test]
fn ioregs_in0_tstio_and_otim_use_internal_data_and_duplicate_the_bus() {
    let mut input = recording_machine(Variant::Z80180);
    input.bus.read_value = 0x55;
    input.io_regs[DCNTL] = 0xa5;
    input.mem_poke(0, 0xed);
    input.mem_poke(1, 0x00);
    input.mem_poke(2, 0x32);
    input.step();
    assert_eq!(input.reg(Reg::BC).to_be_bytes()[0], 0xa5);
    assert_eq!(input.bus.reads, vec![0x0032]);

    let mut tstio = recording_machine(Variant::Z80180);
    tstio.bus.read_value = 0xff;
    tstio.io_regs[DCNTL] = 0x81;
    tstio.mem_poke(0, 0xed);
    tstio.mem_poke(1, 0x74);
    tstio.mem_poke(2, 0x80);
    tstio.set_reg(Reg::BC, 0x0032);
    tstio.step();
    assert_eq!(
        tstio.reg(Reg::AF).to_be_bytes()[1] & (FLAG_S | FLAG_Z),
        FLAG_S
    );
    assert_eq!(tstio.bus.reads, vec![0x0032]);

    let mut otim = recording_machine(Variant::Z80180);
    otim.mem_poke(0, 0xed);
    otim.mem_poke(1, 0x83);
    otim.mem_poke(0x2000, 0x60);
    otim.set_reg(Reg::BC, 0x0132);
    otim.set_reg(Reg::HL, 0x2000);
    assert_eq!(otim.step(), 23, "internal OTIM has no external-I/O waits");
    assert_eq!(otim.io_reg_peek(DCNTL as u8), 0x60);
    assert_eq!(otim.bus.writes, vec![(0x0032, 0x60)]);
}

#[test]
fn ioregs_internal_cycles_do_not_receive_external_io_waits() {
    let mut internal_in = recording_machine(Variant::Z80180);
    internal_in.mem_poke(0, 0xed);
    internal_in.mem_poke(1, 0x00);
    internal_in.mem_poke(2, 0x33);
    assert_eq!(internal_in.step(), 21);

    let mut external_in = recording_machine(Variant::Z80180);
    external_in.mem_poke(0, 0xed);
    external_in.mem_poke(1, 0x00);
    external_in.mem_poke(2, 0x40);
    assert_eq!(external_in.step(), 25);

    let mut internal_out = recording_machine(Variant::Z80180);
    internal_out.mem_poke(0, 0xed);
    internal_out.mem_poke(1, 0x01);
    internal_out.mem_poke(2, 0x33);
    assert_eq!(internal_out.step(), 22);

    let mut external_out = recording_machine(Variant::Z80180);
    external_out.mem_poke(0, 0xed);
    external_out.mem_poke(1, 0x01);
    external_out.mem_poke(2, 0x40);
    assert_eq!(external_out.step(), 26);
}
