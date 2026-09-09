use crate::*;

impl<B: HostBus> Z180<B> {
    pub fn set_dreq(&mut self, ch: usize, level: bool) {
        let Some(current) = self.dreq_level.get_mut(ch) else {
            return;
        };
        if level && !*current {
            self.dreq_edge_pending[ch] = true;
        }
        *current = level;
    }
    fn dma0_transfer_byte(&mut self) -> u32 {
        let source_mode = (self.io_regs[DMODE] >> 2) & 0x03;
        let destination_mode = (self.io_regs[DMODE] >> 4) & 0x03;
        let source = u32::from(self.io_regs[SAR0L])
            | (u32::from(self.io_regs[SAR0H]) << 8)
            | (u32::from(self.io_regs[SAR0B] & 0x0f) << 16);
        let destination = u32::from(self.io_regs[DAR0L])
            | (u32::from(self.io_regs[DAR0H]) << 8)
            | (u32::from(self.io_regs[DAR0B] & 0x0f) << 16);

        let byte = if source_mode == 3 {
            self.dma_io_read(source as u16)
        } else {
            self.emulation_mem_read(source)
        };
        if self.bus_error.is_some() {
            return 0;
        }
        if destination_mode == 3 {
            self.dma_io_write(destination as u16, byte);
        } else {
            self.emulation_mem_write(destination, byte);
        }
        if self.bus_error.is_some() {
            return 0;
        }

        let memory_waits = u32::from((self.io_regs[DCNTL] >> 6) & 0x03);
        let io_waits = u32::from(((self.io_regs[DCNTL] >> 4) & 0x03) + 1);
        let mut cycles = 6_u32
            .saturating_add(if source_mode == 3 {
                io_waits
            } else {
                memory_waits
            })
            .saturating_add(if destination_mode == 3 {
                io_waits
            } else {
                memory_waits
            });

        let (next_source, source_crossed) = match source_mode {
            0 => (
                source.wrapping_add(1) & 0x000f_ffff,
                source & 0xffff == 0xffff,
            ),
            1 => (
                source.wrapping_sub(1) & 0x000f_ffff,
                source.trailing_zeros() >= 16,
            ),
            _ => (source, false),
        };
        let (next_destination, destination_crossed) = match destination_mode {
            0 => (
                destination.wrapping_add(1) & 0x000f_ffff,
                destination & 0xffff == 0xffff,
            ),
            1 => (
                destination.wrapping_sub(1) & 0x000f_ffff,
                destination.trailing_zeros() >= 16,
            ),
            _ => (destination, false),
        };
        if memory_waits == 0 {
            cycles = cycles
                .saturating_add(u32::from(source_crossed))
                .saturating_add(u32::from(destination_crossed));
        }

        self.io_regs[SAR0L] = next_source as u8;
        self.io_regs[SAR0H] = (next_source >> 8) as u8;
        self.io_regs[SAR0B] = (next_source >> 16) as u8 & 0x0f;
        self.io_regs[DAR0L] = next_destination as u8;
        self.io_regs[DAR0H] = (next_destination >> 8) as u8;
        self.io_regs[DAR0B] = (next_destination >> 16) as u8 & 0x0f;

        let count = u16::from_le_bytes([self.io_regs[BCR0L], self.io_regs[BCR0H]]) - 1;
        [self.io_regs[BCR0L], self.io_regs[BCR0H]] = count.to_le_bytes();
        if count == 0 {
            self.io_regs[DSTAT] &= !0x40;
            self.update_dma_interrupt_requests();
        }
        cycles
    }
    fn dma1_transfer_byte(&mut self) -> u32 {
        let mode = self.io_regs[DCNTL] & 0x03;
        let memory_to_io = mode & 0x02 == 0;
        let memory = u32::from(self.io_regs[MAR1L])
            | (u32::from(self.io_regs[MAR1H]) << 8)
            | (u32::from(self.io_regs[MAR1B] & 0x0f) << 16);
        let io = u16::from_le_bytes([self.io_regs[IAR1L], self.io_regs[IAR1H]]);

        if memory_to_io {
            let byte = self.emulation_mem_read(memory);
            if self.bus_error.is_some() {
                return 0;
            }
            self.dma_io_write(io, byte);
        } else {
            let byte = self.dma_io_read(io);
            if self.bus_error.is_some() {
                return 0;
            }
            self.emulation_mem_write(memory, byte);
        }
        if self.bus_error.is_some() {
            return 0;
        }

        let memory_waits = u32::from((self.io_regs[DCNTL] >> 6) & 0x03);
        let io_waits = u32::from(((self.io_regs[DCNTL] >> 4) & 0x03) + 1);
        let decrements = mode & 0x01 != 0;
        let crossed = if decrements {
            memory.trailing_zeros() >= 16
        } else {
            memory & 0xffff == 0xffff
        };
        let next_memory = if decrements {
            memory.wrapping_sub(1) & 0x000f_ffff
        } else {
            memory.wrapping_add(1) & 0x000f_ffff
        };
        self.io_regs[MAR1L] = next_memory as u8;
        self.io_regs[MAR1H] = (next_memory >> 8) as u8;
        self.io_regs[MAR1B] = (next_memory >> 16) as u8 & 0x0f;

        let count = u16::from_le_bytes([self.io_regs[BCR1L], self.io_regs[BCR1H]]) - 1;
        [self.io_regs[BCR1L], self.io_regs[BCR1H]] = count.to_le_bytes();
        if count == 0 {
            self.io_regs[DSTAT] &= !0x80;
            self.update_dma_interrupt_requests();
        }

        6_u32
            .saturating_add(memory_waits)
            .saturating_add(io_waits)
            .saturating_add(u32::from(crossed && memory_waits == 0))
    }
    pub(crate) fn dma_io_read(&mut self, port: u16) -> u8 {
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
    pub(crate) fn dma_io_write(&mut self, port: u16, value: u8) {
        if self.bus_error.is_some() {
            return;
        }
        if let Some(index) = self.internal_io_index(port) {
            if let Err(error) = self.bus.io_write(port, value) {
                self.bus_error = Some(error);
                return;
            }
            self.write_internal_io(index, value);
        } else if let Err(error) = self.bus.io_write(port, value) {
            self.bus_error = Some(error);
            return;
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
    pub(crate) fn service_dma(&mut self) -> u32 {
        if self.io_regs[DSTAT] & 0x01 == 0 {
            return 0;
        }

        if self.io_regs[DSTAT] & 0x40 != 0 {
            let count = u16::from_le_bytes([self.io_regs[BCR0L], self.io_regs[BCR0H]]);
            if count == 0 {
                self.io_regs[DSTAT] &= !0x40;
                self.update_dma_interrupt_requests();
            } else {
                let source_mode = (self.io_regs[DMODE] >> 2) & 0x03;
                let destination_mode = (self.io_regs[DMODE] >> 4) & 0x03;
                let memory_to_memory = source_mode < 2 && destination_mode < 2;
                let valid = !(source_mode >= 2 && destination_mode >= 2);

                if memory_to_memory {
                    self.dreq_edge_pending[0] = false;
                    let transfers = if self.io_regs[DMODE] & 0x02 != 0 {
                        count
                    } else {
                        1
                    };
                    let mut cycles = 0_u32;
                    for _ in 0..transfers {
                        cycles = cycles.saturating_add(self.dma0_transfer_byte());
                        if self.bus_error.is_some() {
                            break;
                        }
                    }
                    return cycles;
                }

                if valid && self.dma_request_ready(0) {
                    let edge_sense = self.io_regs[DCNTL] & 0x08 != 0;
                    self.dreq_edge_pending[0] = false;
                    let transfers = if edge_sense { 1 } else { count };
                    let mut cycles = 0_u32;
                    for _ in 0..transfers {
                        cycles = cycles.saturating_add(self.dma0_transfer_byte());
                        if self.bus_error.is_some() {
                            break;
                        }
                    }
                    return cycles;
                }
            }
        }

        if self.io_regs[DSTAT] & 0x80 != 0 {
            let count = u16::from_le_bytes([self.io_regs[BCR1L], self.io_regs[BCR1H]]);
            if count == 0 {
                self.io_regs[DSTAT] &= !0x80;
                self.update_dma_interrupt_requests();
            } else if self.dma_request_ready(1) {
                let edge_sense = self.io_regs[DCNTL] & 0x04 != 0;
                self.dreq_edge_pending[1] = false;
                let transfers = if edge_sense { 1 } else { count };
                let mut cycles = 0_u32;
                for _ in 0..transfers {
                    cycles = cycles.saturating_add(self.dma1_transfer_byte());
                }
                return cycles;
            }
        }

        0
    }
    fn dma_request_ready(&self, channel: usize) -> bool {
        let edge_sense = self.io_regs[DCNTL] & if channel == 0 { 0x08 } else { 0x04 } != 0;
        if edge_sense {
            self.dreq_edge_pending[channel]
        } else {
            self.dreq_level[channel]
        }
    }
    pub(crate) fn update_dma_interrupt_requests(&mut self) {
        self.internal_irq_pending &= !0x0c;
        if self.io_regs[DSTAT] & 0x44 == 0x04 {
            self.internal_irq_pending |= 0x04;
        }
        if self.io_regs[DSTAT] & 0x88 == 0x08 {
            self.internal_irq_pending |= 0x08;
        }
    }
}
