use super::*;

#[test]
fn prt_both_channels_tick_at_phi_divided_by_twenty_and_reload_after_zero() {
    let mut cpu = machine();
    cpu.write_internal_io(TMDR0L, 0x02);
    cpu.write_internal_io(TMDR0H, 0x00);
    cpu.write_internal_io(RLDR0L, 0x03);
    cpu.write_internal_io(RLDR0H, 0x00);
    cpu.write_internal_io(TMDR1L, 0x01);
    cpu.write_internal_io(TMDR1H, 0x00);
    cpu.write_internal_io(RLDR1L, 0x04);
    cpu.write_internal_io(RLDR1H, 0x00);
    cpu.write_internal_io(TCR, 0x03);

    assert_eq!(cpu.finish_step(19), 19);
    assert_eq!(cpu.io_reg_peek(TMDR0L as u8), 0x02);
    assert_eq!(cpu.io_reg_peek(TMDR1L as u8), 0x01);
    assert_eq!(cpu.io_reg_peek(TCR as u8), 0x03);

    assert_eq!(cpu.finish_step(1), 1);
    assert_eq!(cpu.io_reg_peek(TMDR0L as u8), 0x01);
    assert_eq!(cpu.io_reg_peek(TMDR1L as u8), 0x00);
    assert_eq!(cpu.io_reg_peek(TCR as u8), 0x83, "PRT1 reaches zero");

    assert_eq!(cpu.finish_step(20), 20);
    assert_eq!(cpu.io_reg_peek(TMDR0L as u8), 0x00);
    assert_eq!(cpu.io_reg_peek(TMDR1L as u8), 0x04);
    assert_eq!(cpu.io_reg_peek(TCR as u8), 0xc3, "PRT0 reaches zero");

    assert_eq!(cpu.finish_step(20), 20);
    assert_eq!(cpu.io_reg_peek(TMDR0L as u8), 0x03);
    assert_eq!(cpu.io_reg_peek(TMDR1L as u8), 0x03);
}

#[test]
fn prt_tmdr_writes_require_the_corresponding_channel_to_be_stopped() {
    let mut cpu = machine();
    for (channel, low, high) in [(0_u8, TMDR0L, TMDR0H), (1, TMDR1L, TMDR1H)] {
        cpu.write_internal_io(low, 0x34);
        cpu.write_internal_io(high, 0x12);
        cpu.write_internal_io(TCR, 1 << channel);
        cpu.write_internal_io(low, 0xcd);
        cpu.write_internal_io(high, 0xab);

        assert_eq!(cpu.io_reg_peek(low as u8), 0x34, "channel {channel} low");
        assert_eq!(cpu.io_reg_peek(high as u8), 0x12, "channel {channel} high");

        cpu.write_internal_io(TCR, 0x00);
    }
}

#[test]
fn prt_low_byte_read_latches_the_simultaneous_high_byte() {
    for (channel, low, high) in [(0_u8, TMDR0L, TMDR0H), (1, TMDR1L, TMDR1H)] {
        let mut cpu = machine();
        cpu.write_internal_io(low, 0x00);
        cpu.write_internal_io(high, 0x13);

        assert_eq!(cpu.read_internal_io(low), 0x00, "channel {channel} low");
        cpu.write_internal_io(TCR, 1 << channel);
        cpu.finish_step(20);
        assert_eq!(
            cpu.read_internal_io(high),
            0x13,
            "channel {channel} latched high"
        );
        assert_eq!(
            cpu.read_internal_io(high),
            0x12,
            "channel {channel} live high"
        );
    }
}

#[test]
fn prt_tif_clear_requires_tcr_then_the_corresponding_tmdr_read() {
    let mut cpu = machine();
    cpu.write_internal_io(TMDR0L, 0x01);
    cpu.write_internal_io(TMDR0H, 0x00);
    cpu.write_internal_io(TMDR1L, 0x01);
    cpu.write_internal_io(TMDR1H, 0x00);
    cpu.write_internal_io(TCR, 0x33);
    cpu.finish_step(20);

    assert_eq!(cpu.io_reg_peek(TCR as u8), 0xf3);
    assert_eq!(cpu.internal_irq_pending & 0x03, 0x03);
    assert_eq!(cpu.read_internal_io(TMDR0L), 0x00);
    assert_eq!(cpu.io_reg_peek(TCR as u8), 0xf3, "TMDR read alone");

    assert_eq!(cpu.read_internal_io(TCR), 0xf3);
    assert_eq!(cpu.read_internal_io(TMDR0H), 0x00);
    assert_eq!(cpu.io_reg_peek(TCR as u8), 0xb3, "only TIF0 clears");
    assert_eq!(cpu.internal_irq_pending & 0x03, 0x02);

    assert_eq!(cpu.read_internal_io(TMDR1L), 0x00);
    assert_eq!(cpu.io_reg_peek(TCR as u8), 0x33, "TIF1 then clears");
    assert_eq!(cpu.internal_irq_pending & 0x03, 0x00);
}

#[test]
fn prt_each_channel_delivers_its_internal_interrupt_from_halt() {
    for (channel, low, high, tcr, flag, pending, vector) in [
        (0_u8, TMDR0L, TMDR0H, 0x11_u8, 0x40_u8, 0x01_u8, 0x20a4_u32),
        (1, TMDR1L, TMDR1H, 0x22, 0x80, 0x02, 0x20a6),
    ] {
        let mut cpu = machine();
        cpu.write_internal_io(DCNTL, 0x00);
        cpu.write_internal_io(IL, 0xa0);
        cpu.write_internal_io(low, 0x01);
        cpu.write_internal_io(high, 0x00);
        cpu.write_internal_io(TCR, tcr);
        cpu.mem_poke(0, 0x76);
        cpu.mem_poke(vector, 0x56);
        cpu.mem_poke(vector + 1, 0x34);
        cpu.set_reg(Reg::IR, 0x2000);
        cpu.set_reg(Reg::SP, 0x8000);
        cpu.set_iff1(true);

        assert_eq!(cpu.step(), 3, "channel {channel} enters HALT");
        for _ in 0..5 {
            assert_eq!(cpu.step(), 3, "channel {channel} HALT idle");
        }
        assert_eq!(cpu.internal_irq_pending & pending, 0);
        assert_eq!(cpu.step(), 3, "channel {channel} reaches timer tick");
        assert_eq!(cpu.io_reg_peek(TCR as u8) & flag, flag);
        assert_eq!(cpu.internal_irq_pending & pending, pending);

        assert_eq!(cpu.step(), 18, "channel {channel} acknowledges PRT IRQ");
        assert_eq!(cpu.reg(Reg::PC), 0x3456, "channel {channel} vector");
        assert_eq!(cpu.reg(Reg::SP), 0x7ffe);
        assert_eq!(cpu.mem_peek(0x7ffe), 0x01);
        assert_eq!(cpu.mem_peek(0x7fff), 0x00);
    }
}

#[test]
fn frc_counts_down_once_per_ten_phi_cycles_and_wraps() {
    let mut cpu = machine();
    assert_eq!(cpu.io_reg_peek(FRC as u8), 0xff);

    assert_eq!(cpu.finish_step(9), 9);
    assert_eq!(cpu.read_internal_io(FRC), 0xff);
    assert_eq!(cpu.read_internal_io(FRC), 0xff, "reads do not change FRC");

    assert_eq!(cpu.finish_step(1), 1);
    assert_eq!(cpu.io_reg_peek(FRC as u8), 0xfe);
    assert_eq!(cpu.finish_step(2_540), 2_540);
    assert_eq!(cpu.io_reg_peek(FRC as u8), 0x00);
    assert_eq!(cpu.finish_step(10), 10);
    assert_eq!(cpu.io_reg_peek(FRC as u8), 0xff, "zero wraps to FFh");
}

#[test]
fn frc_is_read_only_and_continues_in_io_stop() {
    let mut cpu = machine();
    cpu.write_internal_io(FRC, 0x12);
    assert_eq!(cpu.io_reg_peek(FRC as u8), 0xff, "FRC write is ignored");

    cpu.write_internal_io(ICR, 0x20);
    assert_eq!(cpu.io_reg_peek(ICR as u8), 0x20, "I/O STOP is active");
    cpu.finish_step(10);
    assert_eq!(cpu.io_reg_peek(FRC as u8), 0xfe);
}

#[test]
fn frc_reset_restores_ff_and_restarts_the_divide_by_ten_phase() {
    let mut cpu = machine();
    cpu.finish_step(9);
    cpu.reset();

    assert_eq!(cpu.io_reg_peek(FRC as u8), 0xff);
    cpu.finish_step(1);
    assert_eq!(cpu.io_reg_peek(FRC as u8), 0xff);
    cpu.finish_step(9);
    assert_eq!(cpu.io_reg_peek(FRC as u8), 0xfe);
}
