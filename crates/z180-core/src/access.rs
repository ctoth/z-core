use super::*;

impl<B: HostBus> Z180<B> {
    pub(super) fn read_logical(&mut self, logical: u16) -> u8 {
        self.timing_memory_waits = self
            .timing_memory_waits
            .saturating_add(u32::from((self.io_regs[DCNTL] >> 6) & 0x03));
        let physical = self.map_external_address(self.mmu_translate(logical));
        let value = self.emulation_mem_read(physical);
        self.capture_insn_byte(logical, value);
        value
    }
    pub(super) fn write_logical(&mut self, logical: u16, value: u8) {
        self.timing_memory_waits = self
            .timing_memory_waits
            .saturating_add(u32::from((self.io_regs[DCNTL] >> 6) & 0x03));
        let physical = self.map_external_address(self.mmu_translate(logical));
        self.emulation_mem_write(physical, value);
    }
    fn map_external_address(&self, physical: u32) -> u32 {
        match &self.ext_mapper {
            Some(ExtMapper::Function(mapper)) => mapper(physical),
            Some(ExtMapper::Table(table)) => {
                table.get(physical as usize).copied().unwrap_or(physical)
            }
            None => physical,
        }
    }
    pub(super) fn emulation_mem_read(&mut self, physical: u32) -> u8 {
        if self.bus_error.is_some() {
            return 0;
        }
        let value = match self.memory.read(&mut self.bus, physical) {
            Ok(value) => value,
            Err(error) => {
                self.bus_error = Some(error);
                return 0;
            }
        };
        if self.mem_watch_matches(physical, WatchKind::Read) {
            self.push_event(Event::MemRead {
                cycle: self.cycle_count,
                pc: self.instruction_pc,
                phys: physical,
                val: value,
            });
        }
        value
    }
    pub(super) fn emulation_mem_write(&mut self, physical: u32, value: u8) {
        if self.bus_error.is_some() {
            return;
        }
        let rom_write = match self.memory.write(&mut self.bus, physical, value) {
            Ok(rom_write) => rom_write,
            Err(error) => {
                self.bus_error = Some(error);
                return;
            }
        };
        if self.mem_watch_matches(physical, WatchKind::Write) {
            self.push_event(Event::MemWrite {
                cycle: self.cycle_count,
                pc: self.instruction_pc,
                phys: physical,
                val: value,
            });
        }
        if rom_write {
            self.push_event(Event::RomWrite {
                cycle: self.cycle_count,
                pc: self.instruction_pc,
                phys: physical,
                val: value,
            });
        }
    }
    fn mem_watch_matches(&self, physical: u32, access: WatchKind) -> bool {
        self.mem_watches.iter().any(|watch| {
            let in_range = physical >= watch.base && physical - watch.base < watch.size;
            let kind_matches = matches!(
                (watch.kind, access),
                (WatchKind::Read | WatchKind::Both, WatchKind::Read)
                    | (WatchKind::Write | WatchKind::Both, WatchKind::Write)
            );
            in_range && kind_matches
        })
    }
    pub(super) fn internal_io_index(&self, port: u16) -> Option<usize> {
        let [high, low] = port.to_be_bytes();
        let base = self.io_regs[ICR] & 0xc0;
        if high == 0 && low >= base && low <= base | 0x3f {
            Some(usize::from(low - base))
        } else {
            None
        }
    }
    pub(super) fn read_io(&mut self, port: u16) -> u8 {
        if self.bus_error.is_some() {
            return 0;
        }
        let value = if let Some(index) = self.internal_io_index(port) {
            if let Err(error) = self.bus.io_read(port) {
                self.bus_error = Some(error);
                return 0;
            }
            self.read_internal_io(index)
        } else {
            self.timing_io_waits = self
                .timing_io_waits
                .saturating_add(u32::from(((self.io_regs[DCNTL] >> 4) & 0x03) + 1));
            match self.bus.io_read(port) {
                Ok(value) => value,
                Err(error) => {
                    self.bus_error = Some(error);
                    return 0;
                }
            }
        };
        if self.io_trace {
            self.push_event(Event::IoRead {
                cycle: self.cycle_count,
                pc: self.instruction_pc,
                port,
                val: value,
            });
        }
        value
    }
    pub(super) fn read_internal_io(&mut self, index: usize) -> u8 {
        let spec = IO_REG_SPECS[index];
        let value = self.io_reg_peek(index as u8);
        match spec.read_effect {
            ReadEffect::AsciCntlb | ReadEffect::None => value,
            ReadEffect::AsciStat => {
                if index == STAT0 {
                    self.asci_dcd_irq_pending = false;
                    if !self.asci_dcd[0] && self.asci_dcd_latched {
                        self.asci_dcd_latched = false;
                    }
                    self.update_asci_interrupt_requests();
                }
                value
            }
            ReadEffect::AsciRdr => {
                let channel = index - RDR0;
                let _ = self.asci_rx_fifo[channel].pop_front();
                if let Some(next) = self.asci_rx_fifo[channel].front().copied() {
                    self.io_regs[index] = next;
                }
                self.sync_asci_status(channel);
                self.update_asci_interrupt_requests();
                value
            }
            ReadEffect::CsioTrd => {
                self.io_regs[CNTR] &= !0x80;
                self.update_csio_interrupt_request();
                value
            }
            ReadEffect::Tcr => {
                self.prt_clear_armed = (value >> 6) & 0x03;
                value
            }
            ReadEffect::TmdrLow | ReadEffect::TmdrHigh => {
                let channel = usize::from(index >= TMDR1L);
                let result = if spec.read_effect == ReadEffect::TmdrLow {
                    let high_index = if channel == 0 { TMDR0H } else { TMDR1H };
                    self.prt_high_latch[channel] = self.io_regs[high_index];
                    self.prt_high_latch_valid[channel] = true;
                    value
                } else if self.prt_high_latch_valid[channel] {
                    self.prt_high_latch_valid[channel] = false;
                    self.prt_high_latch[channel]
                } else {
                    value
                };

                let channel_mask = 1_u8 << channel;
                if self.prt_clear_armed & channel_mask != 0 {
                    self.io_regs[TCR] &= !(0x40_u8 << channel);
                    self.prt_clear_armed &= !channel_mask;
                    self.update_prt_interrupt_requests();
                }
                result
            }
        }
    }
    pub(super) fn write_io(&mut self, port: u16, value: u8) {
        if self.bus_error.is_some() {
            return;
        }
        if let Some(index) = self.internal_io_index(port) {
            if let Err(error) = self.bus.io_write(port, value) {
                self.bus_error = Some(error);
                return;
            }
            self.write_internal_io(index, value);
        } else {
            self.timing_io_waits = self
                .timing_io_waits
                .saturating_add(u32::from(((self.io_regs[DCNTL] >> 4) & 0x03) + 1));
            if let Err(error) = self.bus.io_write(port, value) {
                self.bus_error = Some(error);
                return;
            }
        }
        if self.io_trace {
            self.push_event(Event::IoWrite {
                cycle: self.cycle_count,
                pc: self.instruction_pc,
                port,
                val: value,
            });
        }
    }
    pub(super) fn write_internal_io(&mut self, index: usize, value: u8) {
        let spec = IO_REG_SPECS[index];
        if !spec.is_available(self.variant) {
            return;
        }
        let old = self.io_regs[index];
        self.io_regs[index] = match spec.write_effect {
            WriteEffect::AsciAsext
            | WriteEffect::AsciCntla
            | WriteEffect::AsciCntlb
            | WriteEffect::AsciStat
            | WriteEffect::AsciTdr
            | WriteEffect::CsioCntr
            | WriteEffect::CsioTrd
            | WriteEffect::Icr
            | WriteEffect::None
            | WriteEffect::Mmu
            | WriteEffect::Tcr => (old & !spec.write_mask) | (value & spec.write_mask),
            WriteEffect::Tmdr => {
                let channel = usize::from(index >= TMDR1L);
                if self.io_regs[TCR] & (1_u8 << channel) == 0 {
                    self.prt_high_latch_valid[channel] = false;
                    (old & !spec.write_mask) | (value & spec.write_mask)
                } else {
                    old
                }
            }
            WriteEffect::Rdr => {
                let status_index = if index == 0x08 { 0x04 } else { 0x05 };
                if self.variant == Variant::Z8S180 && self.io_regs[status_index] & 0x80 != 0 {
                    old
                } else {
                    (old & !spec.write_mask) | (value & spec.write_mask)
                }
            }
            WriteEffect::Dstat => {
                let mut next = old & 0xc9;
                if value & 0x20 == 0 {
                    next = (next & !0x80) | (value & 0x80);
                    if value & 0x80 != 0 {
                        next |= 0x01;
                    }
                }
                if value & 0x10 == 0 {
                    next = (next & !0x40) | (value & 0x40);
                    if value & 0x40 != 0 {
                        next |= 0x01;
                    }
                }
                (next & !0x0c) | (value & 0x0c) | 0x30
            }
            WriteEffect::Itc => {
                let trap = old & value & 0x80;
                let ufo = old & 0x40;
                trap | ufo | (value & 0x07)
            }
        };
        if spec.write_effect == WriteEffect::Dstat {
            self.update_dma_interrupt_requests();
        } else if spec.write_effect == WriteEffect::Mmu {
            self.recompute_mmu_pages();
        } else if spec.write_effect == WriteEffect::Tcr {
            self.update_prt_interrupt_requests();
        } else if spec.write_effect == WriteEffect::AsciCntla {
            self.apply_asci_cntla_write(index - CNTLA0, old);
        } else if spec.write_effect == WriteEffect::AsciCntlb {
            let channel = index - CNTLB0;
            if self.asci_rx_shift[channel].is_some()
                && !self.asci_rx_clocked[channel]
                && let Some(cycles) = self.asci_frame_cycles(channel)
            {
                self.asci_rx_cycles[channel] = cycles;
                self.asci_rx_clocked[channel] = true;
            }
            self.update_asci_interrupt_requests();
        } else if spec.write_effect == WriteEffect::AsciStat {
            if index == STAT1 && self.io_regs[STAT1] & 0x04 != 0 {
                self.abort_csio_receive();
            }
            self.update_asci_interrupt_requests();
        } else if spec.write_effect == WriteEffect::AsciTdr {
            let channel = index - TDR0;
            self.asci_tdr_full[channel] = self.io_regs[ICR] & 0x20 == 0;
            self.start_asci_transmit(channel);
            self.sync_asci_status(channel);
            self.update_asci_interrupt_requests();
        } else if spec.write_effect == WriteEffect::AsciAsext {
            let channel = index - 0x12;
            if channel == 0 && self.asci_dcd_auto_enabled() && self.asci_dcd_latched {
                self.abort_asci_receive(0, true);
            }
            self.update_asci_interrupt_requests();
        } else if spec.write_effect == WriteEffect::CsioCntr {
            self.apply_csio_cntr_write(old);
        } else if spec.write_effect == WriteEffect::CsioTrd {
            self.io_regs[CNTR] &= !0x80;
            self.update_csio_interrupt_request();
        } else if spec.write_effect == WriteEffect::Icr && self.io_regs[ICR] & 0x20 != 0 {
            self.stop_asci_for_iostop();
            self.stop_csio_for_iostop();
        }
    }
    pub(super) fn recompute_mmu_pages(&mut self) {
        let ba = usize::from(self.io_regs[CBAR] & 0x0f);
        let ca = usize::from(self.io_regs[CBAR] >> 4);
        let bank_base = u32::from(self.io_regs[BBR]);
        let common_one_base = u32::from(self.io_regs[CBR]);

        for (page, physical_base) in self.mmu_pages.iter_mut().enumerate() {
            let relocation = if page < ba {
                0
            } else if page < ca {
                bank_base
            } else {
                common_one_base
            };
            *physical_base = ((relocation + page as u32) & 0xff) << 12;
        }
    }
}
