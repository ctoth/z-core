use crate::*;

impl<B: HostBus> Z180<B> {
    pub(crate) fn update_prt_interrupt_requests(&mut self) {
        self.internal_irq_pending &= !0x03;
        if self.io_regs[TCR] & 0x50 == 0x50 {
            self.internal_irq_pending |= 0x01;
        }
        if self.io_regs[TCR] & 0xa0 == 0xa0 {
            self.internal_irq_pending |= 0x02;
        }
    }
    pub(crate) fn advance_prt(&mut self, cycles: u32) {
        let total_cycles = self.prt_cycle_remainder.saturating_add(cycles);
        let ticks = total_cycles / 20;
        self.prt_cycle_remainder = total_cycles % 20;

        for _ in 0..ticks {
            for channel in 0..2 {
                if self.io_regs[TCR] & (1_u8 << channel) == 0 {
                    continue;
                }

                let (tmdr_low, tmdr_high, rldr_low, rldr_high, flag) = if channel == 0 {
                    (TMDR0L, TMDR0H, RLDR0L, RLDR0H, 0x40)
                } else {
                    (TMDR1L, TMDR1H, RLDR1L, RLDR1H, 0x80)
                };
                let count = u16::from_le_bytes([self.io_regs[tmdr_low], self.io_regs[tmdr_high]]);
                let next = if count == 0 {
                    u16::from_le_bytes([self.io_regs[rldr_low], self.io_regs[rldr_high]])
                } else {
                    let decremented = count - 1;
                    if decremented == 0 {
                        self.io_regs[TCR] |= flag;
                    }
                    decremented
                };
                let [low, high] = next.to_le_bytes();
                self.io_regs[tmdr_low] = low;
                self.io_regs[tmdr_high] = high;
            }
        }

        if ticks != 0 {
            self.update_prt_interrupt_requests();
        }
    }
}
