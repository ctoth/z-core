use crate::*;

impl<B: HostBus> Z180<B> {
    pub fn asci_rx_push(&mut self, ch: usize, byte: u8) -> bool {
        if ch >= 2 || !self.asci_receiver_enabled(ch) || self.asci_rx_shift[ch].is_some() {
            return false;
        }

        self.asci_rx_shift[ch] = Some(byte);
        if let Some(cycles) = self.asci_frame_cycles(ch) {
            self.asci_rx_cycles[ch] = cycles;
            self.asci_rx_clocked[ch] = true;
        } else {
            self.asci_rx_cycles[ch] = 0;
            self.asci_rx_clocked[ch] = false;
        }
        true
    }
    pub fn asci_tx_pop(&mut self, ch: usize) -> Option<u8> {
        self.asci_tx_output.get_mut(ch)?.pop_front()
    }
    pub fn set_asci_cts(&mut self, ch: usize, level: bool) {
        let Some(cts) = self.asci_cts.get_mut(ch) else {
            return;
        };
        *cts = level;
        self.update_asci_interrupt_requests();
    }
    pub fn set_asci_dcd(&mut self, ch: usize, level: bool) {
        if ch != 0 || self.asci_dcd[0] == level {
            return;
        }

        let previous = self.asci_dcd[0];
        self.asci_dcd[0] = level;
        if !previous && level {
            self.asci_dcd_latched = true;
            self.asci_dcd_irq_pending = true;
            if self.asci_dcd_auto_enabled() {
                self.abort_asci_receive(0, true);
            }
        }
        self.update_asci_interrupt_requests();
    }
    pub(crate) fn asci_cntlb_value(&self, index: usize) -> u8 {
        let channel = index - CNTLB0;
        let cts_visible = channel == 0 || self.io_regs[STAT1] & 0x04 != 0;
        (self.io_regs[index] & !0x20)
            | if cts_visible && self.asci_cts[channel] {
                0x20
            } else {
                0
            }
    }
    pub(crate) fn asci_status_value(&self, channel: usize) -> u8 {
        let index = STAT0 + channel;
        let mut value = self.io_regs[index] & !0x06;
        if channel == 0 && self.asci_dcd_latched {
            value |= 0x04;
        } else if channel == 1 {
            value |= self.io_regs[STAT1] & 0x04;
        }
        if !self.asci_tdr_full[channel] && !self.asci_cts_hides_tdre(channel) {
            value |= 0x02;
        }
        value
    }
    fn asci_cts_hides_tdre(&self, channel: usize) -> bool {
        if !self.asci_cts[channel] {
            return false;
        }
        if channel == 0 {
            self.variant != Variant::Z8S180 || self.io_regs[0x12] & 0x20 == 0
        } else {
            self.io_regs[STAT1] & 0x04 != 0
        }
    }
    pub(crate) fn asci_dcd_auto_enabled(&self) -> bool {
        self.variant != Variant::Z8S180 || self.io_regs[0x12] & 0x40 == 0
    }
    fn asci_receiver_enabled(&self, channel: usize) -> bool {
        let enabled = self.io_regs[ICR] & 0x20 == 0 && self.io_regs[CNTLA0 + channel] & 0x40 != 0;
        let dcd_inhibits = channel == 0 && self.asci_dcd_auto_enabled() && self.asci_dcd_latched;
        enabled && !dcd_inhibits
    }
    pub(crate) fn asci_frame_cycles(&self, channel: usize) -> Option<u64> {
        let asci_control_a = self.io_regs[CNTLA0 + channel];
        let asci_control_b = self.io_regs[CNTLB0 + channel];
        let asext = if self.variant == Variant::Z8S180 {
            self.io_regs[0x12 + channel]
        } else {
            0
        };
        let clock_mode = if asext & 0x10 != 0 {
            1_u64
        } else if asci_control_b & 0x08 == 0 {
            16
        } else {
            64
        };
        let bit_cycles = if self.variant == Variant::Z8S180 && asext & 0x08 != 0 {
            let (low, high) = if channel == 0 {
                (ASTC0L, ASTC0H)
            } else {
                (ASTC1L, ASTC1H)
            };
            let time_constant =
                u64::from(u16::from_le_bytes([self.io_regs[low], self.io_regs[high]]));
            2 * (time_constant + 2) * clock_mode
        } else {
            let divisor = asci_control_b & 0x07;
            if divisor == 0x07 {
                return None;
            }
            let prescale = if asci_control_b & 0x20 == 0 {
                10_u64
            } else {
                30
            };
            prescale * (1_u64 << divisor) * clock_mode
        };

        let data_bits = if asci_control_a & 0x04 != 0 { 8_u64 } else { 7 };
        let parity_or_mp = u64::from(asci_control_a & 0x02 != 0 || asci_control_b & 0x40 != 0);
        let stop_bits = if asci_control_a & 0x01 != 0 { 2_u64 } else { 1 };
        Some((1 + data_bits + parity_or_mp + stop_bits) * bit_cycles)
    }
    pub(crate) fn sync_asci_status(&mut self, channel: usize) {
        let index = STAT0 + channel;
        if self.asci_rx_fifo[channel].is_empty() {
            self.io_regs[index] &= !0x80;
        } else {
            self.io_regs[index] |= 0x80;
        }
        if self.asci_tdr_full[channel] {
            self.io_regs[index] &= !0x02;
        } else {
            self.io_regs[index] |= 0x02;
        }
    }
    pub(crate) fn update_asci_interrupt_requests(&mut self) {
        self.internal_irq_pending &= !0x60;
        for channel in 0..2 {
            let status = self.io_regs[STAT0 + channel];
            let mut receive_cause = status & 0x70 != 0;
            let rdrf_interrupt_enabled =
                self.variant != Variant::Z8S180 || self.io_regs[0x12 + channel] & 0x80 != 0;
            receive_cause |= status & 0x80 != 0 && rdrf_interrupt_enabled;
            if channel == 0 {
                receive_cause |= self.asci_dcd_irq_pending;
            }
            let receive_request = status & 0x08 != 0 && receive_cause;
            let transmit_request =
                status & 0x01 != 0 && self.asci_status_value(channel) & 0x02 != 0;
            if receive_request || transmit_request {
                self.internal_irq_pending |= 0x20_u8 << channel;
            }
        }
    }
    pub(crate) fn abort_asci_receive(&mut self, channel: usize, clear_status: bool) {
        self.asci_rx_shift[channel] = None;
        self.asci_rx_cycles[channel] = 0;
        self.asci_rx_clocked[channel] = false;
        if clear_status {
            self.asci_rx_fifo[channel].clear();
            self.io_regs[STAT0 + channel] &= !0xf0;
        }
        self.sync_asci_status(channel);
    }
    pub(crate) fn start_asci_transmit(&mut self, channel: usize) {
        if self.asci_tx_shift[channel].is_some()
            || !self.asci_tdr_full[channel]
            || self.io_regs[ICR] & 0x20 != 0
            || self.io_regs[CNTLA0 + channel] & 0x20 == 0
        {
            return;
        }

        self.asci_tx_shift[channel] = Some(self.io_regs[TDR0 + channel]);
        self.asci_tdr_full[channel] = false;
        if let Some(cycles) = self.asci_frame_cycles(channel) {
            self.asci_tx_cycles[channel] = cycles;
            self.asci_tx_clocked[channel] = true;
        } else {
            self.asci_tx_cycles[channel] = 0;
            self.asci_tx_clocked[channel] = false;
        }
    }
    pub(crate) fn apply_asci_cntla_write(&mut self, channel: usize, old: u8) {
        if self.io_regs[ICR] & 0x20 != 0 {
            self.io_regs[CNTLA0 + channel] &= !0x60;
        }
        let next = self.io_regs[CNTLA0 + channel];
        if next & 0x08 == 0 {
            self.io_regs[STAT0 + channel] &= !0x70;
        }
        if old & 0x40 != 0 && next & 0x40 == 0 {
            self.abort_asci_receive(channel, false);
        }
        if old & 0x20 != 0 && next & 0x20 == 0 {
            self.asci_tx_shift[channel] = None;
            self.asci_tx_cycles[channel] = 0;
            self.asci_tx_clocked[channel] = false;
        }
        self.start_asci_transmit(channel);
        self.sync_asci_status(channel);
        self.update_asci_interrupt_requests();
    }
    pub(crate) fn stop_asci_for_iostop(&mut self) {
        for channel in 0..2 {
            self.io_regs[CNTLA0 + channel] &= !0x60;
            self.asci_tdr_full[channel] = false;
            self.asci_tx_shift[channel] = None;
            self.asci_tx_cycles[channel] = 0;
            self.asci_tx_clocked[channel] = false;
            self.abort_asci_receive(channel, true);
            self.sync_asci_status(channel);
        }
        self.update_asci_interrupt_requests();
    }
    pub(crate) fn advance_asci(&mut self, cycles: u32) {
        if self.io_regs[ICR] & 0x20 != 0 {
            self.update_asci_interrupt_requests();
            return;
        }
        for channel in 0..2 {
            let mut remaining_cycles = u64::from(cycles);
            while self.asci_tx_shift[channel].is_some() && self.asci_tx_clocked[channel] {
                if self.asci_tx_cycles[channel] > remaining_cycles {
                    self.asci_tx_cycles[channel] -= remaining_cycles;
                    break;
                }
                remaining_cycles -= self.asci_tx_cycles[channel];
                let byte = self.asci_tx_shift[channel]
                    .take()
                    .expect("ASCI transmit shift register was checked as full");
                self.asci_tx_output[channel].push_back(byte);
                self.asci_tx_cycles[channel] = 0;
                self.asci_tx_clocked[channel] = false;
                self.start_asci_transmit(channel);
            }

            if self.asci_rx_shift[channel].is_some() && self.asci_rx_clocked[channel] {
                if self.asci_rx_cycles[channel] > u64::from(cycles) {
                    self.asci_rx_cycles[channel] -= u64::from(cycles);
                } else {
                    let byte = self.asci_rx_shift[channel]
                        .take()
                        .expect("ASCI receive shift register was checked as full");
                    self.asci_rx_cycles[channel] = 0;
                    self.asci_rx_clocked[channel] = false;
                    let fifo_capacity = if self.variant == Variant::Z8S180 {
                        4
                    } else {
                        1
                    };
                    if self.asci_rx_fifo[channel].len() == fifo_capacity {
                        self.io_regs[STAT0 + channel] |= 0x40;
                    } else {
                        let was_empty = self.asci_rx_fifo[channel].is_empty();
                        self.asci_rx_fifo[channel].push_back(byte);
                        if was_empty {
                            self.io_regs[RDR0 + channel] = byte;
                        }
                    }
                }
            }
            self.sync_asci_status(channel);
        }
        self.update_asci_interrupt_requests();
    }
}
