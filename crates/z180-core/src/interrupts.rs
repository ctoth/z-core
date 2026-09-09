use super::*;

impl<B: HostBus> Z180<B> {
    pub fn iff1(&self) -> bool {
        self.iff1
    }
    pub fn set_iff1(&mut self, enabled: bool) {
        self.iff1 = enabled;
    }
    pub fn iff2(&self) -> bool {
        self.iff2
    }
    pub fn set_iff2(&mut self, enabled: bool) {
        self.iff2 = enabled;
    }
    pub fn interrupt_mode(&self) -> u8 {
        self.interrupt_mode
    }
    pub fn set_interrupt_mode(&mut self, mode: u8) {
        self.interrupt_mode = mode;
    }
    pub fn set_irq(&mut self, line: IrqLine, level: bool) {
        let index = match line {
            IrqLine::Int0 => 0,
            IrqLine::Int1 => 1,
            IrqLine::Int2 => 2,
        };
        self.irq_lines[index] = level;
    }
    pub fn set_nmi(&mut self, level: bool) {
        if level && !self.nmi_level {
            self.nmi_pending = true;
            self.io_regs[DSTAT] &= !0x01;
        }
        self.nmi_level = level;
    }
    pub(super) fn interrupt_check_point(&mut self) -> Option<u32> {
        if self.nmi_pending {
            return Some(self.take_nmi());
        }

        let source = self.pending_maskable_source()?;
        if self.ei_shadow {
            return None;
        }
        if !self.iff1 {
            if self.sleeping {
                self.sleeping = false;
            }
            return None;
        }

        Some(self.take_maskable_interrupt(source))
    }
    fn pending_maskable_source(&self) -> Option<IrqSource> {
        if self.irq_lines[0] && self.io_regs[ITC] & 0x01 != 0 {
            Some(IrqSource::Int0)
        } else if self.irq_lines[1] && self.io_regs[ITC] & 0x02 != 0 {
            Some(IrqSource::Int1)
        } else if self.irq_lines[2] && self.io_regs[ITC] & 0x04 != 0 {
            Some(IrqSource::Int2)
        } else if self.internal_irq_pending & 0x01 != 0 {
            Some(IrqSource::Prt0)
        } else if self.internal_irq_pending & 0x02 != 0 {
            Some(IrqSource::Prt1)
        } else if self.internal_irq_pending & 0x04 != 0 {
            Some(IrqSource::Dma0)
        } else if self.internal_irq_pending & 0x08 != 0 {
            Some(IrqSource::Dma1)
        } else if self.internal_irq_pending & 0x10 != 0 {
            Some(IrqSource::Csio)
        } else if self.internal_irq_pending & 0x20 != 0 {
            Some(IrqSource::Asci0)
        } else if self.internal_irq_pending & 0x40 != 0 {
            Some(IrqSource::Asci1)
        } else {
            None
        }
    }
    fn take_nmi(&mut self) -> u32 {
        self.nmi_pending = false;
        self.halted = false;
        self.sleeping = false;
        self.ei_shadow = false;

        let pc = self.registers.get(Reg::PC);
        let _ = self.read_logical(pc);
        self.registers.increment_r();
        self.iff2 = self.iff1;
        self.iff1 = false;
        self.push_word(pc);
        self.registers.set(Reg::PC, 0x0066);
        if self.irq_trace {
            self.push_event(Event::IrqAck {
                cycle: self.cycle_count,
                source: IrqSource::Nmi,
                vector: 0x0066,
            });
        }
        u32::from(NMI_ACKNOWLEDGE_CYCLES)
    }
    fn take_maskable_interrupt(&mut self, source: IrqSource) -> u32 {
        if source == IrqSource::Nmi {
            return self.take_nmi();
        }

        self.halted = false;
        self.sleeping = false;
        self.ei_shadow = false;
        self.iff1 = false;
        self.iff2 = false;
        self.registers.increment_r();

        let pc = self.registers.get(Reg::PC);
        self.push_word(pc);

        let cycles = match source {
            IrqSource::Int0 => match self.interrupt_mode {
                0 => {
                    self.registers.set(Reg::PC, 0x0038);
                    u32::from(INT0_MODE0_RST_CYCLES)
                }
                1 => {
                    self.registers.set(Reg::PC, 0x0038);
                    u32::from(INT0_MODE1_ACKNOWLEDGE_CYCLES)
                }
                _ => {
                    let [i, _] = self.registers.get(Reg::IR).to_be_bytes();
                    let restart = self.read_word(u16::from_be_bytes([i, 0xff]));
                    self.registers.set(Reg::PC, restart);
                    u32::from(VECTORED_ACKNOWLEDGE_CYCLES)
                }
            },
            IrqSource::Int1
            | IrqSource::Int2
            | IrqSource::Prt0
            | IrqSource::Prt1
            | IrqSource::Dma0
            | IrqSource::Dma1
            | IrqSource::Csio
            | IrqSource::Asci0
            | IrqSource::Asci1 => {
                let fixed_code = match source {
                    IrqSource::Int1 | IrqSource::Nmi | IrqSource::Int0 => 0x00,
                    IrqSource::Int2 => 0x02,
                    IrqSource::Prt0 => 0x04,
                    IrqSource::Prt1 => 0x06,
                    IrqSource::Dma0 => 0x08,
                    IrqSource::Dma1 => 0x0a,
                    IrqSource::Csio => 0x0c,
                    IrqSource::Asci0 => 0x0e,
                    IrqSource::Asci1 => 0x10,
                };
                let [i, _] = self.registers.get(Reg::IR).to_be_bytes();
                let vector_low = (self.io_regs[IL] & 0xe0) | fixed_code;
                let restart = self.read_word(u16::from_be_bytes([i, vector_low]));
                self.registers.set(Reg::PC, restart);
                u32::from(VECTORED_ACKNOWLEDGE_CYCLES)
            }
            IrqSource::Nmi => u32::from(NMI_ACKNOWLEDGE_CYCLES),
        };
        if self.irq_trace {
            self.push_event(Event::IrqAck {
                cycle: self.cycle_count,
                source,
                vector: self.registers.get(Reg::PC),
            });
        }
        cycles
    }
    pub(super) fn take_trap(
        &mut self,
        opcode: [u8; 3],
        len: u8,
        stacked_pc: u16,
        ufo: bool,
        m1_fetches: u8,
    ) {
        self.io_regs[ITC] = (self.io_regs[ITC] & 0x07) | 0x80 | if ufo { 0x40 } else { 0 };
        for _ in 0..m1_fetches {
            self.registers.increment_r();
        }
        self.ei_shadow = false;
        self.push_word(stacked_pc);
        self.registers.set(Reg::PC, 0);
        self.push_event(Event::Trap {
            cycle: self.cycle_count,
            pc: self.instruction_pc,
            opcode,
            len,
        });
    }
}
