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
mod access;
mod disassembler;
mod instructions;
mod interrupts;
mod ioregs;
mod memory;
mod optable;
mod peripherals;
mod registers;
#[cfg(feature = "state")]
mod state;
#[cfg(feature = "state")]
pub use state::SaveStateError;
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
    pub fn new(config: MachineConfig, bus: B) -> Result<Self, ConfigError> {
        let mut io_regs = [0; IO_REGISTER_COUNT];
        for (index, spec) in IO_REG_SPECS.iter().copied().enumerate() {
            if spec.is_available(config.variant) {
                io_regs[index] = spec.reset;
            }
        }
        let variant = config.variant;
        let event_capacity = config.event_capacity;
        let memory = Memory::new(config)?;
        let mut cpu = Self {
            registers: Registers::default(),
            memory,
            bus,
            bus_error: None,
            instruction_pc: 0,
            indexed_displacement: None,
            cycle_count: 0,
            variant,
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
            event_capacity,
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

    pub fn mmu_translate(&self, logical: u16) -> u32 {
        let page = usize::from(logical >> 12);
        self.mmu_pages[page] + u32::from(logical & 0x0fff)
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
