#![forbid(unsafe_code)]
#![no_std]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "Z180 registers, addresses, opcodes, and test indices intentionally narrow to fixed hardware widths after masking or bounded iteration"
)]

extern crate alloc;

use alloc::{collections::VecDeque, vec::Vec};
use core::convert::Infallible;

mod debug;
use debug::{MemWatch, TraceCapture};
mod disassembler;
mod ioregs;
mod memory;
mod optable;
mod registers;
#[cfg(feature = "state")]
mod state;
#[cfg(all(test, feature = "state"))]
use state::{STATE_VERSION, SavedState};

pub use disassembler::{DisassembledInstruction, disassemble_one};
pub use memory::{ConfigError, MachineConfig, RegionDef, RegionKind, Variant};
pub use registers::Reg;

use ioregs::{
    ASTC0H, ASTC0L, ASTC1H, ASTC1L, BBR, BCR0H, BCR0L, BCR1H, BCR1L, CBAR, CBR, CNTLA0, CNTLB0,
    CNTR, DAR0B, DAR0H, DAR0L, DCNTL, DMODE, DSTAT, FRC, IAR1H, IAR1L, ICR, IL, IO_REG_SPECS,
    IO_REGISTER_COUNT, ITC, MAR1B, MAR1H, MAR1L, RDR0, RDR1, RLDR0H, RLDR0L, RLDR1H, RLDR1L,
    ReadEffect, SAR0B, SAR0H, SAR0L, STAT0, STAT1, TCR, TDR0, TDR1, TMDR0H, TMDR0L, TMDR1H, TMDR1L,
    TRD, WriteEffect,
};
use memory::Memory;
use optable::{
    HALT_IDLE_CYCLES, INT0_MODE0_RST_CYCLES, INT0_MODE1_ACKNOWLEDGE_CYCLES, NMI_ACKNOWLEDGE_CYCLES,
    SECOND_OPCODE_TRAP_CYCLES, THIRD_OPCODE_TRAP_CYCLES, VECTORED_ACKNOWLEDGE_CYCLES,
};
use registers::Registers;

const FLAG_S: u8 = 0x80;
const FLAG_Z: u8 = 0x40;
const FLAG_Y: u8 = 0x20;
const FLAG_H: u8 = 0x10;
const FLAG_X: u8 = 0x08;
const FLAG_PV: u8 = 0x04;
const FLAG_N: u8 = 0x02;
const FLAG_C: u8 = 0x01;
const FLAG_XY: u8 = FLAG_X | FLAG_Y;
pub trait HostBus {
    type Error;

    fn mem_read(&mut self, phys: u32) -> Result<u8, Self::Error>;
    fn mem_write(&mut self, phys: u32, value: u8) -> Result<(), Self::Error>;
    fn io_read(&mut self, port: u16) -> Result<u8, Self::Error>;
    fn io_write(&mut self, port: u16, value: u8) -> Result<(), Self::Error>;
}

#[cfg_attr(feature = "state", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IrqLine {
    Int0,
    Int1,
    Int2,
}

#[cfg_attr(feature = "state", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IrqSource {
    Nmi,
    Int0,
    Int1,
    Int2,
    Prt0,
    Prt1,
    Dma0,
    Dma1,
    Csio,
    Asci0,
    Asci1,
}

#[cfg_attr(feature = "state", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WatchId(u64);

#[cfg_attr(feature = "state", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WatchKind {
    Read,
    Write,
    Both,
}

#[cfg_attr(feature = "state", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    IoRead {
        cycle: u64,
        pc: u16,
        port: u16,
        val: u8,
    },
    IoWrite {
        cycle: u64,
        pc: u16,
        port: u16,
        val: u8,
    },
    MemWrite {
        cycle: u64,
        pc: u16,
        phys: u32,
        val: u8,
    },
    MemRead {
        cycle: u64,
        pc: u16,
        phys: u32,
        val: u8,
    },
    IrqAck {
        cycle: u64,
        source: IrqSource,
        vector: u16,
    },
    Trap {
        cycle: u64,
        pc: u16,
        opcode: [u8; 3],
        len: u8,
    },
    RomWrite {
        cycle: u64,
        pc: u16,
        phys: u32,
        val: u8,
    },
}

#[cfg_attr(feature = "state", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceEntry {
    pub cycle: u64,
    pub pc: u16,
    pub phys_pc: u32,
    pub bytes: [u8; 4],
    pub len: u8,
}

const EXT_MAP_TABLE_LEN: usize = 1 << 20;

enum ExtMapper {
    Function(fn(u32) -> u32),
    Table(Vec<u32>),
}

#[cfg(feature = "state")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StateError {
    MissingVersion,
    UnsupportedVersion(u8),
    Decode,
}

#[cfg(feature = "state")]
impl core::fmt::Display for StateError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingVersion => write!(formatter, "save state has no version byte"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "save state version {version} is unsupported")
            }
            Self::Decode => write!(formatter, "save state payload is invalid"),
        }
    }
}

#[cfg(feature = "state")]
impl core::error::Error for StateError {}

#[allow(
    clippy::struct_excessive_bools,
    reason = "individual booleans model independent Z180 pins, latches, and enable bits"
)]
pub struct Z180<B: HostBus> {
    registers: Registers,
    memory: Memory,
    bus: B,
    bus_error: Option<B::Error>,
    instruction_pc: u16,
    indexed_displacement: Option<i8>,
    cycle_count: u64,
    variant: Variant,
    io_regs: [u8; IO_REGISTER_COUNT],
    mmu_pages: [u32; 16],
    ext_mapper: Option<ExtMapper>,
    timing_branch_taken: bool,
    timing_repeat_iterations: u16,
    timing_memory_waits: u32,
    timing_io_waits: u32,
    halted: bool,
    sleeping: bool,
    iff1: bool,
    iff2: bool,
    ei_shadow: bool,
    interrupt_mode: u8,
    irq_lines: [bool; 3],
    nmi_level: bool,
    nmi_pending: bool,
    dreq_level: [bool; 2],
    dreq_edge_pending: [bool; 2],
    // Phase 6 peripherals set these bits only after their own enable and
    // request conditions are satisfied; this controller owns priority only.
    internal_irq_pending: u8,
    frc_cycle_remainder: u32,
    prt_cycle_remainder: u32,
    prt_high_latch: [u8; 2],
    prt_high_latch_valid: [bool; 2],
    prt_clear_armed: u8,
    asci_cts: [bool; 2],
    asci_dcd: [bool; 2],
    asci_dcd_latched: bool,
    asci_dcd_irq_pending: bool,
    asci_tdr_full: [bool; 2],
    asci_tx_shift: [Option<u8>; 2],
    asci_tx_cycles: [u64; 2],
    asci_tx_clocked: [bool; 2],
    asci_tx_output: [VecDeque<u8>; 2],
    asci_rx_shift: [Option<u8>; 2],
    asci_rx_cycles: [u64; 2],
    asci_rx_clocked: [bool; 2],
    asci_rx_fifo: [VecDeque<u8>; 2],
    csio_rx_shift: Option<u8>,
    csio_cycles: u64,
    csio_clocked: bool,
    csio_tx_output: VecDeque<u8>,
    event_capacity: usize,
    events: VecDeque<Event>,
    events_lost: bool,
    mem_watches: Vec<MemWatch>,
    next_watch_id: u64,
    io_trace: bool,
    irq_trace: bool,
    pc_watch: Option<u16>,
    pc_watch_hits: u64,
    insn_trace_capacity: Option<usize>,
    insn_trace: VecDeque<TraceEntry>,
    insn_trace_capture: Option<TraceCapture>,
}

impl<B: HostBus> Z180<B> {
    /// Constructs a machine with core-owned memory and the supplied host bus.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when the physical address width, memory regions,
    /// or external mapping configuration is invalid.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "the public API deliberately transfers ownership of MachineConfig as specified by the architecture contract"
    )]
    pub fn new(config: MachineConfig, bus: B) -> Result<Self, ConfigError> {
        let mut io_regs = [0; IO_REGISTER_COUNT];
        for (index, spec) in IO_REG_SPECS.iter().copied().enumerate() {
            if spec.is_available(config.variant) {
                io_regs[index] = spec.reset;
            }
        }
        let mut cpu = Self {
            registers: Registers::default(),
            memory: Memory::new(&config)?,
            bus,
            bus_error: None,
            instruction_pc: 0,
            indexed_displacement: None,
            cycle_count: 0,
            variant: config.variant,
            io_regs,
            mmu_pages: [0; 16],
            ext_mapper: None,
            timing_branch_taken: false,
            timing_repeat_iterations: 0,
            timing_memory_waits: 0,
            timing_io_waits: 0,
            halted: false,
            sleeping: false,
            iff1: false,
            iff2: false,
            ei_shadow: false,
            interrupt_mode: 0,
            irq_lines: [false; 3],
            nmi_level: false,
            nmi_pending: false,
            dreq_level: [false; 2],
            dreq_edge_pending: [false; 2],
            internal_irq_pending: 0,
            frc_cycle_remainder: 0,
            prt_cycle_remainder: 0,
            prt_high_latch: [0; 2],
            prt_high_latch_valid: [false; 2],
            prt_clear_armed: 0,
            asci_cts: [false; 2],
            asci_dcd: [false; 2],
            asci_dcd_latched: false,
            asci_dcd_irq_pending: false,
            asci_tdr_full: [false; 2],
            asci_tx_shift: [None; 2],
            asci_tx_cycles: [0; 2],
            asci_tx_clocked: [false; 2],
            asci_tx_output: core::array::from_fn(|_| VecDeque::new()),
            asci_rx_shift: [None; 2],
            asci_rx_cycles: [0; 2],
            asci_rx_clocked: [false; 2],
            asci_rx_fifo: core::array::from_fn(|_| VecDeque::new()),
            csio_rx_shift: None,
            csio_cycles: 0,
            csio_clocked: false,
            csio_tx_output: VecDeque::new(),
            event_capacity: config.event_capacity,
            events: VecDeque::new(),
            events_lost: false,
            mem_watches: Vec::new(),
            next_watch_id: 1,
            io_trace: false,
            irq_trace: false,
            pc_watch: None,
            pc_watch_hits: 0,
            insn_trace_capacity: None,
            insn_trace: VecDeque::new(),
            insn_trace_capture: None,
        };
        cpu.recompute_mmu_pages();
        Ok(cpu)
    }

    pub fn reset(&mut self) {
        self.registers = Registers::default();
        self.instruction_pc = 0;
        self.indexed_displacement = None;
        let asci_data = [
            self.io_regs[TDR0],
            self.io_regs[TDR1],
            self.io_regs[RDR0],
            self.io_regs[RDR1],
            self.io_regs[TRD],
        ];
        let mut dma_registers = [0_u8; 15];
        dma_registers[..13].copy_from_slice(&self.io_regs[SAR0L..=IAR1H]);
        dma_registers[13..].copy_from_slice(&self.io_regs[BCR1L..=BCR1H]);
        for (index, spec) in IO_REG_SPECS.iter().copied().enumerate() {
            self.io_regs[index] = if spec.is_available(self.variant) {
                spec.reset
            } else {
                0
            };
        }
        self.io_regs[TDR0] = asci_data[0];
        self.io_regs[TDR1] = asci_data[1];
        self.io_regs[RDR0] = asci_data[2];
        self.io_regs[RDR1] = asci_data[3];
        self.io_regs[TRD] = asci_data[4];
        self.io_regs[SAR0L..=IAR1H].copy_from_slice(&dma_registers[..13]);
        self.io_regs[BCR1L..=BCR1H].copy_from_slice(&dma_registers[13..]);
        self.recompute_mmu_pages();
        self.timing_branch_taken = false;
        self.timing_repeat_iterations = 0;
        self.timing_memory_waits = 0;
        self.timing_io_waits = 0;
        self.halted = false;
        self.sleeping = false;
        self.iff1 = false;
        self.iff2 = false;
        self.ei_shadow = false;
        self.interrupt_mode = 0;
        self.irq_lines = [false; 3];
        self.nmi_level = false;
        self.nmi_pending = false;
        self.dreq_level = [false; 2];
        self.dreq_edge_pending = [false; 2];
        self.internal_irq_pending = 0;
        self.frc_cycle_remainder = 0;
        self.prt_cycle_remainder = 0;
        self.prt_high_latch = [0; 2];
        self.prt_high_latch_valid = [false; 2];
        self.prt_clear_armed = 0;
        self.asci_cts = [false; 2];
        self.asci_dcd = [false; 2];
        self.asci_dcd_latched = false;
        self.asci_dcd_irq_pending = false;
        self.asci_tdr_full = [false; 2];
        self.asci_tx_shift = [None; 2];
        self.asci_tx_cycles = [0; 2];
        self.asci_tx_clocked = [false; 2];
        self.asci_rx_shift = [None; 2];
        self.asci_rx_cycles = [0; 2];
        self.asci_rx_clocked = [false; 2];
        for channel in 0..2 {
            self.asci_tx_output[channel].clear();
            self.asci_rx_fifo[channel].clear();
        }
        self.csio_rx_shift = None;
        self.csio_cycles = 0;
        self.csio_clocked = false;
        self.csio_tx_output.clear();
        self.events.clear();
        self.events_lost = false;
        self.pc_watch_hits = 0;
        self.insn_trace.clear();
        self.insn_trace_capture = None;
    }

    /// Executes one instruction, interrupt acknowledge, or DMA service step.
    #[allow(
        clippy::missing_panics_doc,
        clippy::too_many_lines,
        reason = "the fetch loop keeps one auditable control path; all indexed hardware tables are bounded before guest-controlled access"
    )]
    pub fn try_step(&mut self) -> Result<u32, B::Error> {
        debug_assert!(self.bus_error.is_none());
        self.indexed_displacement = None;
        self.timing_branch_taken = false;
        self.timing_repeat_iterations = 0;
        self.timing_memory_waits = 0;
        self.timing_io_waits = 0;

        let dma_cycles = self.service_dma();
        if let Some(error) = self.bus_error.take() {
            return Err(error);
        }
        if dma_cycles != 0 {
            self.finish_step(dma_cycles);
        }

        if let Some(cycles) = self.interrupt_check_point() {
            if let Some(error) = self.bus_error.take() {
                return Err(error);
            }
            return Ok(dma_cycles
                .saturating_add(self.finish_step(cycles.saturating_add(self.wait_cycles()))));
        }

        if self.halted {
            let _ = self.read_logical(self.registers.get(Reg::PC));
            if let Some(error) = self.bus_error.take() {
                return Err(error);
            }
            return Ok(dma_cycles.saturating_add(
                self.finish_step(u32::from(HALT_IDLE_CYCLES).saturating_add(self.wait_cycles())),
            ));
        }
        if self.sleeping {
            return Ok(dma_cycles.saturating_add(self.finish_step(u32::from(HALT_IDLE_CYCLES))));
        }

        let pc = self.registers.get(Reg::PC);
        let registers = self.registers;
        let ei_shadow = self.ei_shadow;
        let pc_watch_hits = self.pc_watch_hits;
        if self.pc_watch == Some(pc) {
            self.pc_watch_hits = self.pc_watch_hits.saturating_add(1);
        }
        self.instruction_pc = pc;
        self.begin_insn_trace(pc);
        let first_opcode = self.read_logical(pc);
        let (opcode, descriptor, m1_fetches, is_indexed_bit) = match first_opcode {
            0xcb => {
                let opcode = self.read_logical(pc.wrapping_add(1));
                (opcode, Self::CB_OPCODES[usize::from(opcode)], 2, false)
            }
            0xdd => {
                let opcode = self.read_logical(pc.wrapping_add(1));
                if opcode == 0xcb {
                    let displacement = self.read_logical(pc.wrapping_add(2)) as i8;
                    self.indexed_displacement = Some(displacement);
                    let opcode = self.read_logical(pc.wrapping_add(3));
                    (opcode, Self::DDCB_OPCODES[usize::from(opcode)], 2, true)
                } else {
                    (opcode, Self::DD_OPCODES[usize::from(opcode)], 2, false)
                }
            }
            0xed => {
                let opcode = self.read_logical(pc.wrapping_add(1));
                (opcode, Self::ED_OPCODES[usize::from(opcode)], 2, false)
            }
            0xfd => {
                let opcode = self.read_logical(pc.wrapping_add(1));
                if opcode == 0xcb {
                    let displacement = self.read_logical(pc.wrapping_add(2)) as i8;
                    self.indexed_displacement = Some(displacement);
                    let opcode = self.read_logical(pc.wrapping_add(3));
                    (opcode, Self::FDCB_OPCODES[usize::from(opcode)], 2, true)
                } else {
                    (opcode, Self::FD_OPCODES[usize::from(opcode)], 2, false)
                }
            }
            _ => (
                first_opcode,
                Self::MAIN_OPCODES[usize::from(first_opcode)],
                1,
                false,
            ),
        };
        if let Some(error) = self.bus_error.take() {
            self.registers = registers;
            self.ei_shadow = ei_shadow;
            self.pc_watch_hits = pc_watch_hits;
            self.insn_trace_capture = None;
            return Err(error);
        }
        let Some(handler) = descriptor.handler else {
            if is_indexed_bit {
                let displacement = self
                    .indexed_displacement
                    .take()
                    .expect("indexed-bit displacement was not decoded");
                let index = if first_opcode == 0xfd {
                    Reg::IY
                } else {
                    Reg::IX
                };
                let address = self
                    .registers
                    .get(index)
                    .wrapping_add(i16::from(displacement) as u16);
                let _ = self.read_logical(address);
                if let Some(error) = self.bus_error.take() {
                    self.registers = registers;
                    self.ei_shadow = ei_shadow;
                    self.pc_watch_hits = pc_watch_hits;
                    self.insn_trace_capture = None;
                    return Err(error);
                }
                self.take_trap([first_opcode, 0xcb, opcode], 3, pc.wrapping_add(2), true, 3);
            } else if matches!(first_opcode, 0xcb | 0xdd | 0xed | 0xfd) {
                self.take_trap([first_opcode, opcode, 0], 2, pc.wrapping_add(1), false, 2);
            } else {
                self.take_trap([first_opcode, 0, 0], 1, pc.wrapping_add(1), false, 1);
            }
            self.finish_insn_trace(if is_indexed_bit {
                4
            } else if matches!(first_opcode, 0xcb | 0xdd | 0xed | 0xfd) {
                2
            } else {
                1
            });
            let cycles = if is_indexed_bit {
                THIRD_OPCODE_TRAP_CYCLES
            } else {
                SECOND_OPCODE_TRAP_CYCLES
            };
            return Ok(dma_cycles.saturating_add(
                self.finish_step(u32::from(cycles).saturating_add(self.wait_cycles())),
            ));
        };
        debug_assert!(!descriptor.mnemonic.is_empty());
        debug_assert!(descriptor.length != 0);
        for _ in 0..descriptor.length {
            self.registers.increment_pc();
        }
        for _ in 0..m1_fetches {
            self.registers.increment_r();
        }

        // The interrupt-check point samples the P2.2 EI shadow before fetch;
        // consuming it before dispatch lets a second EI establish a fresh
        // one-instruction shadow.
        self.ei_shadow = false;

        handler(self, opcode);
        if let Some(error) = self.bus_error.take() {
            self.registers = registers;
            self.ei_shadow = ei_shadow;
            self.pc_watch_hits = pc_watch_hits;
            self.insn_trace_capture = None;
            return Err(error);
        }
        self.finish_insn_trace(descriptor.length);

        let repeat_completed = self.registers.get(Reg::PC) != self.instruction_pc;
        let cycles = descriptor
            .cycles
            .expect("implemented opcode is missing UM0050 timing")
            .resolve(
                self.variant,
                self.timing_branch_taken,
                self.timing_repeat_iterations,
                repeat_completed,
            );
        Ok(dma_cycles.saturating_add(self.finish_step(cycles.saturating_add(self.wait_cycles()))))
    }

    pub fn try_run(&mut self, cycles: u32) -> Result<u32, B::Error> {
        let mut consumed = 0_u32;
        while consumed < cycles {
            let step_cycles = self.try_step()?;
            if step_cycles == 0 {
                break;
            }
            consumed = consumed.saturating_add(step_cycles);
        }
        Ok(consumed)
    }

    pub fn cycle_count(&self) -> u64 {
        self.cycle_count
    }

    pub fn halted(&self) -> bool {
        self.halted
    }

    pub fn sleeping(&self) -> bool {
        self.sleeping
    }

    pub fn itc(&self) -> u8 {
        self.io_reg_peek(ITC as u8)
    }

    pub fn io_reg_peek(&self, internal_addr: u8) -> u8 {
        let index = usize::from(internal_addr);
        let Some(spec) = IO_REG_SPECS.get(index).copied() else {
            return 0;
        };
        if !spec.is_available(self.variant) {
            return 0;
        }
        match spec.read_effect {
            ReadEffect::AsciCntlb => self.asci_cntlb_value(index),
            ReadEffect::AsciStat => self.asci_status_value(index - STAT0),
            ReadEffect::AsciRdr
            | ReadEffect::CsioTrd
            | ReadEffect::None
            | ReadEffect::Tcr
            | ReadEffect::TmdrHigh
            | ReadEffect::TmdrLow => self.io_regs[index] & spec.read_mask,
        }
    }

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

    pub fn mmu_translate(&self, logical: u16) -> u32 {
        let page = usize::from(logical >> 12);
        self.mmu_pages[page] + u32::from(logical & 0x0fff)
    }

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

    pub fn set_dreq(&mut self, ch: usize, level: bool) {
        let Some(current) = self.dreq_level.get_mut(ch) else {
            return;
        };
        if level && !*current {
            self.dreq_edge_pending[ch] = true;
        }
        *current = level;
    }

    pub fn set_nmi(&mut self, level: bool) {
        if level && !self.nmi_level {
            self.nmi_pending = true;
            self.io_regs[DSTAT] &= !0x01;
        }
        self.nmi_level = level;
    }

    pub fn reg(&self, reg: Reg) -> u16 {
        self.registers.get(reg)
    }

    pub fn set_reg(&mut self, reg: Reg, value: u16) {
        self.registers.set(reg, value);
    }

    pub fn instruction_pc(&self) -> u16 {
        self.instruction_pc
    }

    /// Reads core-owned physical RAM or ROM without emulation side effects.
    ///
    /// External and unmapped addresses return the configured unmapped byte.
    pub fn mem_peek(&self, phys: u32) -> u8 {
        self.memory.peek(phys)
    }

    /// Writes core-owned physical RAM without emulation side effects.
    ///
    /// ROM, External, and unmapped addresses are left unchanged. The debugger
    /// write never calls [`HostBus`] or emits a memory-watch event.
    pub fn mem_poke(&mut self, phys: u32, value: u8) {
        self.memory.poke(phys, value);
    }

    /// Replaces a page-aligned physical range with a new region kind.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when the range is unaligned or outside the
    /// configured physical address space.
    pub fn remap(&mut self, base: u32, size: u32, kind: RegionKind) -> Result<(), ConfigError> {
        self.memory.remap(base, size, kind)
    }

    pub fn set_ext_mapper(&mut self, mapper: Option<fn(u32) -> u32>) {
        self.ext_mapper = mapper.map(ExtMapper::Function);
    }

    /// Installs or clears the board-level external address mapping table.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when the table does not cover the complete
    /// 20-bit Z180 physical address space.
    pub fn set_ext_map_table(&mut self, table: Option<Vec<u32>>) -> Result<(), ConfigError> {
        if let Some(table) = &table
            && table.len() != EXT_MAP_TABLE_LEN
        {
            return Err(ConfigError::InvalidExtMapTableLength {
                expected: EXT_MAP_TABLE_LEN,
                actual: table.len(),
            });
        }
        self.ext_mapper = table.map(ExtMapper::Table);
        Ok(())
    }

    #[must_use]
    pub fn ram_regions(&self) -> Vec<(u32, u32)> {
        self.memory.ram_regions()
    }

    pub fn ram_region(&self, base: u32) -> Option<&[u8]> {
        self.memory.ram_region(base)
    }

    pub fn ram_region_mut(&mut self, base: u32) -> Option<&mut [u8]> {
        self.memory.ram_region_mut(base)
    }

    #[must_use]
    pub fn is_instruction_implemented(opcodes: &[u8]) -> bool {
        match opcodes {
            [opcode] => Self::MAIN_OPCODES[usize::from(*opcode)].handler.is_some(),
            [0xcb, opcode] => Self::CB_OPCODES[usize::from(*opcode)].handler.is_some(),
            [0xdd, opcode] => Self::DD_OPCODES[usize::from(*opcode)].handler.is_some(),
            [0xed, opcode] => Self::ED_OPCODES[usize::from(*opcode)].handler.is_some(),
            [0xfd, opcode] => Self::FD_OPCODES[usize::from(*opcode)].handler.is_some(),
            [0xdd, 0xcb, opcode] => Self::DDCB_OPCODES[usize::from(*opcode)].handler.is_some(),
            [0xfd, 0xcb, opcode] => Self::FDCB_OPCODES[usize::from(*opcode)].handler.is_some(),
            _ => false,
        }
    }

    fn interrupt_check_point(&mut self) -> Option<u32> {
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

    fn take_trap(&mut self, opcode: [u8; 3], len: u8, stacked_pc: u16, ufo: bool, m1_fetches: u8) {
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

    fn accumulator(&self) -> u8 {
        self.registers.get(Reg::AF).to_be_bytes()[0]
    }

    fn flags(&self) -> u8 {
        self.registers.get(Reg::AF).to_be_bytes()[1]
    }

    fn set_accumulator(&mut self, value: u8) {
        self.registers
            .set(Reg::AF, u16::from_be_bytes([value, self.flags()]));
    }

    fn set_flags(&mut self, value: u8) {
        self.registers
            .set(Reg::AF, u16::from_be_bytes([self.accumulator(), value]));
    }

    fn set_accumulator_and_flags(&mut self, accumulator: u8, flags: u8) {
        self.registers
            .set(Reg::AF, u16::from_be_bytes([accumulator, flags]));
    }

    fn read_reg8(&mut self, code: u8) -> u8 {
        if code & 0x07 == 6 {
            self.read_logical(self.registers.get(Reg::HL))
        } else {
            self.registers.byte(code).unwrap_or(0)
        }
    }

    fn write_reg8(&mut self, code: u8, value: u8) {
        if code & 0x07 == 6 {
            self.write_logical(self.registers.get(Reg::HL), value);
        } else {
            let _ = self.registers.set_byte(code, value);
        }
    }

    fn reg16(&self, code: u8) -> u16 {
        match code & 0x03 {
            0 => self.registers.get(Reg::BC),
            1 => self.registers.get(Reg::DE),
            2 => self.registers.get(Reg::HL),
            _ => self.registers.get(Reg::SP),
        }
    }

    fn set_reg16(&mut self, code: u8, value: u16) {
        let reg = match code & 0x03 {
            0 => Reg::BC,
            1 => Reg::DE,
            2 => Reg::HL,
            _ => Reg::SP,
        };
        self.registers.set(reg, value);
    }

    fn stack_reg16(&self, code: u8) -> u16 {
        match code & 0x03 {
            0 => self.registers.get(Reg::BC),
            1 => self.registers.get(Reg::DE),
            2 => self.registers.get(Reg::HL),
            _ => self.registers.get(Reg::AF),
        }
    }

    fn set_stack_reg16(&mut self, code: u8, value: u16) {
        let reg = match code & 0x03 {
            0 => Reg::BC,
            1 => Reg::DE,
            2 => Reg::HL,
            _ => Reg::AF,
        };
        self.registers.set(reg, value);
    }

    fn immediate8(&mut self) -> u8 {
        self.read_logical(self.instruction_pc.wrapping_add(1))
    }

    fn immediate16(&mut self) -> u16 {
        self.read_word(self.instruction_pc.wrapping_add(1))
    }

    fn read_word(&mut self, address: u16) -> u16 {
        let low = self.read_logical(address);
        let high = self.read_logical(address.wrapping_add(1));
        u16::from_le_bytes([low, high])
    }

    fn write_word(&mut self, address: u16, value: u16) {
        let [low, high] = value.to_le_bytes();
        self.write_logical(address, low);
        self.write_logical(address.wrapping_add(1), high);
    }

    fn push_word(&mut self, value: u16) {
        let sp = self.registers.get(Reg::SP);
        let [low, high] = value.to_le_bytes();
        self.write_logical(sp.wrapping_sub(1), high);
        self.write_logical(sp.wrapping_sub(2), low);
        self.registers.set(Reg::SP, sp.wrapping_sub(2));
    }

    fn pop_word(&mut self) -> u16 {
        let sp = self.registers.get(Reg::SP);
        let value = self.read_word(sp);
        self.registers.set(Reg::SP, sp.wrapping_add(2));
        value
    }

    fn condition(&self, code: u8) -> bool {
        let flags = self.flags();
        match code & 0x07 {
            0 => flags & FLAG_Z == 0,
            1 => flags & FLAG_Z != 0,
            2 => flags & FLAG_C == 0,
            3 => flags & FLAG_C != 0,
            4 => flags & FLAG_PV == 0,
            5 => flags & FLAG_PV != 0,
            6 => flags & FLAG_S == 0,
            _ => flags & FLAG_S != 0,
        }
    }

    fn relative_target(&self, displacement: u8) -> u16 {
        let signed = i16::from(displacement as i8);
        self.registers.get(Reg::PC).wrapping_add(signed as u16)
    }

    const fn sign_zero_xy(value: u8) -> u8 {
        let mut flags = value & (FLAG_S | FLAG_XY);
        if value == 0 {
            flags |= FLAG_Z;
        }
        flags
    }

    const fn parity(value: u8) -> bool {
        value.count_ones() & 1 == 0
    }

    const fn parity_flag(value: u8) -> u8 {
        if Self::parity(value) { FLAG_PV } else { 0 }
    }

    fn add8(&mut self, value: u8, with_carry: bool) {
        let accumulator = self.accumulator();
        let carry = u8::from(with_carry && self.flags() & FLAG_C != 0);
        let sum = u16::from(accumulator) + u16::from(value) + u16::from(carry);
        let result = sum as u8;
        let mut flags = Self::sign_zero_xy(result);
        if (accumulator & 0x0f) + (value & 0x0f) + carry > 0x0f {
            flags |= FLAG_H;
        }
        if (!(accumulator ^ value) & (accumulator ^ result) & 0x80) != 0 {
            flags |= FLAG_PV;
        }
        if sum > 0xff {
            flags |= FLAG_C;
        }
        self.set_accumulator_and_flags(result, flags);
    }

    fn sub8(&mut self, value: u8, with_carry: bool, compare_only: bool) {
        let accumulator = self.accumulator();
        let carry = u8::from(with_carry && self.flags() & FLAG_C != 0);
        let result = accumulator.wrapping_sub(value).wrapping_sub(carry);
        let mut flags = Self::sign_zero_xy(result) | FLAG_N;
        if (accumulator & 0x0f) < (value & 0x0f) + carry {
            flags |= FLAG_H;
        }
        if ((accumulator ^ value) & (accumulator ^ result) & 0x80) != 0 {
            flags |= FLAG_PV;
        }
        if u16::from(accumulator) < u16::from(value) + u16::from(carry) {
            flags |= FLAG_C;
        }
        if compare_only {
            self.set_flags(flags);
        } else {
            self.set_accumulator_and_flags(result, flags);
        }
    }

    fn execute_alu(&mut self, operation: u8, value: u8) {
        match operation & 0x07 {
            0 => self.add8(value, false),
            1 => self.add8(value, true),
            2 => self.sub8(value, false, false),
            3 => self.sub8(value, true, false),
            4 => {
                let result = self.accumulator() & value;
                let flags = Self::sign_zero_xy(result) | Self::parity_flag(result) | FLAG_H;
                self.set_accumulator_and_flags(result, flags);
            }
            5 => {
                let result = self.accumulator() ^ value;
                let flags = Self::sign_zero_xy(result) | Self::parity_flag(result);
                self.set_accumulator_and_flags(result, flags);
            }
            6 => {
                let result = self.accumulator() | value;
                let flags = Self::sign_zero_xy(result) | Self::parity_flag(result);
                self.set_accumulator_and_flags(result, flags);
            }
            _ => self.sub8(value, false, true),
        }
    }

    #[allow(
        clippy::unused_self,
        reason = "all opcode handlers share the table's method-pointer signature, including NOP"
    )]
    pub(crate) fn execute_nop(&mut self, _opcode: u8) {}

    pub(crate) fn execute_halt(&mut self, _opcode: u8) {
        self.halted = true;
    }

    pub(crate) fn execute_ld_reg16_immediate(&mut self, opcode: u8) {
        let value = self.immediate16();
        self.set_reg16(opcode >> 4, value);
    }

    pub(crate) fn execute_ld_indirect_a(&mut self, opcode: u8) {
        let address = if opcode & 0x10 == 0 {
            self.registers.get(Reg::BC)
        } else {
            self.registers.get(Reg::DE)
        };
        self.write_logical(address, self.accumulator());
    }

    pub(crate) fn execute_ld_a_indirect(&mut self, opcode: u8) {
        let address = if opcode & 0x10 == 0 {
            self.registers.get(Reg::BC)
        } else {
            self.registers.get(Reg::DE)
        };
        let value = self.read_logical(address);
        self.set_accumulator(value);
    }

    pub(crate) fn execute_inc_reg16(&mut self, opcode: u8) {
        let code = opcode >> 4;
        self.set_reg16(code, self.reg16(code).wrapping_add(1));
    }

    pub(crate) fn execute_dec_reg16(&mut self, opcode: u8) {
        let code = opcode >> 4;
        self.set_reg16(code, self.reg16(code).wrapping_sub(1));
    }

    pub(crate) fn execute_inc_reg8(&mut self, opcode: u8) {
        let code = (opcode >> 3) & 0x07;
        let value = self.read_reg8(code);
        let result = value.wrapping_add(1);
        let mut flags = Self::sign_zero_xy(result) | (self.flags() & FLAG_C);
        if value & 0x0f == 0x0f {
            flags |= FLAG_H;
        }
        if value == 0x7f {
            flags |= FLAG_PV;
        }
        self.write_reg8(code, result);
        self.set_flags(flags);
    }

    pub(crate) fn execute_dec_reg8(&mut self, opcode: u8) {
        let code = (opcode >> 3) & 0x07;
        let value = self.read_reg8(code);
        let result = value.wrapping_sub(1);
        let mut flags = Self::sign_zero_xy(result) | (self.flags() & FLAG_C) | FLAG_N;
        if value.trailing_zeros() >= 4 {
            flags |= FLAG_H;
        }
        if value == 0x80 {
            flags |= FLAG_PV;
        }
        self.write_reg8(code, result);
        self.set_flags(flags);
    }

    pub(crate) fn execute_ld_reg8_immediate(&mut self, opcode: u8) {
        let value = self.immediate8();
        self.write_reg8((opcode >> 3) & 0x07, value);
    }

    pub(crate) fn execute_add_hl(&mut self, opcode: u8) {
        let hl = self.registers.get(Reg::HL);
        let value = self.reg16(opcode >> 4);
        let sum = u32::from(hl) + u32::from(value);
        let result = sum as u16;
        let mut flags = self.flags() & (FLAG_S | FLAG_Z | FLAG_PV);
        flags |= result.to_be_bytes()[0] & FLAG_XY;
        if ((hl ^ value ^ result) & 0x1000) != 0 {
            flags |= FLAG_H;
        }
        if sum > 0xffff {
            flags |= FLAG_C;
        }
        self.registers.set(Reg::HL, result);
        self.set_flags(flags);
    }

    pub(crate) fn execute_accumulator_rotate(&mut self, opcode: u8) {
        let accumulator = self.accumulator();
        let old_carry = u8::from(self.flags() & FLAG_C != 0);
        let (result, carry) = match opcode {
            0x07 => (accumulator.rotate_left(1), accumulator >> 7),
            0x0f => (accumulator.rotate_right(1), accumulator & 1),
            0x17 => ((accumulator << 1) | old_carry, accumulator >> 7),
            _ => ((accumulator >> 1) | (old_carry << 7), accumulator & 1),
        };
        let flags =
            (self.flags() & (FLAG_S | FLAG_Z | FLAG_PV)) | (result & FLAG_XY) | (carry & FLAG_C);
        self.set_accumulator_and_flags(result, flags);
    }

    pub(crate) fn execute_cb_rotate_shift(&mut self, opcode: u8) {
        let code = opcode & 0x07;
        let value = self.read_reg8(code);
        let old_carry = u8::from(self.flags() & FLAG_C != 0);
        let (result, carry) = match (opcode >> 3) & 0x07 {
            0 => (value.rotate_left(1), value >> 7),
            1 => (value.rotate_right(1), value & 1),
            2 => ((value << 1) | old_carry, value >> 7),
            3 => ((value >> 1) | (old_carry << 7), value & 1),
            4 => (value << 1, value >> 7),
            5 => ((value >> 1) | (value & 0x80), value & 1),
            7 => (value >> 1, value & 1),
            _ => return,
        };
        let flags = Self::sign_zero_xy(result) | Self::parity_flag(result) | carry;
        self.write_reg8(code, result);
        self.set_flags(flags);
    }

    pub(crate) fn execute_cb_bit(&mut self, opcode: u8) {
        let bit = (opcode >> 3) & 0x07;
        let value = self.read_reg8(opcode & 0x07);
        let mask = 1_u8 << bit;
        let mut flags = (self.flags() & FLAG_C) | (value & FLAG_XY) | FLAG_H;
        if value & mask == 0 {
            flags |= FLAG_Z | FLAG_PV;
        } else if bit == 7 {
            flags |= FLAG_S;
        }
        self.set_flags(flags);
    }

    pub(crate) fn execute_cb_res(&mut self, opcode: u8) {
        let code = opcode & 0x07;
        let value = self.read_reg8(code);
        let mask = 1_u8 << ((opcode >> 3) & 0x07);
        self.write_reg8(code, value & !mask);
    }

    pub(crate) fn execute_cb_set(&mut self, opcode: u8) {
        let code = opcode & 0x07;
        let value = self.read_reg8(code);
        let mask = 1_u8 << ((opcode >> 3) & 0x07);
        self.write_reg8(code, value | mask);
    }

    #[allow(
        clippy::too_many_lines,
        reason = "the indexed-page dispatcher stays aligned with the single auditable opcode table"
    )]
    pub(crate) fn execute_index<const IY: bool>(&mut self, opcode: u8) {
        let index_reg = if IY { Reg::IY } else { Reg::IX };

        match opcode {
            0x09 | 0x19 | 0x29 | 0x39 => {
                let index = self.registers.get(index_reg);
                let value = match (opcode >> 4) & 0x03 {
                    0 => self.registers.get(Reg::BC),
                    1 => self.registers.get(Reg::DE),
                    2 => index,
                    _ => self.registers.get(Reg::SP),
                };
                let sum = u32::from(index) + u32::from(value);
                let result = sum as u16;
                let mut flags = self.flags() & (FLAG_S | FLAG_Z | FLAG_PV);
                flags |= result.to_be_bytes()[0] & FLAG_XY;
                if ((index ^ value ^ result) & 0x1000) != 0 {
                    flags |= FLAG_H;
                }
                if sum > 0xffff {
                    flags |= FLAG_C;
                }
                self.registers.set(index_reg, result);
                self.set_flags(flags);
            }
            0x21 => {
                let value = self.read_word(self.instruction_pc.wrapping_add(2));
                self.registers.set(index_reg, value);
            }
            0x22 => {
                let address = self.read_word(self.instruction_pc.wrapping_add(2));
                self.write_word(address, self.registers.get(index_reg));
            }
            0x23 => {
                let value = self.registers.get(index_reg).wrapping_add(1);
                self.registers.set(index_reg, value);
            }
            0x2a => {
                let address = self.read_word(self.instruction_pc.wrapping_add(2));
                let value = self.read_word(address);
                self.registers.set(index_reg, value);
            }
            0x2b => {
                let value = self.registers.get(index_reg).wrapping_sub(1);
                self.registers.set(index_reg, value);
            }
            0x34 | 0x35 => {
                let displacement = self.read_logical(self.instruction_pc.wrapping_add(2)) as i8;
                let address = self
                    .registers
                    .get(index_reg)
                    .wrapping_add(i16::from(displacement) as u16);
                let value = self.read_logical(address);
                let result = if opcode == 0x34 {
                    value.wrapping_add(1)
                } else {
                    value.wrapping_sub(1)
                };
                let mut flags = Self::sign_zero_xy(result) | (self.flags() & FLAG_C);
                if opcode == 0x34 {
                    if value & 0x0f == 0x0f {
                        flags |= FLAG_H;
                    }
                    if value == 0x7f {
                        flags |= FLAG_PV;
                    }
                } else {
                    flags |= FLAG_N;
                    if value.trailing_zeros() >= 4 {
                        flags |= FLAG_H;
                    }
                    if value == 0x80 {
                        flags |= FLAG_PV;
                    }
                }
                self.write_logical(address, result);
                self.set_flags(flags);
            }
            0x36 => {
                let displacement = self.read_logical(self.instruction_pc.wrapping_add(2)) as i8;
                let address = self
                    .registers
                    .get(index_reg)
                    .wrapping_add(i16::from(displacement) as u16);
                let value = self.read_logical(self.instruction_pc.wrapping_add(3));
                self.write_logical(address, value);
            }
            0x46 | 0x4e | 0x56 | 0x5e | 0x66 | 0x6e | 0x7e => {
                let displacement = self.read_logical(self.instruction_pc.wrapping_add(2)) as i8;
                let address = self
                    .registers
                    .get(index_reg)
                    .wrapping_add(i16::from(displacement) as u16);
                let value = self.read_logical(address);
                self.write_reg8((opcode >> 3) & 0x07, value);
            }
            0x70..=0x75 | 0x77 => {
                let displacement = self.read_logical(self.instruction_pc.wrapping_add(2)) as i8;
                let address = self
                    .registers
                    .get(index_reg)
                    .wrapping_add(i16::from(displacement) as u16);
                let value = self.read_reg8(opcode & 0x07);
                self.write_logical(address, value);
            }
            0x86 | 0x8e | 0x96 | 0x9e | 0xa6 | 0xae | 0xb6 | 0xbe => {
                let displacement = self.read_logical(self.instruction_pc.wrapping_add(2)) as i8;
                let address = self
                    .registers
                    .get(index_reg)
                    .wrapping_add(i16::from(displacement) as u16);
                let value = self.read_logical(address);
                self.execute_alu((opcode >> 3) & 0x07, value);
            }
            0xe1 => {
                let value = self.pop_word();
                self.registers.set(index_reg, value);
            }
            0xe3 => {
                let sp = self.registers.get(Reg::SP);
                let memory_value = self.read_word(sp);
                let index = self.registers.get(index_reg);
                self.write_word(sp, index);
                self.registers.set(index_reg, memory_value);
            }
            0xe5 => self.push_word(self.registers.get(index_reg)),
            0xe9 => self.registers.set(Reg::PC, self.registers.get(index_reg)),
            0xf9 => self.registers.set(Reg::SP, self.registers.get(index_reg)),
            _ => {}
        }
    }

    pub(crate) fn execute_index_cb<const IY: bool>(&mut self, opcode: u8) {
        if opcode & 0x07 != 6 || (0x30..=0x37).contains(&opcode) {
            return;
        }

        let index_reg = if IY { Reg::IY } else { Reg::IX };
        let displacement = self
            .indexed_displacement
            .take()
            .expect("indexed-bit displacement was not decoded");
        let address = self
            .registers
            .get(index_reg)
            .wrapping_add(i16::from(displacement) as u16);
        let value = self.read_logical(address);

        match opcode {
            0x00..=0x3f => {
                let old_carry = u8::from(self.flags() & FLAG_C != 0);
                let (result, carry) = match (opcode >> 3) & 0x07 {
                    0 => (value.rotate_left(1), value >> 7),
                    1 => (value.rotate_right(1), value & 1),
                    2 => ((value << 1) | old_carry, value >> 7),
                    3 => ((value >> 1) | (old_carry << 7), value & 1),
                    4 => (value << 1, value >> 7),
                    5 => ((value >> 1) | (value & 0x80), value & 1),
                    7 => (value >> 1, value & 1),
                    _ => return,
                };
                let flags = Self::sign_zero_xy(result) | Self::parity_flag(result) | carry;
                self.write_logical(address, result);
                self.set_flags(flags);
            }
            0x40..=0x7f => {
                let bit = (opcode >> 3) & 0x07;
                let mask = 1_u8 << bit;
                let mut flags = (self.flags() & FLAG_C) | (value & FLAG_XY) | FLAG_H;
                if value & mask == 0 {
                    flags |= FLAG_Z | FLAG_PV;
                } else if bit == 7 {
                    flags |= FLAG_S;
                }
                self.set_flags(flags);
            }
            0x80..=0xbf => {
                let mask = 1_u8 << ((opcode >> 3) & 0x07);
                self.write_logical(address, value & !mask);
            }
            _ => {
                let mask = 1_u8 << ((opcode >> 3) & 0x07);
                self.write_logical(address, value | mask);
            }
        }
    }

    #[allow(
        clippy::too_many_lines,
        reason = "the ED-page dispatcher stays aligned with the single auditable opcode table"
    )]
    pub(crate) fn execute_ed(&mut self, opcode: u8) {
        match opcode {
            0x00 | 0x08 | 0x10 | 0x18 | 0x20 | 0x28 | 0x30 | 0x38 => {
                let port = u16::from(self.read_logical(self.instruction_pc.wrapping_add(2)));
                let value = self.read_io(port);
                let code = (opcode >> 3) & 0x07;
                if code != 6 {
                    self.write_reg8(code, value);
                }
                self.set_flags(
                    Self::sign_zero_xy(value) | Self::parity_flag(value) | (self.flags() & FLAG_C),
                );
            }
            0x01 | 0x09 | 0x11 | 0x19 | 0x21 | 0x29 | 0x39 => {
                let port = u16::from(self.read_logical(self.instruction_pc.wrapping_add(2)));
                let value = self.read_reg8((opcode >> 3) & 0x07);
                self.write_io(port, value);
            }
            0x04 | 0x0c | 0x14 | 0x1c | 0x24 | 0x2c | 0x34 | 0x3c => {
                let result = self.accumulator() & self.read_reg8((opcode >> 3) & 0x07);
                self.set_flags(Self::sign_zero_xy(result) | Self::parity_flag(result) | FLAG_H);
            }
            0x40 | 0x48 | 0x50 | 0x58 | 0x60 | 0x68 | 0x78 => {
                let value = self.read_io(self.registers.get(Reg::BC));
                self.write_reg8((opcode >> 3) & 0x07, value);
                self.set_flags(
                    Self::sign_zero_xy(value) | Self::parity_flag(value) | (self.flags() & FLAG_C),
                );
            }
            0x41 | 0x49 | 0x51 | 0x59 | 0x61 | 0x69 | 0x79 => {
                let value = self.read_reg8((opcode >> 3) & 0x07);
                self.write_io(self.registers.get(Reg::BC), value);
            }
            0x42 | 0x52 | 0x62 | 0x72 | 0x4a | 0x5a | 0x6a | 0x7a => {
                let subtract = opcode & 0x08 == 0;
                let left = self.registers.get(Reg::HL);
                let right = self.reg16((opcode >> 4) & 0x03);
                let carry = u32::from(self.flags() & FLAG_C != 0);
                let result = if subtract {
                    left.wrapping_sub(right).wrapping_sub(carry as u16)
                } else {
                    left.wrapping_add(right).wrapping_add(carry as u16)
                };
                let mut flags = result.to_be_bytes()[0] & (FLAG_S | FLAG_XY);
                if result == 0 {
                    flags |= FLAG_Z;
                }
                if ((left ^ right ^ result) & 0x1000) != 0 {
                    flags |= FLAG_H;
                }
                if subtract {
                    flags |= FLAG_N;
                    if ((left ^ right) & (left ^ result) & 0x8000) != 0 {
                        flags |= FLAG_PV;
                    }
                    if u32::from(left) < u32::from(right) + carry {
                        flags |= FLAG_C;
                    }
                } else {
                    if (!(left ^ right) & (left ^ result) & 0x8000) != 0 {
                        flags |= FLAG_PV;
                    }
                    if u32::from(left) + u32::from(right) + carry > 0xffff {
                        flags |= FLAG_C;
                    }
                }
                self.registers.set(Reg::HL, result);
                self.set_flags(flags);
            }
            0x43 | 0x53 | 0x63 | 0x73 => {
                let address = self.read_word(self.instruction_pc.wrapping_add(2));
                self.write_word(address, self.reg16((opcode >> 4) & 0x03));
            }
            0x4b | 0x5b | 0x6b | 0x7b => {
                let address = self.read_word(self.instruction_pc.wrapping_add(2));
                let value = self.read_word(address);
                self.set_reg16((opcode >> 4) & 0x03, value);
            }
            0x44 => {
                let value = self.accumulator();
                self.set_accumulator(0);
                self.sub8(value, false, false);
            }
            0x45 | 0x4d => {
                let pc = self.pop_word();
                self.registers.set(Reg::PC, pc);
                if opcode == 0x45 {
                    self.iff1 = self.iff2;
                }
            }
            0x46 => self.interrupt_mode = 0,
            0x56 => self.interrupt_mode = 1,
            0x5e => self.interrupt_mode = 2,
            0x47 | 0x4f => {
                let [i, r] = self.registers.get(Reg::IR).to_be_bytes();
                let ir = if opcode == 0x47 {
                    u16::from_be_bytes([self.accumulator(), r])
                } else {
                    u16::from_be_bytes([i, self.accumulator()])
                };
                self.registers.set(Reg::IR, ir);
            }
            0x57 | 0x5f => {
                let [i, r] = self.registers.get(Reg::IR).to_be_bytes();
                let value = if opcode == 0x57 { i } else { r };
                let flags = Self::sign_zero_xy(value)
                    | (self.flags() & FLAG_C)
                    | (u8::from(self.iff2) * FLAG_PV);
                self.set_accumulator_and_flags(value, flags);
            }
            0x4c | 0x5c | 0x6c | 0x7c => {
                let code = (opcode >> 4) & 0x03;
                let [high, low] = self.reg16(code).to_be_bytes();
                self.set_reg16(code, u16::from(high) * u16::from(low));
            }
            0x64 => {
                let value = self.read_logical(self.instruction_pc.wrapping_add(2));
                let result = self.accumulator() & value;
                self.set_flags(Self::sign_zero_xy(result) | Self::parity_flag(result) | FLAG_H);
            }
            0x67 | 0x6f => {
                let address = self.registers.get(Reg::HL);
                let memory = self.read_logical(address);
                let accumulator = self.accumulator();
                let (next_memory, next_accumulator) = if opcode == 0x67 {
                    (
                        (accumulator << 4) | (memory >> 4),
                        (accumulator & 0xf0) | (memory & 0x0f),
                    )
                } else {
                    (
                        (memory << 4) | (accumulator & 0x0f),
                        (accumulator & 0xf0) | (memory >> 4),
                    )
                };
                self.write_logical(address, next_memory);
                let flags = Self::sign_zero_xy(next_accumulator)
                    | Self::parity_flag(next_accumulator)
                    | (self.flags() & FLAG_C);
                self.set_accumulator_and_flags(next_accumulator, flags);
            }
            0x74 => {
                let mask = self.read_logical(self.instruction_pc.wrapping_add(2));
                let value = self.read_io(u16::from(self.registers.get(Reg::BC) as u8));
                let result = value & mask;
                self.set_flags(Self::sign_zero_xy(result) | Self::parity_flag(result) | FLAG_H);
            }
            0x76 => self.sleeping = true,
            0x83 | 0x8b | 0x93 | 0x9b => {
                let decrement = opcode & 0x08 != 0;
                let repeat = opcode & 0x10 != 0;
                loop {
                    let [b, c] = self.registers.get(Reg::BC).to_be_bytes();
                    let value = self.read_logical(self.registers.get(Reg::HL));
                    self.write_io(u16::from(c), value);
                    let next_b = b.wrapping_sub(1);
                    let next_c = if decrement {
                        c.wrapping_sub(1)
                    } else {
                        c.wrapping_add(1)
                    };
                    let next_hl = if decrement {
                        self.registers.get(Reg::HL).wrapping_sub(1)
                    } else {
                        self.registers.get(Reg::HL).wrapping_add(1)
                    };
                    self.registers
                        .set(Reg::BC, u16::from_be_bytes([next_b, next_c]));
                    self.registers.set(Reg::HL, next_hl);
                    let mut flags = if value & 0x80 != 0 { FLAG_N } else { 0 };
                    if next_b == 0 {
                        flags |= FLAG_Z;
                        if repeat {
                            flags |= FLAG_PV;
                        }
                    }
                    self.set_flags(flags);
                    if !repeat || next_b == 0 {
                        break;
                    }
                    self.timing_repeat_iterations = self.timing_repeat_iterations.saturating_add(1);
                }
            }
            0xa0 | 0xa8 | 0xb0 | 0xb8 => {
                let decrement = opcode & 0x08 != 0;
                let repeat = opcode & 0x10 != 0;
                let value = self.read_logical(self.registers.get(Reg::HL));
                self.write_logical(self.registers.get(Reg::DE), value);
                let delta = if decrement { u16::MAX } else { 1 };
                self.registers
                    .set(Reg::HL, self.registers.get(Reg::HL).wrapping_add(delta));
                self.registers
                    .set(Reg::DE, self.registers.get(Reg::DE).wrapping_add(delta));
                let count = self.registers.get(Reg::BC).wrapping_sub(1);
                self.registers.set(Reg::BC, count);
                let mut flags = self.flags() & (FLAG_S | FLAG_Z | FLAG_C);
                if count != 0 {
                    flags |= FLAG_PV;
                }
                self.set_flags(flags);
                if repeat && count != 0 {
                    self.timing_repeat_iterations = 1;
                    self.registers
                        .set(Reg::PC, self.registers.get(Reg::PC).wrapping_sub(2));
                }
            }
            0xa1 | 0xa9 | 0xb1 | 0xb9 => {
                let decrement = opcode & 0x08 != 0;
                let repeat = opcode & 0x10 != 0;
                let value = self.read_logical(self.registers.get(Reg::HL));
                let accumulator = self.accumulator();
                let result = accumulator.wrapping_sub(value);
                let count = self.registers.get(Reg::BC).wrapping_sub(1);
                let delta = if decrement { u16::MAX } else { 1 };
                self.registers.set(Reg::BC, count);
                self.registers
                    .set(Reg::HL, self.registers.get(Reg::HL).wrapping_add(delta));
                let mut flags = Self::sign_zero_xy(result) | (self.flags() & FLAG_C) | FLAG_N;
                if accumulator & 0x0f < value & 0x0f {
                    flags |= FLAG_H;
                }
                if count != 0 {
                    flags |= FLAG_PV;
                }
                self.set_flags(flags);
                if repeat && count != 0 && result != 0 {
                    self.timing_repeat_iterations = 1;
                    self.registers
                        .set(Reg::PC, self.registers.get(Reg::PC).wrapping_sub(2));
                }
            }
            0xa2 | 0xaa | 0xb2 | 0xba | 0xa3 | 0xab | 0xb3 | 0xbb => {
                let input = opcode & 0x01 == 0;
                let decrement = opcode & 0x08 != 0;
                let repeat = opcode & 0x10 != 0;
                let [b, c] = self.registers.get(Reg::BC).to_be_bytes();
                let next_b = b.wrapping_sub(1);
                let port = u16::from_be_bytes([if input { b } else { next_b }, c]);
                let address = self.registers.get(Reg::HL);
                let value = if input {
                    let value = self.read_io(port);
                    self.write_logical(address, value);
                    value
                } else {
                    let value = self.read_logical(address);
                    self.write_io(port, value);
                    value
                };
                self.registers.set(Reg::BC, u16::from_be_bytes([next_b, c]));
                let delta = if decrement { u16::MAX } else { 1 };
                let next_hl = address.wrapping_add(delta);
                self.registers.set(Reg::HL, next_hl);
                let adjustment = if input {
                    c.wrapping_add(if decrement { u8::MAX } else { 1 })
                } else {
                    next_hl.to_le_bytes()[0]
                };
                let flag_sum = u16::from(value) + u16::from(adjustment);
                let mut flags = Self::sign_zero_xy(next_b);
                if value & 0x80 != 0 {
                    flags |= FLAG_N;
                }
                if flag_sum > u16::from(u8::MAX) {
                    flags |= FLAG_H | FLAG_C;
                }
                flags |= Self::parity_flag((flag_sum.to_le_bytes()[0] & 0x07) ^ next_b);
                if repeat && next_b != 0 {
                    let parity_input = if flags & FLAG_C != 0 {
                        flags &= !FLAG_H;
                        if flags & FLAG_N != 0 {
                            if next_b.trailing_zeros() >= 4 {
                                flags |= FLAG_H;
                            }
                            next_b.wrapping_sub(1) & 0x07
                        } else {
                            if next_b & 0x0f == 0x0f {
                                flags |= FLAG_H;
                            }
                            next_b.wrapping_add(1) & 0x07
                        }
                    } else {
                        next_b & 0x07
                    };
                    flags ^= Self::parity_flag(parity_input) ^ FLAG_PV;
                }
                self.set_flags(flags);
                if repeat && next_b != 0 {
                    self.timing_repeat_iterations = 1;
                    self.registers
                        .set(Reg::PC, self.registers.get(Reg::PC).wrapping_sub(2));
                }
            }
            _ => {}
        }
    }

    pub(crate) fn execute_ld_absolute_hl(&mut self, _opcode: u8) {
        let address = self.immediate16();
        self.write_word(address, self.registers.get(Reg::HL));
    }

    pub(crate) fn execute_ld_hl_absolute(&mut self, _opcode: u8) {
        let address = self.immediate16();
        let value = self.read_word(address);
        self.registers.set(Reg::HL, value);
    }

    pub(crate) fn execute_ld_absolute_a(&mut self, _opcode: u8) {
        let address = self.immediate16();
        self.write_logical(address, self.accumulator());
    }

    pub(crate) fn execute_ld_a_absolute(&mut self, _opcode: u8) {
        let address = self.immediate16();
        let value = self.read_logical(address);
        self.set_accumulator(value);
    }

    pub(crate) fn execute_daa(&mut self, _opcode: u8) {
        let accumulator = self.accumulator();
        let old_flags = self.flags();
        let subtract = old_flags & FLAG_N != 0;
        let mut correction = 0_u8;

        if old_flags & FLAG_H != 0 || accumulator & 0x0f > 9 {
            correction |= 0x06;
        }
        let carry = if old_flags & FLAG_C != 0 || accumulator > 0x99 {
            correction |= 0x60;
            true
        } else {
            false
        };

        let result = if subtract {
            accumulator.wrapping_sub(correction)
        } else {
            accumulator.wrapping_add(correction)
        };
        let mut flags =
            Self::sign_zero_xy(result) | Self::parity_flag(result) | (old_flags & FLAG_N);
        if (accumulator ^ result) & 0x10 != 0 {
            flags |= FLAG_H;
        }
        if carry {
            flags |= FLAG_C;
        }
        self.set_accumulator_and_flags(result, flags);
    }

    pub(crate) fn execute_cpl(&mut self, _opcode: u8) {
        let accumulator = !self.accumulator();
        let flags = (self.flags() & (FLAG_S | FLAG_Z | FLAG_PV | FLAG_C))
            | (accumulator & FLAG_XY)
            | FLAG_H
            | FLAG_N;
        self.set_accumulator_and_flags(accumulator, flags);
    }

    pub(crate) fn execute_scf(&mut self, _opcode: u8) {
        let flags =
            (self.flags() & (FLAG_S | FLAG_Z | FLAG_PV)) | (self.accumulator() & FLAG_XY) | FLAG_C;
        self.set_flags(flags);
    }

    pub(crate) fn execute_ccf(&mut self, _opcode: u8) {
        let old_carry = self.flags() & FLAG_C;
        let mut flags =
            (self.flags() & (FLAG_S | FLAG_Z | FLAG_PV)) | (self.accumulator() & FLAG_XY);
        if old_carry != 0 {
            flags |= FLAG_H;
        } else {
            flags |= FLAG_C;
        }
        self.set_flags(flags);
    }

    pub(crate) fn execute_ld_block(&mut self, opcode: u8) {
        let destination = (opcode >> 3) & 0x07;
        let source = opcode & 0x07;
        let value = self.read_reg8(source);
        self.write_reg8(destination, value);
    }

    pub(crate) fn execute_alu_reg8(&mut self, opcode: u8) {
        let value = self.read_reg8(opcode & 0x07);
        self.execute_alu((opcode >> 3) & 0x07, value);
    }

    pub(crate) fn execute_alu_immediate(&mut self, opcode: u8) {
        let value = self.immediate8();
        self.execute_alu((opcode >> 3) & 0x07, value);
    }

    pub(crate) fn execute_ex_af(&mut self, _opcode: u8) {
        let primary = self.registers.get(Reg::AF);
        let alternate = self.registers.get(Reg::AF2);
        self.registers.set(Reg::AF, alternate);
        self.registers.set(Reg::AF2, primary);
    }

    pub(crate) fn execute_djnz(&mut self, _opcode: u8) {
        let [b, c] = self.registers.get(Reg::BC).to_be_bytes();
        let next_b = b.wrapping_sub(1);
        self.registers.set(Reg::BC, u16::from_be_bytes([next_b, c]));
        let displacement = self.immediate8();
        if next_b != 0 {
            self.timing_branch_taken = true;
            self.registers
                .set(Reg::PC, self.relative_target(displacement));
        }
    }

    pub(crate) fn execute_jr(&mut self, _opcode: u8) {
        let displacement = self.immediate8();
        self.registers
            .set(Reg::PC, self.relative_target(displacement));
    }

    pub(crate) fn execute_jr_condition(&mut self, opcode: u8) {
        let displacement = self.immediate8();
        if self.condition((opcode >> 3) & 0x03) {
            self.timing_branch_taken = true;
            self.registers
                .set(Reg::PC, self.relative_target(displacement));
        }
    }

    pub(crate) fn execute_ret_condition(&mut self, opcode: u8) {
        if self.condition((opcode >> 3) & 0x07) {
            self.timing_branch_taken = true;
            let target = self.pop_word();
            self.registers.set(Reg::PC, target);
        }
    }

    pub(crate) fn execute_pop(&mut self, opcode: u8) {
        let value = self.pop_word();
        self.set_stack_reg16(opcode >> 4, value);
    }

    pub(crate) fn execute_jp_condition(&mut self, opcode: u8) {
        let target = self.immediate16();
        if self.condition((opcode >> 3) & 0x07) {
            self.timing_branch_taken = true;
            self.registers.set(Reg::PC, target);
        }
    }

    pub(crate) fn execute_call_condition(&mut self, opcode: u8) {
        let target = self.immediate16();
        if self.condition((opcode >> 3) & 0x07) {
            self.timing_branch_taken = true;
            self.push_word(self.registers.get(Reg::PC));
            self.registers.set(Reg::PC, target);
        }
    }

    pub(crate) fn execute_push(&mut self, opcode: u8) {
        self.push_word(self.stack_reg16(opcode >> 4));
    }

    pub(crate) fn execute_rst(&mut self, opcode: u8) {
        self.push_word(self.registers.get(Reg::PC));
        self.registers.set(Reg::PC, u16::from(opcode & 0x38));
    }

    pub(crate) fn execute_jp(&mut self, _opcode: u8) {
        let target = self.immediate16();
        self.registers.set(Reg::PC, target);
    }

    pub(crate) fn execute_ret(&mut self, _opcode: u8) {
        let target = self.pop_word();
        self.registers.set(Reg::PC, target);
    }

    pub(crate) fn execute_call(&mut self, _opcode: u8) {
        let target = self.immediate16();
        self.push_word(self.registers.get(Reg::PC));
        self.registers.set(Reg::PC, target);
    }

    pub(crate) fn execute_out_immediate(&mut self, _opcode: u8) {
        let accumulator = self.accumulator();
        let port = u16::from_be_bytes([accumulator, self.immediate8()]);
        self.write_io(port, accumulator);
    }

    pub(crate) fn execute_exx(&mut self, _opcode: u8) {
        for (primary, alternate) in [
            (Reg::BC, Reg::BC2),
            (Reg::DE, Reg::DE2),
            (Reg::HL, Reg::HL2),
        ] {
            let primary_value = self.registers.get(primary);
            let alternate_value = self.registers.get(alternate);
            self.registers.set(primary, alternate_value);
            self.registers.set(alternate, primary_value);
        }
    }

    pub(crate) fn execute_in_immediate(&mut self, _opcode: u8) {
        let accumulator = self.accumulator();
        let port = u16::from_be_bytes([accumulator, self.immediate8()]);
        let value = self.read_io(port);
        self.set_accumulator(value);
    }

    pub(crate) fn execute_ex_sp_hl(&mut self, _opcode: u8) {
        let sp = self.registers.get(Reg::SP);
        let memory_value = self.read_word(sp);
        let hl = self.registers.get(Reg::HL);
        self.write_word(sp, hl);
        self.registers.set(Reg::HL, memory_value);
    }

    pub(crate) fn execute_jp_hl(&mut self, _opcode: u8) {
        self.registers.set(Reg::PC, self.registers.get(Reg::HL));
    }

    pub(crate) fn execute_ex_de_hl(&mut self, _opcode: u8) {
        let de = self.registers.get(Reg::DE);
        let hl = self.registers.get(Reg::HL);
        self.registers.set(Reg::DE, hl);
        self.registers.set(Reg::HL, de);
    }

    pub(crate) fn execute_di(&mut self, _opcode: u8) {
        self.iff1 = false;
        self.iff2 = false;
        self.ei_shadow = false;
    }

    pub(crate) fn execute_ld_sp_hl(&mut self, _opcode: u8) {
        self.registers.set(Reg::SP, self.registers.get(Reg::HL));
    }

    pub(crate) fn execute_ei(&mut self, _opcode: u8) {
        self.iff1 = true;
        self.iff2 = true;
        self.ei_shadow = true;
    }

    fn read_logical(&mut self, logical: u16) -> u8 {
        self.timing_memory_waits = self
            .timing_memory_waits
            .saturating_add(u32::from((self.io_regs[DCNTL] >> 6) & 0x03));
        let physical = self.map_external_address(self.mmu_translate(logical));
        let value = self.emulation_mem_read(physical);
        self.capture_insn_byte(logical, value);
        value
    }

    fn write_logical(&mut self, logical: u16, value: u8) {
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

    fn emulation_mem_read(&mut self, physical: u32) -> u8 {
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

    fn emulation_mem_write(&mut self, physical: u32, value: u8) {
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

    fn internal_io_index(&self, port: u16) -> Option<usize> {
        let [high, low] = port.to_be_bytes();
        let base = self.io_regs[ICR] & 0xc0;
        if high == 0 && low >= base && low <= base | 0x3f {
            Some(usize::from(low - base))
        } else {
            None
        }
    }

    fn read_io(&mut self, port: u16) -> u8 {
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

    fn read_internal_io(&mut self, index: usize) -> u8 {
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

    fn write_io(&mut self, port: u16, value: u8) {
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

    fn write_internal_io(&mut self, index: usize, value: u8) {
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

    fn asci_cntlb_value(&self, index: usize) -> u8 {
        let channel = index - CNTLB0;
        let cts_visible = channel == 0 || self.io_regs[STAT1] & 0x04 != 0;
        (self.io_regs[index] & !0x20)
            | if cts_visible && self.asci_cts[channel] {
                0x20
            } else {
                0
            }
    }

    fn asci_status_value(&self, channel: usize) -> u8 {
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

    fn asci_dcd_auto_enabled(&self) -> bool {
        self.variant != Variant::Z8S180 || self.io_regs[0x12] & 0x40 == 0
    }

    fn asci_receiver_enabled(&self, channel: usize) -> bool {
        let enabled = self.io_regs[ICR] & 0x20 == 0 && self.io_regs[CNTLA0 + channel] & 0x40 != 0;
        let dcd_inhibits = channel == 0 && self.asci_dcd_auto_enabled() && self.asci_dcd_latched;
        enabled && !dcd_inhibits
    }

    fn asci_frame_cycles(&self, channel: usize) -> Option<u64> {
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

    fn sync_asci_status(&mut self, channel: usize) {
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

    fn update_asci_interrupt_requests(&mut self) {
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

    fn abort_asci_receive(&mut self, channel: usize, clear_status: bool) {
        self.asci_rx_shift[channel] = None;
        self.asci_rx_cycles[channel] = 0;
        self.asci_rx_clocked[channel] = false;
        if clear_status {
            self.asci_rx_fifo[channel].clear();
            self.io_regs[STAT0 + channel] &= !0xf0;
        }
        self.sync_asci_status(channel);
    }

    fn start_asci_transmit(&mut self, channel: usize) {
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

    fn apply_asci_cntla_write(&mut self, channel: usize, old: u8) {
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

    fn stop_asci_for_iostop(&mut self) {
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

    fn csio_transfer_cycles(&self) -> Option<u64> {
        let speed = self.io_regs[CNTR] & 0x07;
        if speed == 0x07 {
            None
        } else {
            Some(8 * (20_u64 << speed))
        }
    }

    fn update_csio_interrupt_request(&mut self) {
        self.internal_irq_pending &= !0x10;
        if self.io_regs[CNTR] & 0xc0 == 0xc0 {
            self.internal_irq_pending |= 0x10;
        }
    }

    fn abort_csio_receive(&mut self) {
        self.io_regs[CNTR] &= !0x20;
        self.csio_rx_shift = None;
        self.csio_cycles = 0;
        self.csio_clocked = false;
    }

    fn apply_csio_cntr_write(&mut self, old: u8) {
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

    fn stop_csio_for_iostop(&mut self) {
        self.io_regs[CNTR] &= !0xb0;
        self.csio_rx_shift = None;
        self.csio_cycles = 0;
        self.csio_clocked = false;
        self.update_csio_interrupt_request();
    }

    fn advance_csio(&mut self, cycles: u32) {
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

    fn advance_asci(&mut self, cycles: u32) {
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

    fn recompute_mmu_pages(&mut self) {
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

    fn update_prt_interrupt_requests(&mut self) {
        self.internal_irq_pending &= !0x03;
        if self.io_regs[TCR] & 0x50 == 0x50 {
            self.internal_irq_pending |= 0x01;
        }
        if self.io_regs[TCR] & 0xa0 == 0xa0 {
            self.internal_irq_pending |= 0x02;
        }
    }

    fn advance_prt(&mut self, cycles: u32) {
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

    fn dma_io_read(&mut self, port: u16) -> u8 {
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

    fn dma_io_write(&mut self, port: u16, value: u8) {
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

    fn service_dma(&mut self) -> u32 {
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

    fn update_dma_interrupt_requests(&mut self) {
        self.internal_irq_pending &= !0x0c;
        if self.io_regs[DSTAT] & 0x44 == 0x04 {
            self.internal_irq_pending |= 0x04;
        }
        if self.io_regs[DSTAT] & 0x88 == 0x08 {
            self.internal_irq_pending |= 0x08;
        }
    }

    fn wait_cycles(&self) -> u32 {
        self.timing_memory_waits
            .saturating_add(self.timing_io_waits)
    }

    fn finish_step(&mut self, cycles: u32) -> u32 {
        let frc_cycles = self.frc_cycle_remainder.saturating_add(cycles);
        let frc_ticks = frc_cycles / 10;
        self.frc_cycle_remainder = frc_cycles % 10;
        self.io_regs[FRC] = self.io_regs[FRC].wrapping_sub(frc_ticks as u8);
        self.advance_prt(cycles);
        self.advance_asci(cycles);
        self.advance_csio(cycles);
        self.cycle_count = self.cycle_count.saturating_add(u64::from(cycles));
        cycles
    }
}

impl<B: HostBus<Error = Infallible>> Z180<B> {
    pub fn step(&mut self) -> u32 {
        match self.try_step() {
            Ok(cycles) => cycles,
            Err(error) => match error {},
        }
    }

    pub fn run(&mut self, cycles: u32) -> u32 {
        match self.try_run(cycles) {
            Ok(consumed) => consumed,
            Err(error) => match error {},
        }
    }
}

#[cfg(test)]
mod tests;
