use crate::*;

impl<B: HostBus> Z180<B> {
    pub fn csio_rx_push(&mut self, byte: u8) -> bool {
        if self.io_regs[ICR] & 0x20 != 0
            || self.io_regs[CNTR] & 0x20 == 0
            || self.io_regs[STAT1] & 0x04 != 0
            || self.csio_rx_shift.is_some()
        {
            return false;
        }

        self.csio_rx_shift = Some(byte);
        if let Some(cycles) = self.csio_transfer_cycles() {
            self.csio_cycles = cycles;
            self.csio_clocked = true;
        } else {
            self.csio_cycles = 0;
            self.csio_clocked = false;
        }
        true
    }
    pub fn csio_tx_pop(&mut self) -> Option<u8> {
        self.csio_tx_output.pop_front()
    }
    fn csio_transfer_cycles(&self) -> Option<u64> {
        let speed = self.io_regs[CNTR] & 0x07;
        if speed == 0x07 {
            None
        } else {
            Some(8 * (20_u64 << speed))
        }
    }
    pub(crate) fn update_csio_interrupt_request(&mut self) {
        self.internal_irq_pending &= !0x10;
        if self.io_regs[CNTR] & 0xc0 == 0xc0 {
            self.internal_irq_pending |= 0x10;
        }
    }
    pub(crate) fn abort_csio_receive(&mut self) {
        self.io_regs[CNTR] &= !0x20;
        self.csio_rx_shift = None;
        self.csio_cycles = 0;
        self.csio_clocked = false;
    }
    pub(crate) fn apply_csio_cntr_write(&mut self, old: u8) {
        if self.io_regs[ICR] & 0x20 != 0 {
            self.io_regs[CNTR] &= !0xb0;
        }
        if self.io_regs[CNTR] & 0x30 == 0x30 {
            self.io_regs[CNTR] &= !0x30;
        }
        if self.io_regs[STAT1] & 0x04 != 0 {
            self.io_regs[CNTR] &= !0x20;
        }
        let next = self.io_regs[CNTR];

        if old & 0x20 != 0 && next & 0x20 == 0 {
            self.abort_csio_receive();
        }
        if old & 0x10 != 0 && next & 0x10 == 0 {
            self.csio_cycles = 0;
            self.csio_clocked = false;
        }
        if next & 0x20 != 0 && old & 0x20 == 0 {
            self.csio_cycles = 0;
            self.csio_clocked = false;
            self.csio_rx_shift = None;
        }
        if next & 0x10 != 0 && old & 0x10 == 0 {
            self.csio_rx_shift = None;
            if let Some(cycles) = self.csio_transfer_cycles() {
                self.csio_cycles = cycles;
                self.csio_clocked = true;
            } else {
                self.csio_cycles = 0;
                self.csio_clocked = false;
            }
        }
        self.update_csio_interrupt_request();
    }
    pub(crate) fn stop_csio_for_iostop(&mut self) {
        self.io_regs[CNTR] &= !0xb0;
        self.csio_rx_shift = None;
        self.csio_cycles = 0;
        self.csio_clocked = false;
        self.update_csio_interrupt_request();
    }
    pub(crate) fn advance_csio(&mut self, cycles: u32) {
        if self.io_regs[ICR] & 0x20 != 0 || !self.csio_clocked {
            return;
        }
        if self.csio_cycles > u64::from(cycles) {
            self.csio_cycles -= u64::from(cycles);
            return;
        }

        self.csio_cycles = 0;
        self.csio_clocked = false;
        if self.io_regs[CNTR] & 0x10 != 0 {
            self.csio_tx_output.push_back(self.io_regs[TRD]);
            self.io_regs[CNTR] &= !0x10;
            self.io_regs[CNTR] |= 0x80;
        } else if self.io_regs[CNTR] & 0x20 != 0 {
            let Some(byte) = self.csio_rx_shift.take() else {
                return;
            };
            self.io_regs[TRD] = byte;
            self.io_regs[CNTR] &= !0x20;
            self.io_regs[CNTR] |= 0x80;
        }
        self.update_csio_interrupt_request();
    }
}
