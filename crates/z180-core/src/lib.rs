#![forbid(unsafe_code)]
#![no_std]

extern crate alloc;

use alloc::{collections::VecDeque, vec::Vec};

mod ioregs;
mod memory;
mod optable;
mod registers;

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
    fn mem_read(&mut self, phys: u32) -> u8;
    fn mem_write(&mut self, phys: u32, value: u8);
    fn io_read(&mut self, port: u16) -> u8;
    fn io_write(&mut self, port: u16, value: u8);
}

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
#[derive(Clone, Copy)]
struct MemWatch {
    id: WatchId,
    base: u32,
    size: u32,
    kind: WatchKind,
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

struct TraceCapture {
    entry: TraceEntry,
    captured: u8,
}

#[cfg(feature = "state")]
const STATE_VERSION: u8 = 3;

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

pub struct Z180<B: HostBus> {
    registers: Registers,
    memory: Memory,
    bus: B,
    instruction_pc: u16,
    cycle_count: u64,
    variant: Variant,
    io_regs: [u8; IO_REGISTER_COUNT],
    mmu_pages: [u32; 16],
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

#[cfg(feature = "state")]
#[derive(serde::Deserialize, serde::Serialize)]
struct SavedState {
    registers: Registers,
    memory: Memory,
    instruction_pc: u16,
    cycle_count: u64,
    variant: Variant,
    io_regs: Vec<u8>,
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
    events: Vec<Event>,
    events_lost: bool,
    mem_watches: Vec<MemWatch>,
    next_watch_id: u64,
    io_trace: bool,
    irq_trace: bool,
    pc_watch: Option<u16>,
    pc_watch_hits: u64,
    insn_trace_capacity: Option<usize>,
    insn_trace: Vec<TraceEntry>,
}

impl<B: HostBus> Z180<B> {
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
            instruction_pc: 0,
            cycle_count: 0,
            variant: config.variant,
            io_regs,
            mmu_pages: [0; 16],
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

    pub fn step(&mut self) -> u32 {
        self.timing_branch_taken = false;
        self.timing_repeat_iterations = 0;
        self.timing_memory_waits = 0;
        self.timing_io_waits = 0;

        let dma_cycles = self.service_dma();
        if dma_cycles != 0 {
            self.finish_step(dma_cycles);
        }

        if let Some(cycles) = self.interrupt_check_point() {
            return dma_cycles
                .saturating_add(self.finish_step(cycles.saturating_add(self.wait_cycles())));
        }

        if self.halted {
            let _ = self.read_logical(self.registers.get(Reg::PC));
            return dma_cycles.saturating_add(
                self.finish_step(u32::from(HALT_IDLE_CYCLES).saturating_add(self.wait_cycles())),
            );
        }
        if self.sleeping {
            return dma_cycles;
        }

        let pc = self.registers.get(Reg::PC);
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
        let Some(handler) = descriptor.handler else {
            if is_indexed_bit {
                let displacement = self.read_logical(pc.wrapping_add(2)) as i8;
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
                self.take_trap([first_opcode, 0xcb, opcode], 3, pc.wrapping_add(4), true, 3);
            } else if matches!(first_opcode, 0xcb | 0xdd | 0xed | 0xfd) {
                self.take_trap([first_opcode, opcode, 0], 2, pc.wrapping_add(2), false, 2);
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
            return dma_cycles.saturating_add(
                self.finish_step(u32::from(cycles).saturating_add(self.wait_cycles())),
            );
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
        dma_cycles.saturating_add(self.finish_step(cycles.saturating_add(self.wait_cycles())))
    }

    pub fn run(&mut self, cycles: u32) -> u32 {
        let mut consumed = 0_u32;
        while consumed < cycles {
            let step_cycles = self.step();
            if step_cycles == 0 {
                break;
            }
            consumed = consumed.saturating_add(step_cycles);
        }
        consumed
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

    pub fn add_mem_watch(&mut self, base: u32, size: u32, kind: WatchKind) -> WatchId {
        let id = WatchId(self.next_watch_id);
        self.next_watch_id = self.next_watch_id.wrapping_add(1);
        if self.next_watch_id == 0 {
            self.next_watch_id = 1;
        }
        self.mem_watches.push(MemWatch {
            id,
            base,
            size,
            kind,
        });
        let _ = self.ensure_event_storage();
        id
    }

    pub fn remove_mem_watch(&mut self, id: WatchId) {
        self.mem_watches.retain(|watch| watch.id != id);
    }

    pub fn set_io_trace(&mut self, enabled: bool) {
        self.io_trace = enabled;
        if enabled {
            let _ = self.ensure_event_storage();
        }
    }

    pub fn set_irq_trace(&mut self, enabled: bool) {
        self.irq_trace = enabled;
        if enabled {
            let _ = self.ensure_event_storage();
        }
    }

    pub fn set_pc_watch(&mut self, addr: Option<u16>) {
        self.pc_watch = addr;
        self.pc_watch_hits = 0;
    }

    pub fn pc_watch_hits(&self) -> u64 {
        self.pc_watch_hits
    }

    pub fn drain_events(&mut self) -> Vec<Event> {
        let mut drained = Vec::with_capacity(self.events.len());
        drained.extend(self.events.drain(..));
        drained
    }

    pub fn events_lost(&self) -> bool {
        self.events_lost
    }

    pub fn clear_events_lost(&mut self) {
        self.events_lost = false;
    }

    pub fn set_insn_trace(&mut self, capacity: Option<usize>) {
        self.insn_trace_capture = None;
        let Some(capacity) = capacity else {
            self.insn_trace_capacity = None;
            self.insn_trace = VecDeque::new();
            return;
        };

        if self.insn_trace.capacity() < capacity
            && self
                .insn_trace
                .try_reserve_exact(capacity - self.insn_trace.len())
                .is_err()
        {
            return;
        }
        while self.insn_trace.len() > capacity {
            let _ = self.insn_trace.pop_front();
        }
        self.insn_trace_capacity = Some(capacity);
    }

    pub fn drain_insn_trace(&mut self) -> Vec<TraceEntry> {
        let mut drained = Vec::with_capacity(self.insn_trace.len());
        drained.extend(self.insn_trace.drain(..));
        drained
    }

    fn begin_insn_trace(&mut self, pc: u16) {
        let Some(capacity) = self.insn_trace_capacity else {
            return;
        };
        if capacity == 0 {
            return;
        }
        self.insn_trace_capture = Some(TraceCapture {
            entry: TraceEntry {
                cycle: self.cycle_count,
                pc,
                phys_pc: self.mmu_translate(pc),
                bytes: [0; 4],
                len: 0,
            },
            captured: 0,
        });
    }

    fn capture_insn_byte(&mut self, logical: u16, value: u8) {
        let Some(capture) = &mut self.insn_trace_capture else {
            return;
        };
        let offset = logical.wrapping_sub(capture.entry.pc);
        if offset >= 4 {
            return;
        }
        let bit = 1_u8 << offset;
        if capture.captured & bit == 0 {
            capture.entry.bytes[usize::from(offset)] = value;
            capture.captured |= bit;
        }
    }

    fn finish_insn_trace(&mut self, len: u8) {
        let Some(mut capture) = self.insn_trace_capture.take() else {
            return;
        };
        let used = usize::from(len).min(capture.entry.bytes.len());
        capture.entry.len = used as u8;
        for byte in &mut capture.entry.bytes[used..] {
            *byte = 0;
        }
        self.push_insn_trace(capture.entry);
    }

    fn push_insn_trace(&mut self, entry: TraceEntry) {
        let Some(capacity) = self.insn_trace_capacity else {
            return;
        };
        if capacity == 0 {
            return;
        }
        if self.insn_trace.capacity() < capacity
            && self
                .insn_trace
                .try_reserve_exact(capacity - self.insn_trace.len())
                .is_err()
        {
            return;
        }
        if self.insn_trace.len() == capacity {
            let _ = self.insn_trace.pop_front();
        }
        self.insn_trace.push_back(entry);
    }

    fn ensure_event_storage(&mut self) -> bool {
        if self.event_capacity == 0 || self.events.capacity() >= self.event_capacity {
            return self.event_capacity != 0;
        }
        self.events
            .try_reserve_exact(self.event_capacity - self.events.len())
            .is_ok()
    }

    fn push_event(&mut self, event: Event) {
        if !self.ensure_event_storage() {
            self.events_lost = true;
            return;
        }
        if self.events.len() == self.event_capacity {
            let _ = self.events.pop_front();
            self.events_lost = true;
        }
        self.events.push_back(event);
    }

    #[cfg(feature = "state")]
    pub fn save_state(&self) -> Vec<u8> {
        let state = SavedState {
            registers: self.registers,
            memory: self.memory.clone(),
            instruction_pc: self.instruction_pc,
            cycle_count: self.cycle_count,
            variant: self.variant,
            io_regs: self.io_regs.to_vec(),
            timing_branch_taken: self.timing_branch_taken,
            timing_repeat_iterations: self.timing_repeat_iterations,
            timing_memory_waits: self.timing_memory_waits,
            timing_io_waits: self.timing_io_waits,
            halted: self.halted,
            sleeping: self.sleeping,
            iff1: self.iff1,
            iff2: self.iff2,
            ei_shadow: self.ei_shadow,
            interrupt_mode: self.interrupt_mode,
            irq_lines: self.irq_lines,
            nmi_level: self.nmi_level,
            nmi_pending: self.nmi_pending,
            dreq_level: self.dreq_level,
            dreq_edge_pending: self.dreq_edge_pending,
            internal_irq_pending: self.internal_irq_pending,
            frc_cycle_remainder: self.frc_cycle_remainder,
            prt_cycle_remainder: self.prt_cycle_remainder,
            prt_high_latch: self.prt_high_latch,
            prt_high_latch_valid: self.prt_high_latch_valid,
            prt_clear_armed: self.prt_clear_armed,
            asci_cts: self.asci_cts,
            asci_dcd: self.asci_dcd,
            asci_dcd_latched: self.asci_dcd_latched,
            asci_dcd_irq_pending: self.asci_dcd_irq_pending,
            asci_tdr_full: self.asci_tdr_full,
            asci_tx_shift: self.asci_tx_shift,
            asci_tx_cycles: self.asci_tx_cycles,
            asci_tx_clocked: self.asci_tx_clocked,
            asci_tx_output: self.asci_tx_output.clone(),
            asci_rx_shift: self.asci_rx_shift,
            asci_rx_cycles: self.asci_rx_cycles,
            asci_rx_clocked: self.asci_rx_clocked,
            asci_rx_fifo: self.asci_rx_fifo.clone(),
            csio_rx_shift: self.csio_rx_shift,
            csio_cycles: self.csio_cycles,
            csio_clocked: self.csio_clocked,
            csio_tx_output: self.csio_tx_output.clone(),
            event_capacity: self.event_capacity,
            events: self.events.iter().cloned().collect(),
            events_lost: self.events_lost,
            mem_watches: self.mem_watches.clone(),
            next_watch_id: self.next_watch_id,
            io_trace: self.io_trace,
            irq_trace: self.irq_trace,
            pc_watch: self.pc_watch,
            pc_watch_hits: self.pc_watch_hits,
            insn_trace_capacity: self.insn_trace_capacity,
            insn_trace: self.insn_trace.iter().cloned().collect(),
        };

        let mut bytes = Vec::new();
        bytes.push(STATE_VERSION);
        if let Ok(payload) = postcard::to_allocvec(&state) {
            bytes.extend_from_slice(&payload);
        }
        bytes
    }

    #[cfg(feature = "state")]
    pub fn load_state(&mut self, data: &[u8]) -> Result<(), StateError> {
        let Some((&version, payload)) = data.split_first() else {
            return Err(StateError::MissingVersion);
        };
        if version != STATE_VERSION {
            return Err(StateError::UnsupportedVersion(version));
        }
        let state: SavedState = postcard::from_bytes(payload).map_err(|_| StateError::Decode)?;
        let io_regs: [u8; IO_REGISTER_COUNT] = state
            .io_regs
            .as_slice()
            .try_into()
            .map_err(|_| StateError::Decode)?;
        let insn_trace_is_valid = match state.insn_trace_capacity {
            Some(capacity) => state.insn_trace.len() <= capacity,
            None => state.insn_trace.is_empty(),
        };
        if state.events.len() > state.event_capacity
            || state.next_watch_id == 0
            || !insn_trace_is_valid
        {
            return Err(StateError::Decode);
        }
        let mut events = VecDeque::new();
        if events.try_reserve_exact(state.event_capacity).is_err() {
            return Err(StateError::Decode);
        }
        events.extend(state.events.iter().cloned());
        let mut insn_trace = VecDeque::new();
        if let Some(capacity) = state.insn_trace_capacity
            && insn_trace.try_reserve_exact(capacity).is_err()
        {
            return Err(StateError::Decode);
        }
        insn_trace.extend(state.insn_trace.iter().cloned());

        self.registers = state.registers;
        self.memory = state.memory;
        self.instruction_pc = state.instruction_pc;
        self.cycle_count = state.cycle_count;
        self.variant = state.variant;
        self.io_regs = io_regs;
        self.recompute_mmu_pages();
        self.timing_branch_taken = state.timing_branch_taken;
        self.timing_repeat_iterations = state.timing_repeat_iterations;
        self.timing_memory_waits = state.timing_memory_waits;
        self.timing_io_waits = state.timing_io_waits;
        self.halted = state.halted;
        self.sleeping = state.sleeping;
        self.iff1 = state.iff1;
        self.iff2 = state.iff2;
        self.ei_shadow = state.ei_shadow;
        self.interrupt_mode = state.interrupt_mode;
        self.irq_lines = state.irq_lines;
        self.nmi_level = state.nmi_level;
        self.nmi_pending = state.nmi_pending;
        self.dreq_level = state.dreq_level;
        self.dreq_edge_pending = state.dreq_edge_pending;
        self.internal_irq_pending = state.internal_irq_pending;
        self.frc_cycle_remainder = state.frc_cycle_remainder;
        self.prt_cycle_remainder = state.prt_cycle_remainder;
        self.prt_high_latch = state.prt_high_latch;
        self.prt_high_latch_valid = state.prt_high_latch_valid;
        self.prt_clear_armed = state.prt_clear_armed;
        self.asci_cts = state.asci_cts;
        self.asci_dcd = state.asci_dcd;
        self.asci_dcd_latched = state.asci_dcd_latched;
        self.asci_dcd_irq_pending = state.asci_dcd_irq_pending;
        self.asci_tdr_full = state.asci_tdr_full;
        self.asci_tx_shift = state.asci_tx_shift;
        self.asci_tx_cycles = state.asci_tx_cycles;
        self.asci_tx_clocked = state.asci_tx_clocked;
        self.asci_tx_output = state.asci_tx_output;
        self.asci_rx_shift = state.asci_rx_shift;
        self.asci_rx_cycles = state.asci_rx_cycles;
        self.asci_rx_clocked = state.asci_rx_clocked;
        self.asci_rx_fifo = state.asci_rx_fifo;
        self.csio_rx_shift = state.csio_rx_shift;
        self.csio_cycles = state.csio_cycles;
        self.csio_clocked = state.csio_clocked;
        self.csio_tx_output = state.csio_tx_output;
        self.event_capacity = state.event_capacity;
        self.events = events;
        self.events_lost = state.events_lost;
        self.mem_watches = state.mem_watches;
        self.next_watch_id = state.next_watch_id;
        self.io_trace = state.io_trace;
        self.irq_trace = state.irq_trace;
        self.pc_watch = state.pc_watch;
        self.pc_watch_hits = state.pc_watch_hits;
        self.insn_trace_capacity = state.insn_trace_capacity;
        self.insn_trace = insn_trace;
        self.insn_trace_capture = None;
        Ok(())
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

    pub fn mem_peek(&self, phys: u32) -> u8 {
        self.memory.peek(phys)
    }

    pub fn mem_poke(&mut self, phys: u32, value: u8) {
        self.memory.poke(&mut self.bus, phys, value);
    }

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
                    IrqSource::Int1 => 0x00,
                    IrqSource::Int2 => 0x02,
                    IrqSource::Prt0 => 0x04,
                    IrqSource::Prt1 => 0x06,
                    IrqSource::Dma0 => 0x08,
                    IrqSource::Dma1 => 0x0a,
                    IrqSource::Csio => 0x0c,
                    IrqSource::Asci0 => 0x0e,
                    IrqSource::Asci1 => 0x10,
                    IrqSource::Nmi | IrqSource::Int0 => 0,
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
        let new_sp = self.registers.get(Reg::SP).wrapping_sub(2);
        self.write_word(new_sp, value);
        self.registers.set(Reg::SP, new_sp);
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
        if value & 0x0f == 0 {
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
                    if value & 0x0f == 0 {
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
        let displacement = self.read_logical(self.instruction_pc.wrapping_add(2)) as i8;
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
                            if next_b & 0x0f == 0 {
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
        let physical = self.mmu_translate(logical);
        let value = self.emulation_mem_read(physical);
        self.capture_insn_byte(logical, value);
        value
    }

    fn write_logical(&mut self, logical: u16, value: u8) {
        self.timing_memory_waits = self
            .timing_memory_waits
            .saturating_add(u32::from((self.io_regs[DCNTL] >> 6) & 0x03));
        let physical = self.mmu_translate(logical);
        self.emulation_mem_write(physical, value);
    }

    fn emulation_mem_read(&mut self, physical: u32) -> u8 {
        let value = self.memory.read(&mut self.bus, physical);
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
        let rom_write = self.memory.write(&mut self.bus, physical, value);
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
        let value = if let Some(index) = self.internal_io_index(port) {
            let _ = self.bus.io_read(port);
            self.read_internal_io(index)
        } else {
            self.timing_io_waits = self
                .timing_io_waits
                .saturating_add(u32::from(((self.io_regs[DCNTL] >> 4) & 0x03) + 1));
            self.bus.io_read(port)
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
            ReadEffect::AsciCntlb => value,
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
            ReadEffect::None => value,
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
        if let Some(index) = self.internal_io_index(port) {
            self.bus.io_write(port, value);
            self.write_internal_io(index, value);
        } else {
            self.timing_io_waits = self
                .timing_io_waits
                .saturating_add(u32::from(((self.io_regs[DCNTL] >> 4) & 0x03) + 1));
            self.bus.io_write(port, value);
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
        let cntla = self.io_regs[CNTLA0 + channel];
        let cntlb = self.io_regs[CNTLB0 + channel];
        let asext = if self.variant == Variant::Z8S180 {
            self.io_regs[0x12 + channel]
        } else {
            0
        };
        let clock_mode = if asext & 0x10 != 0 {
            1_u64
        } else if cntlb & 0x08 == 0 {
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
            let divisor = cntlb & 0x07;
            if divisor == 0x07 {
                return None;
            }
            let prescale = if cntlb & 0x20 == 0 { 10_u64 } else { 30 };
            prescale * (1_u64 << divisor) * clock_mode
        };

        let data_bits = if cntla & 0x04 != 0 { 8_u64 } else { 7 };
        let parity_or_mp = u64::from(cntla & 0x02 != 0 || cntlb & 0x40 != 0);
        let stop_bits = if cntla & 0x01 != 0 { 2_u64 } else { 1 };
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
        if destination_mode == 3 {
            self.dma_io_write(destination as u16, byte);
        } else {
            self.emulation_mem_write(destination, byte);
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
            1 => (source.wrapping_sub(1) & 0x000f_ffff, source & 0xffff == 0),
            _ => (source, false),
        };
        let (next_destination, destination_crossed) = match destination_mode {
            0 => (
                destination.wrapping_add(1) & 0x000f_ffff,
                destination & 0xffff == 0xffff,
            ),
            1 => (
                destination.wrapping_sub(1) & 0x000f_ffff,
                destination & 0xffff == 0,
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
            self.dma_io_write(io, byte);
        } else {
            let byte = self.dma_io_read(io);
            self.emulation_mem_write(memory, byte);
        }

        let memory_waits = u32::from((self.io_regs[DCNTL] >> 6) & 0x03);
        let io_waits = u32::from(((self.io_regs[DCNTL] >> 4) & 0x03) + 1);
        let decrements = mode & 0x01 != 0;
        let crossed = if decrements {
            memory & 0xffff == 0
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
        let value = if let Some(index) = self.internal_io_index(port) {
            let _ = self.bus.io_read(port);
            self.read_internal_io(index)
        } else {
            self.bus.io_read(port)
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
        if let Some(index) = self.internal_io_index(port) {
            self.bus.io_write(port, value);
            self.write_internal_io(index, value);
        } else {
            self.bus.io_write(port, value);
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

#[cfg(test)]
mod tests {
    use alloc::vec;

    use proptest::prelude::*;

    use super::*;

    #[derive(Default)]
    struct NullBus;

    impl HostBus for NullBus {
        fn mem_read(&mut self, _phys: u32) -> u8 {
            0xff
        }

        fn mem_write(&mut self, _phys: u32, _value: u8) {}

        fn io_read(&mut self, _port: u16) -> u8 {
            0xff
        }

        fn io_write(&mut self, _port: u16, _value: u8) {}
    }

    #[derive(Default)]
    struct RecordingBus {
        io_read_value: u8,
        io_reads: Vec<u16>,
        io_writes: Vec<(u16, u8)>,
    }

    impl HostBus for RecordingBus {
        fn mem_read(&mut self, _phys: u32) -> u8 {
            0xff
        }

        fn mem_write(&mut self, _phys: u32, _value: u8) {}

        fn io_read(&mut self, port: u16) -> u8 {
            self.io_reads.push(port);
            self.io_read_value
        }

        fn io_write(&mut self, port: u16, value: u8) {
            self.io_writes.push((port, value));
        }
    }

    fn machine() -> Z180<NullBus> {
        let config = MachineConfig {
            regions: vec![RegionDef {
                base: 0,
                size: 0x1_0000,
                kind: RegionKind::Ram,
            }],
            ..MachineConfig::default()
        };
        Z180::new(config, NullBus).expect("flat RAM configuration must be valid")
    }

    fn recording_machine(variant: Variant) -> Z180<RecordingBus> {
        let config = MachineConfig {
            variant,
            regions: vec![RegionDef {
                base: 0,
                size: 0x1_0000,
                kind: RegionKind::Ram,
            }],
            ..MachineConfig::default()
        };
        Z180::new(config, RecordingBus::default())
            .expect("flat recording configuration must be valid")
    }

    fn mmu_machine() -> Z180<NullBus> {
        let config = MachineConfig {
            regions: vec![RegionDef {
                base: 0,
                size: 0x10_0000,
                kind: RegionKind::Ram,
            }],
            ..MachineConfig::default()
        };
        Z180::new(config, NullBus).expect("1 MiB RAM configuration must be valid")
    }

    #[test]
    fn mmu_reset_and_internal_io_writes_recompute_all_pages() {
        let mut cpu = mmu_machine();
        for logical_page in 0_u16..16 {
            let logical = (logical_page << 12) | 0x0a5;
            assert_eq!(cpu.mmu_translate(logical), u32::from(logical));
        }

        for (pc, register, value) in [(0_u16, CBR, 0x80_u8), (3, CBAR, 0xa4_u8), (6, BBR, 0x40_u8)]
        {
            let physical = cpu.mmu_translate(pc);
            cpu.mem_poke(physical, 0xed);
            cpu.mem_poke(physical + 1, 0x01);
            cpu.mem_poke(physical + 2, register as u8);
            cpu.set_reg(Reg::BC, u16::from(value) << 8);
            assert_ne!(cpu.step(), 0);
        }

        assert_eq!(cpu.io_reg_peek(CBR as u8), 0x80);
        assert_eq!(cpu.io_reg_peek(BBR as u8), 0x40);
        assert_eq!(cpu.io_reg_peek(CBAR as u8), 0xa4);
        assert_eq!(cpu.mmu_translate(0x30a5), 0x030a5);
        assert_eq!(cpu.mmu_translate(0x40a5), 0x440a5);
        assert_eq!(cpu.mmu_translate(0x90a5), 0x490a5);
        assert_eq!(cpu.mmu_translate(0xa0a5), 0x8a0a5);

        cpu.reset();
        assert_eq!(cpu.io_reg_peek(CBR as u8), 0x00);
        assert_eq!(cpu.io_reg_peek(BBR as u8), 0x00);
        assert_eq!(cpu.io_reg_peek(CBAR as u8), 0xf0);
        for logical_page in 0_u16..16 {
            let logical = (logical_page << 12) | 0x0a5;
            assert_eq!(cpu.mmu_translate(logical), u32::from(logical));
        }
    }

    #[test]
    fn mmu_translates_instruction_fetches_reads_and_writes() {
        let mut cpu = mmu_machine();
        cpu.write_internal_io(CBR, 0x10);
        cpu.write_internal_io(CBAR, 0x00);

        cpu.mem_poke(0x1_0000, 0x7e);
        cpu.mem_poke(0x1_2000, 0x5a);
        cpu.set_reg(Reg::HL, 0x2000);
        assert_ne!(cpu.step(), 0);
        assert_eq!(cpu.reg(Reg::AF) >> 8, 0x5a);

        cpu.mem_poke(0x1_0001, 0x77);
        cpu.set_reg(Reg::PC, 1);
        cpu.set_reg(Reg::HL, 0x3000);
        cpu.set_reg(Reg::AF, 0xa500);
        assert_ne!(cpu.step(), 0);
        assert_eq!(cpu.mem_peek(0x1_3000), 0xa5);
        assert_eq!(cpu.mem_peek(0x3000), 0x00);
    }

    #[test]
    fn mmu_boundary_cases_cover_empty_regions_and_one_mibibyte_wrap() {
        let mut cpu = mmu_machine();
        cpu.write_internal_io(CBR, 0x20);
        cpu.write_internal_io(BBR, 0x40);

        cpu.write_internal_io(CBAR, 0x88);
        assert_eq!(cpu.mmu_translate(0x7123), 0x07123, "below BA=CA");
        assert_eq!(cpu.mmu_translate(0x8123), 0x28123, "at BA=CA");

        cpu.write_internal_io(CBAR, 0x80);
        assert_eq!(cpu.mmu_translate(0x0123), 0x40123, "BA=0 bank base");
        assert_eq!(cpu.mmu_translate(0x7123), 0x47123, "BA=0 bank end");
        assert_eq!(cpu.mmu_translate(0x8123), 0x28123, "BA=0 common 1");

        cpu.write_internal_io(CBAR, 0xf4);
        assert_eq!(cpu.mmu_translate(0x3123), 0x03123, "below BA");
        assert_eq!(cpu.mmu_translate(0x4123), 0x44123, "at BA");
        assert_eq!(cpu.mmu_translate(0xe123), 0x4e123, "below CA=F");
        assert_eq!(cpu.mmu_translate(0xf123), 0x2f123, "at CA=F");

        cpu.write_internal_io(CBR, 0xff);
        cpu.write_internal_io(CBAR, 0x00);
        assert_eq!(cpu.mmu_translate(0x1123), 0x00123, "CBR wraps at 1 MiB");
        assert_eq!(cpu.mmu_translate(0xf123), 0x0e123, "CBR wrap keeps page");

        cpu.write_internal_io(BBR, 0xff);
        cpu.write_internal_io(CBAR, 0xf0);
        assert_eq!(cpu.mmu_translate(0x1123), 0x00123, "BBR wraps at 1 MiB");
        assert_eq!(cpu.mmu_translate(0xe123), 0x0d123, "BBR wrap keeps page");
    }

    proptest! {
        #[test]
        fn mmu_translation_array_matches_closed_form(
            cbr in any::<u8>(),
            bbr in any::<u8>(),
            cbar in any::<u8>(),
            logical in any::<u16>(),
        ) {
            let mut cpu = mmu_machine();
            cpu.write_internal_io(CBR, cbr);
            cpu.write_internal_io(BBR, bbr);
            cpu.write_internal_io(CBAR, cbar);

            let page = u32::from(logical >> 12);
            let ba = u32::from(cbar & 0x0f);
            let ca = u32::from(cbar >> 4);
            let relocation = if page < ba {
                0
            } else if page < ca {
                u32::from(bbr)
            } else {
                u32::from(cbr)
            };
            let expected = (((relocation + page) & 0xff) << 12)
                | u32::from(logical & 0x0fff);

            prop_assert_eq!(cpu.mmu_translate(logical), expected);
        }
    }

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
        assert_eq!(cpu.bus.io_writes, vec![(0x1234, 0x41)]);
        assert_eq!(cpu.io_reg_peek(BCR0L as u8), 0x01);
        assert_eq!(cpu.step(), 3, "held edge-sense DREQ does not retrigger");
        assert_eq!(cpu.bus.io_writes, vec![(0x1234, 0x41)]);
        cpu.set_dreq(0, false);
        cpu.set_dreq(0, true);
        assert_eq!(cpu.step(), 10);
        assert_eq!(cpu.bus.io_writes, vec![(0x1234, 0x41), (0x1234, 0x42)]);

        cpu.bus.io_read_value = 0x5a;
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
        assert_eq!(cpu.bus.io_reads, vec![0x5678]);
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
        assert_eq!(cpu.bus.io_writes, vec![(0x1234, 0x71), (0x1234, 0x72)]);
        assert_eq!(cpu.io_reg_peek(MAR1L as u8), 0x02);
        assert_eq!(cpu.io_reg_peek(BCR1L as u8), 0x00);

        cpu.bus.io_read_value = 0xa5;
        cpu.write_internal_io(MAR1L, 0x01);
        cpu.write_internal_io(MAR1H, 0x04);
        cpu.write_internal_io(IAR1L, 0x78);
        cpu.write_internal_io(IAR1H, 0x56);
        cpu.write_internal_io(BCR1L, 0x02);
        cpu.write_internal_io(DCNTL, 0x13);
        cpu.write_internal_io(DSTAT, 0x90);

        assert_eq!(cpu.step(), 19, "2 * (6 + 2 I/O waits) + 3-cycle NOP");
        assert_eq!(cpu.bus.io_reads, vec![0x5678, 0x5678]);
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
        assert_eq!(cpu.bus.io_writes, vec![(0x1111, 0xa0)]);
        assert_eq!(cpu.io_reg_peek(DSTAT as u8) & 0xc0, 0x80);
        assert_eq!(cpu.step(), 10);
        assert_eq!(cpu.bus.io_writes, vec![(0x1111, 0xa0), (0x2222, 0xb1)]);
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

    #[cfg(feature = "state")]
    #[test]
    fn save_state_version_and_decode_errors_are_atomic() {
        let mut cpu = machine();
        cpu.set_reg(Reg::AF, 0xa55a);
        cpu.mem_poke(0x1234, 0x66);
        let original = cpu.save_state();
        assert_eq!(original.first(), Some(&STATE_VERSION));
        assert_eq!(cpu.save_state(), original, "repeated saves are identical");

        assert_eq!(cpu.load_state(&[]), Err(StateError::MissingVersion));
        assert_eq!(
            cpu.load_state(&[STATE_VERSION.wrapping_add(1)]),
            Err(StateError::UnsupportedVersion(
                STATE_VERSION.wrapping_add(1)
            ))
        );
        assert_eq!(cpu.load_state(&[STATE_VERSION]), Err(StateError::Decode));
        assert_eq!(
            cpu.save_state(),
            original,
            "decode errors do not mutate state"
        );

        let mut decoded: SavedState = postcard::from_bytes(&original[1..])
            .expect("freshly saved payload must decode in its own test");
        decoded.io_regs.pop();
        let payload = postcard::to_allocvec(&decoded)
            .expect("the deliberately short register file must serialize");
        let mut wrong_register_count = vec![STATE_VERSION];
        wrong_register_count.extend_from_slice(&payload);
        assert_eq!(
            cpu.load_state(&wrong_register_count),
            Err(StateError::Decode)
        );
        assert_eq!(cpu.save_state(), original, "length errors are also atomic");
    }

    #[cfg(feature = "state")]
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(32))]

        #[test]
        fn save_state_round_trip_resume_matches_uninterrupted_execution(
            pre_steps in 0_u8..12,
            run_budget in 0_u32..4_000,
            dma_count in 1_u8..5,
            timer_count in 1_u8..8,
            tx_byte in any::<u8>(),
            rx_byte in any::<u8>(),
        ) {
            let mut original = machine();
            original.write_internal_io(DCNTL, 0x00);
            original.set_reg(Reg::AF, u16::from(tx_byte) << 8 | u16::from(rx_byte));
            original.set_reg(Reg::SP, 0x8000);

            original.write_internal_io(TMDR0L, timer_count);
            original.write_internal_io(TMDR0H, 0x00);
            original.write_internal_io(RLDR0L, timer_count.wrapping_add(1));
            original.write_internal_io(RLDR0H, 0x00);
            original.write_internal_io(TCR, 0x01);

            original.write_internal_io(CNTLB0, 0x00);
            original.write_internal_io(CNTLA0, 0x64);
            original.write_internal_io(TDR0, tx_byte);
            prop_assert!(original.asci_rx_push(0, rx_byte));

            original.write_internal_io(TRD, tx_byte ^ rx_byte);
            original.write_internal_io(CNTR, 0x10);

            for offset in 0..dma_count {
                original.mem_poke(0x0100 + u32::from(offset), tx_byte.wrapping_add(offset));
            }
            original.write_internal_io(SAR0L, 0x00);
            original.write_internal_io(SAR0H, 0x01);
            original.write_internal_io(DAR0L, 0x00);
            original.write_internal_io(DAR0H, 0x02);
            original.write_internal_io(BCR0L, dma_count);
            original.write_internal_io(DMODE, 0x00);
            original.write_internal_io(DSTAT, 0x60);

            for _ in 0..pre_steps {
                prop_assert_ne!(original.step(), 0);
            }

            let saved = original.save_state();
            let mut resumed = machine();
            prop_assert_eq!(resumed.load_state(&saved), Ok(()));
            prop_assert_eq!(resumed.save_state(), saved);

            prop_assert_eq!(original.run(run_budget), resumed.run(run_budget));
            prop_assert_eq!(original.asci_tx_pop(0), resumed.asci_tx_pop(0));
            prop_assert_eq!(original.csio_tx_pop(), resumed.csio_tx_pop());
            prop_assert_eq!(original.drain_events(), resumed.drain_events());
            prop_assert_eq!(original.save_state(), resumed.save_state());
        }
    }

    #[cfg(feature = "state")]
    #[test]
    fn determinism_timer_asci_dma_matches_after_ten_million_cycles() {
        const RUN_CYCLES: u32 = 10_000_000;

        let mut first = mmu_machine();
        let mut second = mmu_machine();
        for cpu in [&mut first, &mut second] {
            cpu.write_internal_io(DCNTL, 0x80);

            cpu.set_reg(Reg::PC, 0xffff);
            cpu.mem_poke(0xffff, 0xdd);
            cpu.mem_poke(0x0000, 0x76);

            cpu.write_internal_io(TMDR0L, 0x07);
            cpu.write_internal_io(TMDR0H, 0x00);
            cpu.write_internal_io(RLDR0L, 0x0b);
            cpu.write_internal_io(RLDR0H, 0x00);
            cpu.write_internal_io(TCR, 0x01);

            cpu.write_internal_io(CNTLB0, 0x00);
            cpu.write_internal_io(CNTLA0, 0x64);
            cpu.write_internal_io(TDR0, 0x5a);
            assert!(cpu.asci_rx_push(0, 0xa5));

            for offset in 0_u32..0x100 {
                cpu.mem_poke(0x1_0000 + offset, offset as u8 ^ 0xa5);
            }
            cpu.write_internal_io(SAR0L, 0x00);
            cpu.write_internal_io(SAR0H, 0x00);
            cpu.write_internal_io(SAR0B, 0x01);
            cpu.write_internal_io(DAR0L, 0x00);
            cpu.write_internal_io(DAR0H, 0x00);
            cpu.write_internal_io(DAR0B, 0x02);
            cpu.write_internal_io(BCR0L, 0x00);
            cpu.write_internal_io(BCR0H, 0x01);
            cpu.write_internal_io(DMODE, 0x00);
            cpu.write_internal_io(DSTAT, 0x60);
        }

        assert_eq!(first.run(RUN_CYCLES), RUN_CYCLES);
        assert_eq!(second.run(RUN_CYCLES), RUN_CYCLES);
        assert_eq!(first.cycle_count(), u64::from(RUN_CYCLES));
        assert_eq!(second.cycle_count(), u64::from(RUN_CYCLES));

        let first_state = first.save_state();
        let second_state = second.save_state();
        let first_events = first.drain_events();
        let second_events = second.drain_events();

        assert_eq!(
            first_events.len(),
            1,
            "the scripted TRAP makes the stream nonempty"
        );
        assert_eq!(first_state, second_state);
        assert_eq!(first_events, second_events);
    }

    #[test]
    fn event_memory_watch_fires_exactly_on_its_physical_half_open_range() {
        let mut cpu = machine();
        cpu.mem_poke(0x0100, 0x11);
        cpu.mem_poke(0x0101, 0x22);
        let watch = cpu.add_mem_watch(0x0100, 2, WatchKind::Both);

        assert_eq!(cpu.read_logical(0x00ff), 0x00);
        assert_eq!(cpu.read_logical(0x0100), 0x11);
        assert_eq!(cpu.read_logical(0x0101), 0x22);
        assert_eq!(cpu.read_logical(0x0102), 0x00);
        cpu.write_logical(0x00ff, 0x30);
        cpu.write_logical(0x0100, 0x31);
        cpu.write_logical(0x0101, 0x32);
        cpu.write_logical(0x0102, 0x33);

        assert_eq!(
            cpu.drain_events(),
            vec![
                Event::MemRead {
                    cycle: 0,
                    pc: 0,
                    phys: 0x0100,
                    val: 0x11,
                },
                Event::MemRead {
                    cycle: 0,
                    pc: 0,
                    phys: 0x0101,
                    val: 0x22,
                },
                Event::MemWrite {
                    cycle: 0,
                    pc: 0,
                    phys: 0x0100,
                    val: 0x31,
                },
                Event::MemWrite {
                    cycle: 0,
                    pc: 0,
                    phys: 0x0101,
                    val: 0x32,
                },
            ]
        );

        cpu.remove_mem_watch(watch);
        let _ = cpu.read_logical(0x0100);
        cpu.write_logical(0x0100, 0x44);
        assert!(cpu.drain_events().is_empty());

        let _ = cpu.add_mem_watch(0x0100, 0, WatchKind::Both);
        let _ = cpu.read_logical(0x0100);
        cpu.write_logical(0x0100, 0x55);
        assert!(cpu.drain_events().is_empty());
    }

    #[test]
    fn event_memory_watches_cover_dma_and_rom_write_attempts() {
        let mut dma = machine();
        dma.mem_poke(0x0100, 0xa5);
        let _ = dma.add_mem_watch(0x0100, 1, WatchKind::Read);
        let _ = dma.add_mem_watch(0x0200, 1, WatchKind::Write);
        dma.write_internal_io(SAR0L, 0x00);
        dma.write_internal_io(SAR0H, 0x01);
        dma.write_internal_io(DAR0L, 0x00);
        dma.write_internal_io(DAR0H, 0x02);
        dma.write_internal_io(BCR0L, 0x01);
        dma.write_internal_io(DMODE, 0x00);
        dma.write_internal_io(DCNTL, 0x00);
        dma.write_internal_io(DSTAT, 0x60);
        assert_ne!(dma.step(), 0);
        assert_eq!(
            dma.drain_events(),
            vec![
                Event::MemRead {
                    cycle: 0,
                    pc: 0,
                    phys: 0x0100,
                    val: 0xa5,
                },
                Event::MemWrite {
                    cycle: 0,
                    pc: 0,
                    phys: 0x0200,
                    val: 0xa5,
                },
            ]
        );

        let config = MachineConfig {
            regions: vec![RegionDef {
                base: 0,
                size: 0x1000,
                kind: RegionKind::Rom(vec![0; 0x1000]),
            }],
            ..MachineConfig::default()
        };
        let mut rom = Z180::new(config, NullBus).expect("ROM configuration must be valid");
        rom.mem_poke(0x0010, 0x11);
        assert!(rom.drain_events().is_empty(), "host pokes are not watched");
        let _ = rom.add_mem_watch(0x0010, 1, WatchKind::Write);
        rom.write_logical(0x0010, 0x22);
        assert_eq!(rom.mem_peek(0x0010), 0x00);
        assert_eq!(
            rom.drain_events(),
            vec![
                Event::MemWrite {
                    cycle: 0,
                    pc: 0,
                    phys: 0x0010,
                    val: 0x22,
                },
                Event::RomWrite {
                    cycle: 0,
                    pc: 0,
                    phys: 0x0010,
                    val: 0x22,
                },
            ]
        );
    }

    #[test]
    fn event_ring_retains_newest_entries_and_loss_is_sticky() {
        let config = MachineConfig {
            event_capacity: 2,
            regions: vec![RegionDef {
                base: 0,
                size: 0x1_0000,
                kind: RegionKind::Ram,
            }],
            ..MachineConfig::default()
        };
        let mut cpu = Z180::new(config, NullBus).expect("event-ring configuration must be valid");
        let _ = cpu.add_mem_watch(0, 4, WatchKind::Read);
        let _ = cpu.read_logical(0);
        let _ = cpu.read_logical(1);
        let _ = cpu.read_logical(2);

        assert!(cpu.events_lost());
        let storage_capacity = cpu.events.capacity();
        assert_eq!(
            cpu.drain_events(),
            vec![
                Event::MemRead {
                    cycle: 0,
                    pc: 0,
                    phys: 1,
                    val: 0,
                },
                Event::MemRead {
                    cycle: 0,
                    pc: 0,
                    phys: 2,
                    val: 0,
                },
            ]
        );
        assert_eq!(cpu.events.capacity(), storage_capacity);
        assert!(cpu.events_lost(), "draining does not clear the sticky flag");
        cpu.clear_events_lost();
        assert!(!cpu.events_lost());

        let _ = cpu.read_logical(3);
        cpu.reset();
        assert!(!cpu.events_lost());
        assert!(cpu.drain_events().is_empty());
        let _ = cpu.read_logical(0);
        assert_eq!(cpu.drain_events().len(), 1, "reset preserves watches");

        let disabled_config = MachineConfig {
            event_capacity: 0,
            regions: vec![RegionDef {
                base: 0,
                size: 0x1000,
                kind: RegionKind::Ram,
            }],
            ..MachineConfig::default()
        };
        let mut disabled =
            Z180::new(disabled_config, NullBus).expect("zero event capacity must be valid");
        let _ = disabled.add_mem_watch(0, 1, WatchKind::Read);
        let _ = disabled.read_logical(0);
        assert!(disabled.drain_events().is_empty());
        assert!(disabled.events_lost());
    }

    #[test]
    fn event_io_trace_records_cpu_dma_and_internal_duplicate_accesses_once() {
        let mut cpu = recording_machine(Variant::Z80180);
        cpu.instruction_pc = 0x1234;
        cpu.bus.io_read_value = 0x5a;
        cpu.set_io_trace(true);

        assert_eq!(cpu.read_io(0x0040), 0x5a);
        cpu.write_io(0x0041, 0xa5);
        assert_eq!(cpu.read_io(CNTLA0 as u16), 0x10);
        assert_eq!(cpu.dma_io_read(0x0042), 0x5a);
        cpu.dma_io_write(0x0043, 0xc3);

        assert_eq!(
            cpu.drain_events(),
            vec![
                Event::IoRead {
                    cycle: 0,
                    pc: 0x1234,
                    port: 0x0040,
                    val: 0x5a,
                },
                Event::IoWrite {
                    cycle: 0,
                    pc: 0x1234,
                    port: 0x0041,
                    val: 0xa5,
                },
                Event::IoRead {
                    cycle: 0,
                    pc: 0x1234,
                    port: CNTLA0 as u16,
                    val: 0x10,
                },
                Event::IoRead {
                    cycle: 0,
                    pc: 0x1234,
                    port: 0x0042,
                    val: 0x5a,
                },
                Event::IoWrite {
                    cycle: 0,
                    pc: 0x1234,
                    port: 0x0043,
                    val: 0xc3,
                },
            ]
        );
        assert_eq!(cpu.bus.io_reads, vec![0x0040, CNTLA0 as u16, 0x0042]);

        cpu.set_io_trace(false);
        let _ = cpu.read_io(0x0044);
        assert!(cpu.drain_events().is_empty());
    }

    #[test]
    fn event_irq_trace_and_pc_watch_use_acknowledge_and_instruction_boundaries() {
        let mut nmi = machine();
        nmi.set_irq_trace(true);
        nmi.set_nmi(true);
        assert_ne!(nmi.step(), 0);
        assert_eq!(
            nmi.drain_events(),
            vec![Event::IrqAck {
                cycle: 0,
                source: IrqSource::Nmi,
                vector: 0x0066,
            }]
        );

        let mut int0 = machine();
        int0.set_irq_trace(true);
        int0.set_interrupt_mode(1);
        int0.set_iff1(true);
        int0.set_irq(IrqLine::Int0, true);
        assert_ne!(int0.step(), 0);
        assert_eq!(
            int0.drain_events(),
            vec![Event::IrqAck {
                cycle: 0,
                source: IrqSource::Int0,
                vector: 0x0038,
            }]
        );

        let mut pc = machine();
        pc.mem_poke(0, 0x00);
        pc.mem_poke(1, 0x76);
        pc.set_pc_watch(Some(1));
        assert_ne!(pc.step(), 0);
        assert_eq!(pc.pc_watch_hits(), 0);
        assert_ne!(pc.step(), 0);
        assert_eq!(pc.pc_watch_hits(), 1);
        assert_ne!(pc.step(), 0);
        assert_eq!(pc.pc_watch_hits(), 1, "HALT idle is not instruction entry");

        pc.set_pc_watch(Some(2));
        assert_eq!(pc.pc_watch_hits(), 0, "setting a watch resets its count");
        assert_ne!(pc.step(), 0);
        assert_eq!(pc.pc_watch_hits(), 0);
        pc.set_pc_watch(Some(0));
        pc.reset();
        assert_eq!(pc.pc_watch_hits(), 0);
        assert_ne!(pc.step(), 0);
        assert_eq!(pc.pc_watch_hits(), 1, "reset preserves the watched address");
    }

    #[cfg(feature = "state")]
    #[test]
    fn event_debug_configuration_and_ring_round_trip_in_save_state() {
        let config = MachineConfig {
            event_capacity: 2,
            regions: vec![RegionDef {
                base: 0,
                size: 0x1_0000,
                kind: RegionKind::Ram,
            }],
            ..MachineConfig::default()
        };
        let mut original =
            Z180::new(config, NullBus).expect("debug-state configuration must be valid");
        let _ = original.add_mem_watch(0, 4, WatchKind::Read);
        original.set_io_trace(true);
        original.set_irq_trace(true);
        original.set_pc_watch(Some(0));
        assert_ne!(original.step(), 0);
        let _ = original.read_logical(1);
        let _ = original.read_logical(2);
        assert!(original.events_lost());
        assert_eq!(original.pc_watch_hits(), 1);

        let saved = original.save_state();
        assert_eq!(saved.first(), Some(&STATE_VERSION));
        let mut resumed = machine();
        assert_eq!(resumed.load_state(&saved), Ok(()));
        assert_eq!(resumed.save_state(), saved);
        assert!(resumed.events_lost());
        assert_eq!(resumed.pc_watch_hits(), 1);
        assert_eq!(resumed.drain_events(), original.drain_events());

        let _ = resumed.read_logical(3);
        assert_eq!(resumed.drain_events().len(), 1, "memory watch was restored");
    }

    #[test]
    fn insn_trace_records_fetched_bytes_physical_pc_and_traps_without_extra_reads() {
        let mut cpu = mmu_machine();
        cpu.write_internal_io(BBR, 0x10);
        cpu.write_internal_io(CBAR, 0xf4);
        cpu.set_reg(Reg::PC, 0x4000);
        cpu.set_reg(Reg::IX, 0x5000);
        for (offset, byte) in [0x3e, 0x42, 0xdd, 0xcb, 0x01, 0x46, 0xed, 0x31]
            .into_iter()
            .enumerate()
        {
            cpu.mem_poke(0x1_4000 + offset as u32, byte);
        }
        let _ = cpu.add_mem_watch(0x1_4000, 8, WatchKind::Read);
        let _ = cpu.add_mem_watch(0x1_5001, 1, WatchKind::Read);
        cpu.set_insn_trace(Some(3));

        let first_cycles = cpu.step();
        let second_cycles = cpu.step();
        let _ = cpu.step();

        assert_eq!(
            cpu.drain_insn_trace(),
            vec![
                TraceEntry {
                    cycle: 0,
                    pc: 0x4000,
                    phys_pc: 0x1_4000,
                    bytes: [0x3e, 0x42, 0, 0],
                    len: 2,
                },
                TraceEntry {
                    cycle: u64::from(first_cycles),
                    pc: 0x4002,
                    phys_pc: 0x1_4002,
                    bytes: [0xdd, 0xcb, 0x01, 0x46],
                    len: 4,
                },
                TraceEntry {
                    cycle: u64::from(first_cycles + second_cycles),
                    pc: 0x4006,
                    phys_pc: 0x1_4006,
                    bytes: [0xed, 0x31, 0, 0],
                    len: 2,
                },
            ]
        );
        let watched_reads: Vec<u32> = cpu
            .drain_events()
            .into_iter()
            .filter_map(|event| match event {
                Event::MemRead { phys, .. } => Some(phys),
                _ => None,
            })
            .collect();
        assert_eq!(
            watched_reads,
            vec![
                0x1_4000, 0x1_4001, 0x1_4002, 0x1_4003, 0x1_4005, 0x1_4004, 0x1_5001, 0x1_4006,
                0x1_4007,
            ],
            "tracing observes existing fetches without adding memory reads"
        );
    }

    #[test]
    fn insn_trace_ring_resizes_drains_resets_and_disables_exactly() {
        let mut cpu = machine();
        cpu.mem_poke(0, 0x00);
        cpu.mem_poke(1, 0x00);
        cpu.mem_poke(2, 0x00);
        cpu.mem_poke(3, 0x76);
        cpu.set_insn_trace(Some(3));
        assert_ne!(cpu.step(), 0);
        assert_ne!(cpu.step(), 0);
        assert_ne!(cpu.step(), 0);
        assert_ne!(cpu.step(), 0);

        cpu.set_insn_trace(Some(2));
        assert_eq!(
            cpu.drain_insn_trace()
                .into_iter()
                .map(|entry| entry.pc)
                .collect::<Vec<_>>(),
            vec![2, 3]
        );
        let storage_capacity = cpu.insn_trace.capacity();
        assert_ne!(cpu.step(), 0);
        assert!(cpu.drain_insn_trace().is_empty(), "HALT idle is not traced");
        assert_eq!(cpu.insn_trace.capacity(), storage_capacity);

        cpu.reset();
        assert!(cpu.drain_insn_trace().is_empty());
        assert_ne!(cpu.step(), 0);
        assert_eq!(cpu.drain_insn_trace()[0].pc, 0);

        cpu.set_insn_trace(Some(0));
        assert_ne!(cpu.step(), 0);
        assert!(cpu.drain_insn_trace().is_empty());
        cpu.set_insn_trace(None);
        assert_eq!(cpu.insn_trace_capacity, None);
        assert_eq!(cpu.insn_trace.capacity(), 0);
    }

    #[cfg(feature = "state")]
    #[test]
    fn insn_trace_configuration_and_ring_round_trip_in_save_state() {
        let mut original = machine();
        original.mem_poke(0, 0x00);
        original.mem_poke(1, 0x00);
        original.mem_poke(2, 0x00);
        original.set_insn_trace(Some(2));
        assert_ne!(original.step(), 0);
        assert_ne!(original.step(), 0);

        let saved = original.save_state();
        assert_eq!(saved.first(), Some(&STATE_VERSION));
        let mut resumed = machine();
        assert_eq!(resumed.load_state(&saved), Ok(()));
        assert_eq!(resumed.save_state(), saved);
        assert_eq!(resumed.drain_insn_trace(), original.drain_insn_trace());

        assert_ne!(resumed.step(), 0);
        assert_ne!(original.step(), 0);
        assert_eq!(resumed.drain_insn_trace(), original.drain_insn_trace());
    }

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

    #[test]
    fn asci_standard_divisors_and_frame_formats_use_hand_computed_cycle_counts() {
        let mut cpu = machine();

        cpu.write_internal_io(CNTLA0, 0x44); // RE, 8 data, no parity, 1 stop.
        cpu.write_internal_io(CNTLB0, 0x00); // /10, /16, SS=0: 160 phi/bit.
        assert!(cpu.asci_rx_push(0, 0xa5));
        cpu.finish_step(1_599); // (1 start + 8 data + 1 stop) * 160 - 1.
        assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0x80, 0);
        cpu.finish_step(1);
        assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0x80, 0x80);
        assert_eq!(cpu.read_internal_io(RDR0), 0xa5);

        cpu.write_internal_io(CNTLA0, 0x47); // RE, 8 data, parity, 2 stop.
        cpu.write_internal_io(CNTLB0, 0x22); // /30, /16, SS=2: 1920 phi/bit.
        assert!(cpu.asci_rx_push(0, 0x5a));
        cpu.finish_step(23_039); // (1 + 8 + 1 + 2) * 1920 - 1.
        assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0x80, 0);
        cpu.finish_step(1);
        assert_eq!(cpu.read_internal_io(RDR0), 0x5a);

        cpu.write_internal_io(CNTLA0, 0x40); // RE, 7 data, no parity, 1 stop.
        cpu.write_internal_io(CNTLB0, 0x09); // /10, /64, SS=1: 1280 phi/bit.
        assert!(cpu.asci_rx_push(0, 0x33));
        cpu.finish_step(11_519); // (1 + 7 + 1) * 1280 - 1.
        assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0x80, 0);
        cpu.finish_step(1);
        assert_eq!(cpu.read_internal_io(RDR0), 0x33);
    }

    #[test]
    fn asci_tdr_tsr_double_buffering_and_tdre_follow_transmit_progress() {
        let mut cpu = machine();
        cpu.write_internal_io(CNTLB0, 0x00);
        cpu.write_internal_io(CNTLA0, 0x24); // TE, 8N1.

        cpu.write_internal_io(TDR0, 0x11);
        assert_eq!(
            cpu.io_reg_peek(STAT0 as u8) & 0x02,
            0x02,
            "TDR moved to TSR"
        );
        cpu.write_internal_io(TDR0, 0x22);
        assert_eq!(
            cpu.io_reg_peek(STAT0 as u8) & 0x02,
            0x00,
            "second byte fills TDR"
        );

        cpu.finish_step(1_599);
        assert_eq!(cpu.asci_tx_pop(0), None);
        cpu.finish_step(1);
        assert_eq!(cpu.asci_tx_pop(0), Some(0x11));
        assert_eq!(
            cpu.io_reg_peek(STAT0 as u8) & 0x02,
            0x02,
            "second byte moved to TSR"
        );

        cpu.finish_step(1_600);
        assert_eq!(cpu.asci_tx_pop(0), Some(0x22));
        assert_eq!(cpu.asci_tx_pop(0), None);
    }

    #[test]
    fn asci_rdr_rsr_double_buffering_sets_overrun_without_replacing_rdr() {
        let mut cpu = machine();
        cpu.write_internal_io(CNTLB0, 0x00);
        cpu.write_internal_io(CNTLA0, 0x44);

        assert!(cpu.asci_rx_push(0, 0x11));
        cpu.finish_step(1_600);
        assert!(
            cpu.asci_rx_push(0, 0x22),
            "RSR remains available while RDR is full"
        );
        cpu.finish_step(1_600);

        assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0xc0, 0xc0);
        assert_eq!(
            cpu.read_internal_io(RDR0),
            0x11,
            "overrun preserves the prior RDR byte"
        );
        assert_eq!(
            cpu.io_reg_peek(STAT0 as u8) & 0xc0,
            0x40,
            "RDR read leaves OVRN set"
        );
        cpu.write_internal_io(CNTLA0, 0x44); // EFR=0 clears OVRN/PE/FE.
        assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0x70, 0);
    }

    #[test]
    fn s180_asci_fifo_and_astc_brg_have_the_documented_depth_and_timing() {
        let mut cpu = recording_machine(Variant::Z8S180);
        cpu.write_internal_io(CNTLB0, 0x00);
        cpu.write_internal_io(CNTLA0, 0x44);

        for byte in 0_u8..4 {
            assert!(cpu.asci_rx_push(0, byte));
            cpu.finish_step(1_600);
        }
        assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0xc0, 0x80);
        assert!(cpu.asci_rx_push(0, 4));
        cpu.finish_step(1_600);
        assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0xc0, 0xc0);
        for byte in 0_u8..4 {
            assert_eq!(cpu.read_internal_io(RDR0), byte);
        }
        assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0x80, 0);

        cpu.write_internal_io(0x12, 0x18); // X1 bit clock + 16-bit BRG.
        cpu.write_internal_io(ASTC0L, 0x03);
        cpu.write_internal_io(ASTC0H, 0x00);
        assert!(cpu.asci_rx_push(0, 0x55));
        cpu.finish_step(99); // 8N1 * [2 * (3 + 2) * 1] - 1.
        assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0x80, 0);
        cpu.finish_step(1);
        assert_eq!(cpu.read_internal_io(RDR0), 0x55);
    }

    #[test]
    fn asci_rie_tie_and_s180_rdrf_inhibit_qualify_internal_requests() {
        let mut cpu = machine();
        cpu.write_internal_io(CNTLB0, 0x00);
        cpu.write_internal_io(CNTLA0, 0x44);
        cpu.write_internal_io(STAT0, 0x08);
        assert!(cpu.asci_rx_push(0, 0xa5));
        cpu.finish_step(1_599);
        assert_eq!(cpu.internal_irq_pending & 0x20, 0);
        cpu.finish_step(1);
        assert_eq!(cpu.internal_irq_pending & 0x20, 0x20);
        assert_eq!(cpu.read_internal_io(RDR0), 0xa5);
        assert_eq!(cpu.internal_irq_pending & 0x20, 0);

        cpu.write_internal_io(STAT0, 0x01);
        assert_eq!(
            cpu.internal_irq_pending & 0x20,
            0x20,
            "TIE with TDRE requests"
        );
        cpu.write_internal_io(TDR0, 0x5a);
        assert_eq!(
            cpu.internal_irq_pending & 0x20,
            0,
            "TE is off, so TDR remains full"
        );

        let mut s180 = recording_machine(Variant::Z8S180);
        s180.write_internal_io(CNTLB0, 0x00);
        s180.write_internal_io(CNTLA0, 0x44);
        s180.write_internal_io(STAT0, 0x08);
        assert!(s180.asci_rx_push(0, 0x33));
        s180.finish_step(1_600);
        assert_eq!(
            s180.internal_irq_pending & 0x20,
            0,
            "reset ASEXT inhibits RDRF IRQ"
        );
        s180.write_internal_io(0x12, 0x80);
        assert_eq!(
            s180.internal_irq_pending & 0x20,
            0x20,
            "ASEXT bit 7 removes inhibit"
        );
    }

    #[test]
    fn both_asci_channels_deliver_their_internal_vectors_from_halt() {
        for (channel, pending, vector) in [(0_usize, 0x20_u8, 0x20ae_u32), (1, 0x40, 0x20b0)] {
            let mut cpu = machine();
            cpu.write_internal_io(DCNTL, 0x00);
            cpu.write_internal_io(IL, 0xa0);
            cpu.mem_poke(0, 0x76);
            cpu.mem_poke(vector, 0x56);
            cpu.mem_poke(vector + 1, 0x34);
            cpu.set_reg(Reg::IR, 0x2000);
            cpu.set_reg(Reg::SP, 0x8000);
            cpu.set_iff1(true);

            assert_eq!(cpu.step(), 3, "channel {channel} executes HALT");
            assert!(cpu.halted());
            cpu.write_internal_io(STAT0 + channel, 0x01);
            assert_eq!(cpu.internal_irq_pending & pending, pending);
            assert_eq!(cpu.step(), 18, "channel {channel} acknowledges ASCI IRQ");
            assert_eq!(cpu.reg(Reg::PC), 0x3456);
            assert_eq!(cpu.reg(Reg::SP), 0x7ffe);
        }
    }

    #[test]
    fn asci_cts_suppresses_only_the_documented_tdre_surfaces() {
        let mut cpu = machine();
        assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0x02, 0x02);
        cpu.write_internal_io(CNTLB0, 0x00);
        cpu.write_internal_io(CNTLA0, 0x24);
        cpu.write_internal_io(TDR0, 0xa5);
        cpu.set_asci_cts(0, true);
        assert_eq!(cpu.io_reg_peek(CNTLB0 as u8) & 0x20, 0x20);
        assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0x02, 0);
        cpu.finish_step(1_600);
        assert_eq!(cpu.asci_tx_pop(0), Some(0xa5), "CTS does not stop TSR");
        assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0x02, 0);
        cpu.set_asci_cts(0, false);
        assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0x02, 0x02);

        cpu.set_asci_cts(1, true);
        assert_eq!(cpu.io_reg_peek(STAT1 as u8) & 0x02, 0x02, "CTS1E is clear");
        cpu.write_internal_io(STAT1, 0x04);
        assert_eq!(cpu.io_reg_peek(CNTLB0 as u8 + 1) & 0x20, 0x20);
        assert_eq!(
            cpu.io_reg_peek(STAT1 as u8) & 0x02,
            0,
            "CTS1E enables gating"
        );

        let mut s180 = recording_machine(Variant::Z8S180);
        s180.set_asci_cts(0, true);
        assert_eq!(s180.io_reg_peek(STAT0 as u8) & 0x02, 0);
        s180.write_internal_io(0x12, 0x20);
        assert_eq!(
            s180.io_reg_peek(STAT0 as u8) & 0x02,
            0x02,
            "ASEXT CTS0 disable makes the pin advisory"
        );
    }

    #[test]
    fn asci_dcd_transition_latch_gates_receive_and_requests_until_stat0_read() {
        let mut cpu = machine();
        cpu.write_internal_io(CNTLB0, 0x00);
        cpu.write_internal_io(CNTLA0, 0x44);
        cpu.write_internal_io(STAT0, 0x08);
        assert!(cpu.asci_rx_push(0, 0x11));

        cpu.set_asci_dcd(0, true);
        assert_eq!(cpu.internal_irq_pending & 0x20, 0x20);
        assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0xf4, 0x04);
        assert!(!cpu.asci_rx_push(0, 0x22));
        cpu.set_asci_dcd(0, false);
        assert!(!cpu.asci_rx_push(0, 0x33), "latched DCD remains inhibiting");

        assert_eq!(
            cpu.read_internal_io(STAT0) & 0x04,
            0x04,
            "first read reports prior high"
        );
        assert_eq!(cpu.internal_irq_pending & 0x20, 0);
        assert_eq!(
            cpu.read_internal_io(STAT0) & 0x04,
            0,
            "second read reports low"
        );
        assert!(cpu.asci_rx_push(0, 0x44));

        let mut s180 = recording_machine(Variant::Z8S180);
        s180.write_internal_io(CNTLB0, 0x00);
        s180.write_internal_io(CNTLA0, 0x44);
        s180.write_internal_io(0x12, 0x40);
        s180.set_asci_dcd(0, true);
        assert!(
            s180.asci_rx_push(0, 0x55),
            "ASEXT DCD disable makes the pin advisory"
        );
        s180.finish_step(1_600);
        assert_eq!(s180.read_internal_io(RDR0), 0x55);
    }

    #[test]
    fn asci_reset_and_iostop_preserve_data_registers_while_stopping_operations() {
        let mut cpu = machine();
        cpu.write_internal_io(TDR0, 0xa5);
        cpu.write_internal_io(RDR0, 0x5a);
        cpu.reset();
        assert_eq!(cpu.io_reg_peek(TDR0 as u8), 0xa5);
        assert_eq!(cpu.io_reg_peek(RDR0 as u8), 0x5a);
        assert_eq!(cpu.io_reg_peek(STAT0 as u8), 0x02);

        cpu.write_internal_io(CNTLB0, 0x00);
        cpu.write_internal_io(CNTLA0, 0x64);
        cpu.write_internal_io(TDR0, 0x11);
        assert!(cpu.asci_rx_push(0, 0x22));
        cpu.io_regs[STAT0] |= 0x70;
        cpu.write_internal_io(ICR, 0x20);
        assert_eq!(cpu.io_reg_peek(CNTLA0 as u8) & 0x60, 0);
        assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0xf2, 0x02);
        cpu.write_internal_io(CNTLA0, 0x64);
        cpu.write_internal_io(TDR0, 0x33);
        assert_eq!(cpu.io_reg_peek(CNTLA0 as u8) & 0x60, 0);
        assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0x02, 0x02);
        assert!(!cpu.asci_rx_push(0, 0x44));
        cpu.finish_step(3_200);
        assert_eq!(cpu.asci_tx_pop(0), None);
        assert_eq!(cpu.io_reg_peek(STAT0 as u8) & 0x80, 0);
        assert_eq!(cpu.io_reg_peek(TDR0 as u8), 0x33);
        assert_eq!(cpu.io_reg_peek(RDR0 as u8), 0x5a);
    }

    #[test]
    fn csio_internal_speed_selects_match_table_22_byte_timings() {
        let mut cpu = machine();
        for speed in 0_u8..=6 {
            let expected = 160_u32 << speed;
            cpu.write_internal_io(TRD, 0x80 | speed);
            cpu.write_internal_io(CNTR, 0x10 | speed);

            cpu.finish_step(expected - 1);
            assert_eq!(cpu.io_reg_peek(CNTR as u8) & 0x90, 0x10, "SS={speed}");
            assert_eq!(cpu.csio_tx_pop(), None, "SS={speed} before completion");
            cpu.finish_step(1);
            assert_eq!(cpu.io_reg_peek(CNTR as u8) & 0x90, 0x80, "SS={speed}");
            assert_eq!(cpu.csio_tx_pop(), Some(0x80 | speed), "SS={speed} byte");
        }
    }

    #[test]
    fn csio_receive_is_half_duplex_unbuffered_and_clears_ef_on_trd_access() {
        let mut cpu = machine();
        cpu.write_internal_io(CNTR, 0x20);
        assert!(cpu.csio_rx_push(0xa5));
        assert!(!cpu.csio_rx_push(0x5a), "one RXS shift operation at a time");
        cpu.finish_step(159);
        assert_eq!(cpu.io_reg_peek(CNTR as u8) & 0xa0, 0x20);
        cpu.finish_step(1);
        assert_eq!(cpu.io_reg_peek(CNTR as u8) & 0xa0, 0x80);
        assert_eq!(cpu.read_internal_io(TRD), 0xa5);
        assert_eq!(cpu.io_reg_peek(CNTR as u8) & 0x80, 0);

        cpu.write_internal_io(CNTR, 0x30);
        assert_eq!(
            cpu.io_reg_peek(CNTR as u8) & 0x30,
            0,
            "invalid duplex request stops"
        );

        cpu.write_internal_io(STAT1, 0x04);
        cpu.write_internal_io(CNTR, 0x20);
        assert_eq!(
            cpu.io_reg_peek(CNTR as u8) & 0x20,
            0,
            "CTS1E owns the RXS pin"
        );
        assert!(!cpu.csio_rx_push(0x33));
    }

    #[test]
    fn csio_is_unbuffered_and_software_clears_abort_active_operations() {
        let mut cpu = machine();
        cpu.write_internal_io(TRD, 0x11);
        cpu.write_internal_io(CNTR, 0x10);
        cpu.finish_step(80);
        cpu.write_internal_io(TRD, 0x22);
        cpu.finish_step(80);
        assert_eq!(
            cpu.csio_tx_pop(),
            Some(0x22),
            "TRD write updates active shift data"
        );

        cpu.write_internal_io(TRD, 0x33);
        cpu.write_internal_io(CNTR, 0x10);
        cpu.finish_step(80);
        cpu.write_internal_io(CNTR, 0x00);
        cpu.finish_step(160);
        assert_eq!(cpu.csio_tx_pop(), None);
        assert_eq!(cpu.io_reg_peek(CNTR as u8) & 0x90, 0);

        cpu.write_internal_io(CNTR, 0x20);
        assert!(cpu.csio_rx_push(0x44));
        cpu.finish_step(80);
        cpu.write_internal_io(CNTR, 0x00);
        cpu.finish_step(160);
        assert_eq!(cpu.io_reg_peek(CNTR as u8) & 0xa0, 0);
        assert_eq!(cpu.io_reg_peek(TRD as u8), 0x33);
    }

    #[test]
    fn csio_ef_eie_interrupt_protocol_and_vector_delivery_match_um0050() {
        let mut cpu = machine();
        cpu.write_internal_io(DCNTL, 0x00);
        cpu.write_internal_io(IL, 0xa0);
        cpu.mem_poke(0, 0x76);
        cpu.mem_poke(0x20ac, 0x56);
        cpu.mem_poke(0x20ad, 0x34);
        cpu.set_reg(Reg::IR, 0x2000);
        cpu.set_reg(Reg::SP, 0x8000);
        cpu.set_iff1(true);

        assert_eq!(cpu.step(), 3);
        assert!(cpu.halted());
        cpu.write_internal_io(TRD, 0xa5);
        cpu.write_internal_io(CNTR, 0x50);
        cpu.finish_step(160);
        assert_eq!(cpu.io_reg_peek(CNTR as u8) & 0xd0, 0xc0);
        assert_eq!(cpu.internal_irq_pending & 0x10, 0x10);

        assert_eq!(cpu.step(), 18);
        assert_eq!(cpu.reg(Reg::PC), 0x3456);
        assert_eq!(cpu.reg(Reg::SP), 0x7ffe);
        assert_eq!(cpu.read_internal_io(TRD), 0xa5);
        assert_eq!(cpu.internal_irq_pending & 0x10, 0);
    }

    #[test]
    fn csio_external_clock_waits_and_reset_iostop_preserve_trd() {
        let mut cpu = machine();
        cpu.write_internal_io(TRD, 0xa5);
        cpu.write_internal_io(CNTR, 0x57);
        cpu.finish_step(1_000_000);
        assert_eq!(cpu.csio_tx_pop(), None);
        assert_eq!(cpu.io_reg_peek(CNTR as u8) & 0x90, 0x10);

        cpu.write_internal_io(ICR, 0x20);
        assert_eq!(cpu.io_reg_peek(CNTR as u8), 0x47, "IOSTOP preserves EIE");
        cpu.write_internal_io(CNTR, 0x50);
        cpu.write_internal_io(TRD, 0x5a);
        assert_eq!(cpu.io_reg_peek(CNTR as u8) & 0xb0, 0);
        cpu.finish_step(160);
        assert_eq!(cpu.csio_tx_pop(), None);

        cpu.reset();
        assert_eq!(cpu.io_reg_peek(CNTR as u8), 0x07);
        assert_eq!(cpu.io_reg_peek(TRD as u8), 0x5a);
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
        assert_eq!(cpu.bus.io_writes, vec![(0x003f, 0x40)]);

        cpu.mem_poke(3, 0xed);
        cpu.mem_poke(4, 0x01);
        cpu.mem_poke(5, 0x72);
        cpu.set_reg(Reg::BC, 0x5a00);
        cpu.step();
        assert_eq!(cpu.io_reg_peek(DCNTL as u8), 0x5a);
        assert_eq!(cpu.bus.io_writes[1], (0x0072, 0x5a));

        cpu.mem_poke(6, 0xed);
        cpu.mem_poke(7, 0x01);
        cpu.mem_poke(8, 0x32);
        cpu.set_reg(Reg::BC, 0xa500);
        cpu.step();
        assert_eq!(cpu.io_reg_peek(DCNTL as u8), 0x5a, "old window is external");
        assert_eq!(cpu.bus.io_writes[2], (0x0032, 0xa5));

        cpu.mem_poke(9, 0xed);
        cpu.mem_poke(10, 0x41);
        cpu.set_reg(Reg::BC, 0xa572);
        cpu.step();
        assert_eq!(
            cpu.io_reg_peek(DCNTL as u8),
            0x5a,
            "nonzero high byte is external"
        );
        assert_eq!(cpu.bus.io_writes[3], (0xa572, 0xa5));
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
            cpu.bus.io_writes,
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
        input.bus.io_read_value = 0x55;
        input.io_regs[DCNTL] = 0xa5;
        input.mem_poke(0, 0xed);
        input.mem_poke(1, 0x00);
        input.mem_poke(2, 0x32);
        input.step();
        assert_eq!(input.reg(Reg::BC).to_be_bytes()[0], 0xa5);
        assert_eq!(input.bus.io_reads, vec![0x0032]);

        let mut tstio = recording_machine(Variant::Z80180);
        tstio.bus.io_read_value = 0xff;
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
        assert_eq!(tstio.bus.io_reads, vec![0x0032]);

        let mut otim = recording_machine(Variant::Z80180);
        otim.mem_poke(0, 0xed);
        otim.mem_poke(1, 0x83);
        otim.mem_poke(0x2000, 0x60);
        otim.set_reg(Reg::BC, 0x0132);
        otim.set_reg(Reg::HL, 0x2000);
        assert_eq!(otim.step(), 23, "internal OTIM has no external-I/O waits");
        assert_eq!(otim.io_reg_peek(DCNTL as u8), 0x60);
        assert_eq!(otim.bus.io_writes, vec![(0x0032, 0x60)]);
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

    #[test]
    fn implemented_set_is_every_documented_unprefixed_opcode() {
        for opcode in 0_u8..=u8::MAX {
            let expected = !matches!(opcode, 0xcb | 0xdd | 0xed | 0xfd);
            assert_eq!(
                Z180::<NullBus>::is_instruction_implemented(&[opcode]),
                expected,
                "opcode {opcode:02x}"
            );
        }

        for opcode in 0_u8..=u8::MAX {
            let expected = !(0x30..=0x37).contains(&opcode);
            assert_eq!(
                Z180::<NullBus>::is_instruction_implemented(&[0xcb, opcode]),
                expected,
                "CB {opcode:02x}"
            );
        }

        for prefix in [0xdd, 0xfd] {
            for opcode in 0_u8..=u8::MAX {
                let expected = matches!(
                    opcode,
                    0x09 | 0x19
                        | 0x21
                        | 0x22
                        | 0x23
                        | 0x29
                        | 0x2a
                        | 0x2b
                        | 0x34
                        | 0x35
                        | 0x36
                        | 0x39
                        | 0x46
                        | 0x4e
                        | 0x56
                        | 0x5e
                        | 0x66
                        | 0x6e
                        | 0x70
                        ..=0x75
                            | 0x77
                            | 0x7e
                            | 0x86
                            | 0x8e
                            | 0x96
                            | 0x9e
                            | 0xa6
                            | 0xae
                            | 0xb6
                            | 0xbe
                            | 0xe1
                            | 0xe3
                            | 0xe5
                            | 0xe9
                            | 0xf9
                );
                assert_eq!(
                    Z180::<NullBus>::is_instruction_implemented(&[prefix, opcode]),
                    expected,
                    "{prefix:02x} {opcode:02x}"
                );
            }
        }

        for prefix in [0xdd, 0xfd] {
            for opcode in 0_u8..=u8::MAX {
                let expected = opcode & 0x07 == 6 && !(0x30..=0x37).contains(&opcode);
                assert_eq!(
                    Z180::<NullBus>::is_instruction_implemented(&[prefix, 0xcb, opcode]),
                    expected,
                    "{prefix:02x} cb __ {opcode:02x}"
                );
            }
        }
    }

    #[test]
    fn undefined_second_opcode_takes_trap() {
        for opcodes in [[0xcb, 0x30], [0xdd, 0x24], [0xed, 0x31], [0xfd, 0x24]] {
            let mut cpu = machine();
            cpu.mem_poke(0x1234, opcodes[0]);
            cpu.mem_poke(0x1235, opcodes[1]);
            cpu.set_reg(Reg::PC, 0x1234);
            cpu.set_reg(Reg::SP, 0x8000);
            cpu.set_reg(Reg::IR, 0x56fe);
            cpu.set_iff1(true);
            cpu.set_iff2(false);

            assert_eq!(cpu.step(), 29, "{opcodes:02x?}");
            assert_eq!(cpu.instruction_pc(), 0x1234, "{opcodes:02x?}");
            assert_eq!(cpu.reg(Reg::PC), 0, "{opcodes:02x?}");
            assert_eq!(cpu.reg(Reg::SP), 0x7ffe, "{opcodes:02x?}");
            assert_eq!(cpu.mem_peek(0x7ffe), 0x36, "{opcodes:02x?}");
            assert_eq!(cpu.mem_peek(0x7fff), 0x12, "{opcodes:02x?}");
            assert_eq!(cpu.reg(Reg::IR), 0x5680, "{opcodes:02x?}");
            assert_eq!(cpu.itc(), 0x81, "{opcodes:02x?}");
            assert!(cpu.iff1(), "{opcodes:02x?}");
            assert!(!cpu.iff2(), "{opcodes:02x?}");
            assert_eq!(cpu.cycle_count(), 29, "{opcodes:02x?}");
            assert_eq!(
                cpu.drain_events(),
                vec![Event::Trap {
                    cycle: 0,
                    pc: 0x1234,
                    opcode: [opcodes[0], opcodes[1], 0],
                    len: 2,
                }],
                "{opcodes:02x?}"
            );
            assert!(cpu.drain_events().is_empty(), "{opcodes:02x?}");
        }
    }

    #[test]
    fn undefined_third_opcode_takes_trap_with_ufo() {
        for prefix in [0xdd, 0xfd] {
            let mut cpu = machine();
            cpu.mem_poke(0x1234, prefix);
            cpu.mem_poke(0x1235, 0xcb);
            cpu.mem_poke(0x1236, 0x05);
            cpu.mem_poke(0x1237, 0x40);
            cpu.set_reg(Reg::PC, 0x1234);
            cpu.set_reg(Reg::SP, 0x8000);
            cpu.set_reg(Reg::IR, 0x567e);
            cpu.set_iff1(false);
            cpu.set_iff2(true);

            assert_eq!(cpu.step(), 44, "{prefix:02x}");
            assert_eq!(cpu.instruction_pc(), 0x1234, "{prefix:02x}");
            assert_eq!(cpu.reg(Reg::PC), 0, "{prefix:02x}");
            assert_eq!(cpu.reg(Reg::SP), 0x7ffe, "{prefix:02x}");
            assert_eq!(cpu.mem_peek(0x7ffe), 0x38, "{prefix:02x}");
            assert_eq!(cpu.mem_peek(0x7fff), 0x12, "{prefix:02x}");
            assert_eq!(cpu.reg(Reg::IR), 0x5601, "{prefix:02x}");
            assert_eq!(cpu.itc(), 0xc1, "{prefix:02x}");
            assert!(!cpu.iff1(), "{prefix:02x}");
            assert!(cpu.iff2(), "{prefix:02x}");
            assert_eq!(cpu.cycle_count(), 44, "{prefix:02x}");
            assert_eq!(
                cpu.drain_events(),
                vec![Event::Trap {
                    cycle: 0,
                    pc: 0x1234,
                    opcode: [prefix, 0xcb, 0x40],
                    len: 3,
                }],
                "{prefix:02x}"
            );
        }
    }

    #[test]
    fn daa_matches_every_accumulator_and_control_flag_combination() {
        let mut cpu = machine();
        cpu.mem_poke(0, 0x27);

        for accumulator in 0_u8..=u8::MAX {
            for controls in 0_u8..8 {
                let initial_flags = (u8::from(controls & 1 != 0) * FLAG_C)
                    | (u8::from(controls & 2 != 0) * FLAG_H)
                    | (u8::from(controls & 4 != 0) * FLAG_N);
                cpu.reset();
                cpu.set_reg(Reg::AF, u16::from_be_bytes([accumulator, initial_flags]));

                assert_eq!(cpu.step(), 7);

                let [actual_accumulator, actual_flags] = cpu.reg(Reg::AF).to_be_bytes();
                let (expected_accumulator, expected_flags) =
                    expected_daa(accumulator, initial_flags);
                assert_eq!(
                    (actual_accumulator, actual_flags),
                    (expected_accumulator, expected_flags),
                    "A={accumulator:02x} F={initial_flags:02x}"
                );
            }
        }
    }

    #[test]
    fn ei_shadow_lasts_through_exactly_the_following_instruction() {
        let mut cpu = machine();
        cpu.mem_poke(0, 0xfb);
        cpu.mem_poke(1, 0x00);
        cpu.mem_poke(2, 0xfb);
        cpu.mem_poke(3, 0xfb);
        cpu.mem_poke(4, 0xf3);

        assert_eq!(cpu.step(), 6);
        assert!(cpu.iff1());
        assert!(cpu.iff2());
        assert!(cpu.ei_shadow);

        assert_eq!(cpu.step(), 6);
        assert!(!cpu.ei_shadow);

        cpu.step();
        assert!(cpu.ei_shadow);
        cpu.step();
        assert!(cpu.ei_shadow);

        cpu.step();
        assert!(!cpu.iff1());
        assert!(!cpu.iff2());
        assert!(!cpu.ei_shadow);
    }

    #[test]
    fn interrupts_ei_shadow_defers_maskable_service_for_one_instruction() {
        let mut cpu = machine();
        cpu.write_internal_io(DCNTL, 0x00);
        cpu.mem_poke(0, 0xfb);
        cpu.mem_poke(1, 0x00);
        cpu.mem_poke(2, 0x00);
        cpu.set_reg(Reg::SP, 0x8000);
        cpu.set_irq(IrqLine::Int0, true);

        assert_eq!(cpu.step(), 3);
        assert!(cpu.ei_shadow);
        assert_eq!(cpu.step(), 3);
        assert_eq!(cpu.reg(Reg::PC), 2);
        assert!(!cpu.ei_shadow);
        assert_eq!(cpu.step(), 13);
        assert_eq!(cpu.reg(Reg::PC), 0x0038);
        assert_eq!(cpu.reg(Reg::SP), 0x7ffe);
        assert_eq!(cpu.mem_peek(0x7ffe), 0x02);
        assert_eq!(cpu.mem_peek(0x7fff), 0x00);
    }

    #[test]
    fn interrupts_nmi_is_edge_latched_and_preserves_iff1_in_iff2() {
        let mut cpu = machine();
        cpu.write_internal_io(DCNTL, 0x00);
        cpu.mem_poke(0x1234, 0x00);
        cpu.mem_poke(0x0066, 0x00);
        cpu.set_reg(Reg::PC, 0x1234);
        cpu.set_reg(Reg::SP, 0x8000);
        cpu.set_reg(Reg::IR, 0x5600);
        cpu.set_iff1(true);
        cpu.set_iff2(false);

        cpu.set_nmi(true);
        assert_eq!(cpu.step(), 11);
        assert_eq!(cpu.reg(Reg::PC), 0x0066);
        assert_eq!(cpu.reg(Reg::SP), 0x7ffe);
        assert_eq!(cpu.mem_peek(0x7ffe), 0x34);
        assert_eq!(cpu.mem_peek(0x7fff), 0x12);
        assert!(!cpu.iff1());
        assert!(cpu.iff2());
        assert_eq!(cpu.reg(Reg::IR), 0x5601);

        cpu.set_nmi(true);
        assert_eq!(cpu.step(), 3);
        assert_eq!(cpu.reg(Reg::PC), 0x0067);

        cpu.set_nmi(false);
        cpu.set_nmi(true);
        assert_eq!(cpu.step(), 11);
        assert_eq!(cpu.reg(Reg::PC), 0x0066);
        assert_eq!(cpu.reg(Reg::SP), 0x7ffc);
    }

    #[test]
    fn interrupts_int0_modes_use_fixed_ff_acknowledge_data() {
        for (mode, expected_cycles, expected_pc) in
            [(0, 13, 0x0038), (1, 11, 0x0038), (2, 18, 0x5678)]
        {
            let mut cpu = machine();
            cpu.write_internal_io(DCNTL, 0x00);
            cpu.mem_poke(0x12ff, 0x78);
            cpu.mem_poke(0x1300, 0x56);
            cpu.set_reg(Reg::PC, 0x3456);
            cpu.set_reg(Reg::SP, 0x8000);
            cpu.set_reg(Reg::IR, 0x1200);
            cpu.set_interrupt_mode(mode);
            cpu.set_iff1(true);
            cpu.set_iff2(true);
            cpu.set_irq(IrqLine::Int0, true);

            assert_eq!(cpu.step(), expected_cycles, "IM{mode}");
            assert_eq!(cpu.reg(Reg::PC), expected_pc, "IM{mode}");
            assert_eq!(cpu.reg(Reg::SP), 0x7ffe, "IM{mode}");
            assert_eq!(cpu.mem_peek(0x7ffe), 0x56, "IM{mode}");
            assert_eq!(cpu.mem_peek(0x7fff), 0x34, "IM{mode}");
            assert!(!cpu.iff1(), "IM{mode}");
            assert!(!cpu.iff2(), "IM{mode}");
            assert_eq!(cpu.reg(Reg::IR), 0x1201, "IM{mode}");
        }
    }

    #[test]
    fn interrupts_vector_through_i_il_in_um0050_priority_order() {
        let mut external = machine();
        external.write_internal_io(DCNTL, 0x00);
        external.write_internal_io(IL, 0xa0);
        external.write_internal_io(ITC, 0x07);
        external.mem_poke(0x20a0, 0x11);
        external.mem_poke(0x20a1, 0x11);
        external.mem_poke(0x20a2, 0x22);
        external.mem_poke(0x20a3, 0x22);
        external.set_reg(Reg::IR, 0x2000);
        external.set_reg(Reg::SP, 0x8000);
        external.set_iff1(true);
        external.set_irq(IrqLine::Int1, true);
        external.set_irq(IrqLine::Int2, true);

        assert_eq!(external.step(), 18);
        assert_eq!(external.reg(Reg::PC), 0x1111, "INT1 precedes INT2");

        let mut internal = machine();
        internal.write_internal_io(DCNTL, 0x00);
        internal.write_internal_io(IL, 0xa0);
        internal.mem_poke(0x20a6, 0x33);
        internal.mem_poke(0x20a7, 0x33);
        internal.mem_poke(0x20a8, 0x44);
        internal.mem_poke(0x20a9, 0x44);
        internal.set_reg(Reg::IR, 0x2000);
        internal.set_reg(Reg::SP, 0x8000);
        internal.set_iff1(true);
        internal.internal_irq_pending = 0x02 | 0x04;

        assert_eq!(internal.step(), 18);
        assert_eq!(internal.reg(Reg::PC), 0x3333, "PRT1 precedes DMA0");
    }

    #[test]
    fn peripheral_interrupts_vector_in_every_adjacent_priority_pair() {
        for (pair, name, higher_bit, lower_bit, higher_code, lower_code) in [
            (0_u8, "PRT0 > PRT1", 0x01_u8, 0x02_u8, 0x04_u8, 0x06_u8),
            (1, "PRT1 > DMA0", 0x02, 0x04, 0x06, 0x08),
            (2, "DMA0 > DMA1", 0x04, 0x08, 0x08, 0x0a),
            (3, "DMA1 > CSI/O", 0x08, 0x10, 0x0a, 0x0c),
            (4, "CSI/O > ASCI0", 0x10, 0x20, 0x0c, 0x0e),
            (5, "ASCI0 > ASCI1", 0x20, 0x40, 0x0e, 0x10),
        ] {
            let mut cpu = machine();
            cpu.write_internal_io(DCNTL, 0x00);
            cpu.write_internal_io(IL, 0xa0);
            cpu.set_reg(Reg::IR, 0x2000);
            cpu.set_reg(Reg::SP, 0x8000);

            let higher_target = 0x4000_u16 | (u16::from(pair) << 8) | u16::from(higher_code);
            let lower_target = 0x5000_u16 | (u16::from(pair) << 8) | u16::from(lower_code);
            let higher_vector = 0x20a0_u32 | u32::from(higher_code);
            let lower_vector = 0x20a0_u32 | u32::from(lower_code);
            cpu.mem_poke(higher_vector, higher_target as u8);
            cpu.mem_poke(higher_vector + 1, (higher_target >> 8) as u8);
            cpu.mem_poke(lower_vector, lower_target as u8);
            cpu.mem_poke(lower_vector + 1, (lower_target >> 8) as u8);

            match pair {
                0 => {
                    cpu.write_internal_io(TMDR0L, 0x01);
                    cpu.write_internal_io(TMDR0H, 0x00);
                    cpu.write_internal_io(TMDR1L, 0x01);
                    cpu.write_internal_io(TMDR1H, 0x00);
                    cpu.write_internal_io(TCR, 0x33);
                    cpu.finish_step(20);
                }
                1 => {
                    cpu.write_internal_io(TMDR1L, 0x01);
                    cpu.write_internal_io(TMDR1H, 0x00);
                    cpu.write_internal_io(TCR, 0x22);
                    cpu.finish_step(20);
                    cpu.write_internal_io(DSTAT, 0x34);
                }
                2 => cpu.write_internal_io(DSTAT, 0x3c),
                3 => {
                    cpu.write_internal_io(DSTAT, 0x38);
                    cpu.write_internal_io(TRD, 0xa5);
                    cpu.write_internal_io(CNTR, 0x50);
                    cpu.finish_step(160);
                }
                4 => {
                    cpu.write_internal_io(TRD, 0xa5);
                    cpu.write_internal_io(CNTR, 0x50);
                    cpu.finish_step(160);
                    cpu.write_internal_io(STAT0, 0x01);
                }
                5 => {
                    cpu.write_internal_io(STAT0, 0x01);
                    cpu.write_internal_io(STAT1, 0x01);
                }
                _ => unreachable!(),
            }

            assert_eq!(
                cpu.internal_irq_pending & (higher_bit | lower_bit),
                higher_bit | lower_bit,
                "{name} real requests"
            );
            cpu.set_iff1(true);
            assert_eq!(cpu.step(), 18, "{name} higher acknowledge");
            assert_eq!(cpu.reg(Reg::PC), higher_target, "{name} higher vector");

            match pair {
                0 => {
                    assert_eq!(cpu.read_internal_io(TCR) & 0xc0, 0xc0);
                    let _ = cpu.read_internal_io(TMDR0L);
                }
                1 => {
                    assert_eq!(cpu.read_internal_io(TCR) & 0x80, 0x80);
                    let _ = cpu.read_internal_io(TMDR1L);
                }
                2 => cpu.write_internal_io(DSTAT, 0x38),
                3 => cpu.write_internal_io(DSTAT, 0x30),
                4 => {
                    let _ = cpu.read_internal_io(TRD);
                }
                5 => cpu.write_internal_io(STAT0, 0x00),
                _ => unreachable!(),
            }

            assert_eq!(
                cpu.internal_irq_pending & (higher_bit | lower_bit),
                lower_bit,
                "{name} lower remains after real higher-source clear"
            );
            cpu.set_iff1(true);
            assert_eq!(cpu.step(), 18, "{name} lower acknowledge");
            assert_eq!(cpu.reg(Reg::PC), lower_target, "{name} lower vector");
        }
    }

    #[test]
    fn interrupts_vector_dispatch_matrix_covers_every_source_gate_and_iff_state() {
        let sources = [
            (IrqSource::Nmi, None, 0x00, 0x00, None),
            (IrqSource::Int0, Some(IrqLine::Int0), 0x00, 0x01, None),
            (
                IrqSource::Int1,
                Some(IrqLine::Int1),
                0x00,
                0x02,
                Some(0x00_u8),
            ),
            (IrqSource::Int2, Some(IrqLine::Int2), 0x00, 0x04, Some(0x02)),
            (IrqSource::Prt0, None, 0x01, 0x00, Some(0x04)),
            (IrqSource::Prt1, None, 0x02, 0x00, Some(0x06)),
            (IrqSource::Dma0, None, 0x04, 0x00, Some(0x08)),
            (IrqSource::Dma1, None, 0x08, 0x00, Some(0x0a)),
            (IrqSource::Csio, None, 0x10, 0x00, Some(0x0c)),
            (IrqSource::Asci0, None, 0x20, 0x00, Some(0x0e)),
            (IrqSource::Asci1, None, 0x40, 0x00, Some(0x10)),
        ];

        for (source, external_line, internal_bit, itc_bit, fixed_code) in sources {
            for enabled in [false, true] {
                for iff1 in [false, true] {
                    let mut cpu = machine();
                    cpu.write_internal_io(DCNTL, 0x00);
                    cpu.write_internal_io(IL, 0xa0);
                    cpu.write_internal_io(ITC, if enabled { itc_bit } else { 0 });
                    cpu.mem_poke(0x0100, 0x00);
                    cpu.set_reg(Reg::PC, 0x0100);
                    cpu.set_reg(Reg::SP, 0x8000);
                    cpu.set_reg(Reg::IR, 0x2000);
                    cpu.set_interrupt_mode(1);
                    cpu.set_iff1(iff1);
                    cpu.set_iff2(true);

                    let expected_pc = match source {
                        IrqSource::Nmi => 0x0066,
                        IrqSource::Int0 => 0x0038,
                        _ => {
                            let code = fixed_code.expect("vectored source must have a fixed code");
                            let vector = 0x20a0 | u16::from(code);
                            let target = 0x4000 | u16::from(code);
                            cpu.mem_poke(u32::from(vector), target as u8);
                            cpu.mem_poke(u32::from(vector.wrapping_add(1)), (target >> 8) as u8);
                            target
                        }
                    };

                    if source == IrqSource::Nmi {
                        cpu.set_nmi(enabled);
                    } else if let Some(line) = external_line {
                        cpu.set_irq(line, true);
                    } else {
                        cpu.internal_irq_pending = if enabled { internal_bit } else { 0 };
                    }

                    let should_service = enabled && (source == IrqSource::Nmi || iff1);
                    let cycles = cpu.step();
                    if should_service {
                        let expected_cycles = match source {
                            IrqSource::Nmi | IrqSource::Int0 => 11,
                            _ => 18,
                        };
                        assert_eq!(
                            cycles, expected_cycles,
                            "{source:?} enabled={enabled} iff1={iff1}"
                        );
                        assert_eq!(cpu.reg(Reg::PC), expected_pc, "{source:?}");
                        assert_eq!(cpu.reg(Reg::SP), 0x7ffe, "{source:?}");
                        assert_eq!(cpu.mem_peek(0x7ffe), 0x00, "{source:?}");
                        assert_eq!(cpu.mem_peek(0x7fff), 0x01, "{source:?}");
                        assert!(!cpu.iff1(), "{source:?}");
                        if source == IrqSource::Nmi {
                            assert_eq!(cpu.iff2(), iff1, "{source:?}");
                        } else {
                            assert!(!cpu.iff2(), "{source:?}");
                        }
                    } else {
                        assert_eq!(cycles, 3, "{source:?} enabled={enabled} iff1={iff1}");
                        assert_eq!(cpu.reg(Reg::PC), 0x0101, "{source:?}");
                        assert_eq!(cpu.reg(Reg::SP), 0x8000, "{source:?}");
                        assert_eq!(cpu.iff1(), iff1, "{source:?}");
                        assert!(cpu.iff2(), "{source:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn interrupts_halt_and_sleep_follow_distinct_wake_rules() {
        let mut halted = machine();
        halted.write_internal_io(DCNTL, 0x00);
        halted.mem_poke(0, 0x76);
        halted.set_reg(Reg::SP, 0x8000);
        halted.set_iff1(true);
        assert_eq!(halted.step(), 3);
        assert!(halted.halted());
        halted.set_irq(IrqLine::Int0, true);
        assert_eq!(halted.step(), 13);
        assert!(!halted.halted());
        assert_eq!(halted.reg(Reg::PC), 0x0038);

        let mut sleeping = machine();
        sleeping.write_internal_io(DCNTL, 0x00);
        sleeping.mem_poke(0, 0xed);
        sleeping.mem_poke(1, 0x76);
        sleeping.mem_poke(2, 0x00);
        assert_eq!(sleeping.step(), 8);
        assert!(sleeping.sleeping());
        sleeping.set_irq(IrqLine::Int1, true);
        assert_eq!(sleeping.step(), 0, "disabled INT1 is ignored");
        assert!(sleeping.sleeping());

        sleeping.write_internal_io(ITC, 0x03);
        assert_eq!(sleeping.step(), 3, "enabled INT1 wakes with IEF1 clear");
        assert!(!sleeping.sleeping());
        assert_eq!(sleeping.reg(Reg::PC), 3);

        let mut serviced = machine();
        serviced.write_internal_io(DCNTL, 0x00);
        serviced.write_internal_io(ITC, 0x03);
        serviced.mem_poke(0, 0xed);
        serviced.mem_poke(1, 0x76);
        serviced.mem_poke(0x2000, 0x56);
        serviced.mem_poke(0x2001, 0x34);
        serviced.set_reg(Reg::IR, 0x2000);
        serviced.set_reg(Reg::SP, 0x8000);
        serviced.set_iff1(true);
        assert_eq!(serviced.step(), 8);
        serviced.set_irq(IrqLine::Int1, true);
        assert_eq!(serviced.step(), 18);
        assert!(!serviced.sleeping());
        assert_eq!(serviced.reg(Reg::PC), 0x3456);
    }

    fn expected_daa(accumulator: u8, flags: u8) -> (u8, u8) {
        let subtract = flags & FLAG_N != 0;
        let low_adjust = flags & FLAG_H != 0 || accumulator & 0x0f > 9;
        let high_adjust = flags & FLAG_C != 0 || accumulator > 0x99;
        let correction = u8::from(low_adjust) * 0x06 + u8::from(high_adjust) * 0x60;
        let result = if subtract {
            accumulator.wrapping_sub(correction)
        } else {
            accumulator.wrapping_add(correction)
        };
        let parity = u8::from(result.count_ones() & 1 == 0) * FLAG_PV;
        let half_carry = u8::from((accumulator ^ result) & 0x10 != 0) * FLAG_H;
        let zero = u8::from(result == 0) * FLAG_Z;
        let expected_flags = (result & (FLAG_S | FLAG_XY))
            | zero
            | half_carry
            | parity
            | (flags & FLAG_N)
            | (u8::from(high_adjust) * FLAG_C);
        (result, expected_flags)
    }

    #[test]
    fn nop_advances_pc_and_r_without_changing_registers() {
        let mut cpu = machine();
        cpu.mem_poke(0x1234, 0x00);
        cpu.set_reg(Reg::PC, 0x1234);
        cpu.set_reg(Reg::AF, 0x56d7);
        cpu.set_reg(Reg::BC, 0x89ab);
        cpu.set_reg(Reg::IR, 0x34ff);

        assert_eq!(cpu.step(), 6);

        assert_eq!(cpu.instruction_pc(), 0x1234);
        assert_eq!(cpu.reg(Reg::PC), 0x1235);
        assert_eq!(cpu.reg(Reg::AF), 0x56d7);
        assert_eq!(cpu.reg(Reg::BC), 0x89ab);
        assert_eq!(cpu.reg(Reg::IR), 0x3480);
        assert_eq!(cpu.cycle_count(), 6);
        assert!(!cpu.halted());
    }

    #[test]
    fn halt_enters_halted_state_and_leaves_flags_unchanged() {
        let mut cpu = machine();
        cpu.mem_poke(0, 0x76);
        cpu.set_reg(Reg::AF, 0x12a5);

        assert_eq!(cpu.step(), 6);
        assert_eq!(cpu.step(), 6);

        assert!(cpu.halted());
        assert_eq!(cpu.reg(Reg::PC), 1);
        assert_eq!(cpu.reg(Reg::AF), 0x12a5);
    }

    #[test]
    fn ld_register_to_register_preserves_flags() {
        let mut cpu = machine();
        cpu.mem_poke(0, 0x78);
        cpu.set_reg(Reg::AF, 0x11d5);
        cpu.set_reg(Reg::BC, 0x42ee);

        cpu.step();

        assert_eq!(cpu.reg(Reg::AF), 0x42d5);
    }

    #[test]
    fn ld_reads_and_writes_through_hl() {
        let mut cpu = machine();
        cpu.mem_poke(0, 0x46);
        cpu.mem_poke(1, 0x70);
        cpu.mem_poke(0x2000, 0x5a);
        cpu.set_reg(Reg::HL, 0x2000);

        cpu.step();
        assert_eq!(cpu.reg(Reg::BC).to_be_bytes()[0], 0x5a);

        cpu.set_reg(Reg::BC, 0xa5ff);
        cpu.step();
        assert_eq!(cpu.mem_peek(0x2000), 0xa5);
    }

    #[test]
    fn opcode_76_is_halt_not_ld_hl_hl() {
        let mut cpu = machine();
        cpu.mem_poke(0, 0x76);
        cpu.mem_poke(0x2222, 0x7c);
        cpu.set_reg(Reg::HL, 0x2222);

        cpu.step();

        assert!(cpu.halted());
        assert_eq!(cpu.mem_peek(0x2222), 0x7c);
    }

    #[test]
    fn reset_preserves_memory_and_clears_r_and_halt() {
        let mut cpu = machine();
        cpu.mem_poke(0, 0x76);
        cpu.set_reg(Reg::IR, 0xab7f);
        cpu.set_iff1(true);
        cpu.set_iff2(true);
        cpu.set_interrupt_mode(2);
        cpu.step();
        cpu.mem_poke(0x4000, 0xcc);

        cpu.reset();

        assert_eq!(cpu.reg(Reg::IR), 0);
        assert_eq!(cpu.reg(Reg::PC), 0);
        assert!(!cpu.halted());
        assert!(!cpu.iff1());
        assert!(!cpu.iff2());
        assert_eq!(cpu.interrupt_mode(), 0);
        assert_eq!(cpu.mem_peek(0x4000), 0xcc);
    }

    #[test]
    fn z180_repeat_block_output_sets_terminal_flags() {
        let cases: [(u8, u16, u16, u16, u8, u8); 2] = [
            (0x93, 0x2000, 0x2001, 0x2002, 0x40, 0x42),
            (0x9b, 0x2001, 0x2000, 0x1fff, 0x42, 0x40),
        ];
        for (opcode, initial_hl, final_value_address, final_hl, initial_c, final_c) in cases {
            let mut cpu = machine();
            cpu.mem_poke(0, 0xed);
            cpu.mem_poke(1, opcode);
            cpu.mem_poke(initial_hl.into(), 0x01);
            cpu.mem_poke(final_value_address.into(), 0x80);
            cpu.set_reg(Reg::AF, 0x55ff);
            cpu.set_reg(Reg::BC, u16::from_be_bytes([2, initial_c]));
            cpu.set_reg(Reg::HL, initial_hl);

            assert_eq!(cpu.step(), 50, "ED {opcode:02x}");
            assert_eq!(cpu.reg(Reg::AF), 0x5546, "ED {opcode:02x}");
            assert_eq!(cpu.reg(Reg::BC), u16::from(final_c), "ED {opcode:02x}");
            assert_eq!(cpu.reg(Reg::HL), final_hl, "ED {opcode:02x}");
        }
    }

    #[test]
    fn reti_preserves_iffs_while_retn_restores_iff1() {
        let mut reti = machine();
        reti.mem_poke(0, 0xed);
        reti.mem_poke(1, 0x4d);
        reti.mem_poke(0x2000, 0x34);
        reti.mem_poke(0x2001, 0x12);
        reti.set_reg(Reg::SP, 0x2000);
        reti.set_iff1(false);
        reti.set_iff2(true);

        assert_eq!(reti.step(), 34);

        assert_eq!(reti.reg(Reg::PC), 0x1234);
        assert_eq!(reti.reg(Reg::SP), 0x2002);
        assert!(!reti.iff1());
        assert!(reti.iff2());

        let mut retn = machine();
        retn.mem_poke(0, 0xed);
        retn.mem_poke(1, 0x45);
        retn.mem_poke(0x2000, 0x78);
        retn.mem_poke(0x2001, 0x56);
        retn.set_reg(Reg::SP, 0x2000);
        retn.set_iff1(false);
        retn.set_iff2(true);

        assert_eq!(retn.step(), 24);

        assert_eq!(retn.reg(Reg::PC), 0x5678);
        assert_eq!(retn.reg(Reg::SP), 0x2002);
        assert!(retn.iff1());
        assert!(retn.iff2());

        let config = MachineConfig {
            variant: Variant::Z8S180,
            regions: vec![RegionDef {
                base: 0,
                size: 0x1_0000,
                kind: RegionKind::Ram,
            }],
            ..MachineConfig::default()
        };
        let mut z8s180_reti =
            Z180::new(config, NullBus).expect("flat Z8S180 RAM configuration must be valid");
        z8s180_reti.mem_poke(0, 0xed);
        z8s180_reti.mem_poke(1, 0x4d);
        z8s180_reti.mem_poke(0x2000, 0x34);
        z8s180_reti.mem_poke(0x2001, 0x12);
        z8s180_reti.set_reg(Reg::SP, 0x2000);
        assert_eq!(z8s180_reti.step(), 24);
    }

    #[test]
    fn timing_selects_conditional_and_repeat_paths() {
        let mut call_not_taken = machine();
        call_not_taken.mem_poke(0, 0xc4);
        call_not_taken.mem_poke(1, 0x34);
        call_not_taken.mem_poke(2, 0x12);
        call_not_taken.set_reg(Reg::AF, u16::from(FLAG_Z));
        assert_eq!(call_not_taken.step(), 15);
        assert_eq!(call_not_taken.reg(Reg::PC), 3);

        let mut call_taken = machine();
        call_taken.mem_poke(0, 0xc4);
        call_taken.mem_poke(1, 0x34);
        call_taken.mem_poke(2, 0x12);
        call_taken.set_reg(Reg::SP, 0x2000);
        assert_eq!(call_taken.step(), 31);
        assert_eq!(call_taken.reg(Reg::PC), 0x1234);

        let mut ldir_terminal = machine();
        ldir_terminal.mem_poke(0, 0xed);
        ldir_terminal.mem_poke(1, 0xb0);
        ldir_terminal.mem_poke(0x1000, 0x5a);
        ldir_terminal.set_reg(Reg::BC, 1);
        ldir_terminal.set_reg(Reg::DE, 0x2000);
        ldir_terminal.set_reg(Reg::HL, 0x1000);
        assert_eq!(ldir_terminal.step(), 24);
        assert_eq!(ldir_terminal.reg(Reg::PC), 2);

        let mut ldir_repeating = machine();
        ldir_repeating.mem_poke(0, 0xed);
        ldir_repeating.mem_poke(1, 0xb0);
        ldir_repeating.mem_poke(0x1000, 0x5a);
        ldir_repeating.set_reg(Reg::BC, 2);
        ldir_repeating.set_reg(Reg::DE, 0x2000);
        ldir_repeating.set_reg(Reg::HL, 0x1000);
        assert_eq!(ldir_repeating.step(), 26);
        assert_eq!(ldir_repeating.reg(Reg::PC), 0);
    }

    #[test]
    fn timing_spot_checks_hand_computed_program_totals() {
        let mut checked = 0_u8;

        // 1. Two NOPs: 2 * (3 base + 1 memory access * 3 waits) = 12.
        {
            let mut cpu = machine();
            cpu.mem_poke(0, 0x00);
            cpu.mem_poke(1, 0x00);
            cpu.step();
            cpu.step();
            assert_eq!(cpu.cycle_count(), 12, "NOP; NOP");
            checked += 1;
        }

        // 2. LD BC,nn; INC BC: (9 + 3*3) + (4 + 1*3) = 25.
        {
            let mut cpu = machine();
            cpu.mem_poke(0, 0x01);
            cpu.mem_poke(1, 0x34);
            cpu.mem_poke(2, 0x12);
            cpu.mem_poke(3, 0x03);
            cpu.step();
            cpu.step();
            assert_eq!(cpu.cycle_count(), 25, "LD BC,1234h; INC BC");
            checked += 1;
        }

        // 3. LD B,n; LD C,B: (6 + 2*3) + (4 + 1*3) = 19.
        {
            let mut cpu = machine();
            cpu.mem_poke(0, 0x06);
            cpu.mem_poke(1, 0x5a);
            cpu.mem_poke(2, 0x48);
            cpu.step();
            cpu.step();
            assert_eq!(cpu.cycle_count(), 19, "LD B,5Ah; LD C,B");
            checked += 1;
        }

        // 4. LD HL,nn; LD A,(HL): (9 + 3*3) + (6 + 2*3) = 30.
        {
            let mut cpu = machine();
            cpu.mem_poke(0, 0x21);
            cpu.mem_poke(1, 0x00);
            cpu.mem_poke(2, 0x20);
            cpu.mem_poke(3, 0x7e);
            cpu.mem_poke(0x2000, 0x5a);
            cpu.step();
            cpu.step();
            assert_eq!(cpu.cycle_count(), 30, "LD HL,2000h; LD A,(HL)");
            checked += 1;
        }

        // 5. LD HL,nn; LD (HL),n: (9 + 3*3) + (9 + 3*3) = 36.
        {
            let mut cpu = machine();
            cpu.mem_poke(0, 0x21);
            cpu.mem_poke(1, 0x00);
            cpu.mem_poke(2, 0x20);
            cpu.mem_poke(3, 0x36);
            cpu.mem_poke(4, 0xa5);
            cpu.step();
            cpu.step();
            assert_eq!(cpu.cycle_count(), 36, "LD HL,2000h; LD (HL),A5h");
            checked += 1;
        }

        // 6. INC (HL): 10 base + 3 memory accesses * 3 waits = 19.
        {
            let mut cpu = machine();
            cpu.mem_poke(0, 0x34);
            cpu.mem_poke(0x2000, 0x7f);
            cpu.set_reg(Reg::HL, 0x2000);
            cpu.step();
            assert_eq!(cpu.cycle_count(), 19, "INC (HL)");
            checked += 1;
        }

        // 7. JR NZ untaken: 6 base + 2 memory accesses * 3 waits = 12.
        {
            let mut cpu = machine();
            cpu.mem_poke(0, 0x20);
            cpu.mem_poke(1, 0x02);
            cpu.set_reg(Reg::AF, u16::from(FLAG_Z));
            cpu.step();
            assert_eq!(cpu.cycle_count(), 12, "JR NZ untaken");
            checked += 1;
        }

        // 8. JR NZ taken: 8 base + 2 memory accesses * 3 waits = 14.
        {
            let mut cpu = machine();
            cpu.mem_poke(0, 0x20);
            cpu.mem_poke(1, 0x02);
            cpu.step();
            assert_eq!(cpu.cycle_count(), 14, "JR NZ taken");
            checked += 1;
        }

        // 9. DJNZ untaken: 7 base + 2 memory accesses * 3 waits = 13.
        {
            let mut cpu = machine();
            cpu.mem_poke(0, 0x10);
            cpu.mem_poke(1, 0x02);
            cpu.set_reg(Reg::BC, 0x0100);
            cpu.step();
            assert_eq!(cpu.cycle_count(), 13, "DJNZ untaken");
            checked += 1;
        }

        // 10. DJNZ taken: 9 base + 2 memory accesses * 3 waits = 15.
        {
            let mut cpu = machine();
            cpu.mem_poke(0, 0x10);
            cpu.mem_poke(1, 0x02);
            cpu.set_reg(Reg::BC, 0x0200);
            cpu.step();
            assert_eq!(cpu.cycle_count(), 15, "DJNZ taken");
            checked += 1;
        }

        // 11. JP NZ untaken: 6 base + 3 memory accesses * 3 waits = 15.
        {
            let mut cpu = machine();
            cpu.mem_poke(0, 0xc2);
            cpu.mem_poke(1, 0x34);
            cpu.mem_poke(2, 0x12);
            cpu.set_reg(Reg::AF, u16::from(FLAG_Z));
            cpu.step();
            assert_eq!(cpu.cycle_count(), 15, "JP NZ untaken");
            checked += 1;
        }

        // 12. JP NZ taken: 9 base + 3 memory accesses * 3 waits = 18.
        {
            let mut cpu = machine();
            cpu.mem_poke(0, 0xc2);
            cpu.mem_poke(1, 0x34);
            cpu.mem_poke(2, 0x12);
            cpu.step();
            assert_eq!(cpu.cycle_count(), 18, "JP NZ taken");
            checked += 1;
        }

        // 13. CALL NZ untaken: 6 base + 3 memory accesses * 3 waits = 15.
        {
            let mut cpu = machine();
            cpu.mem_poke(0, 0xc4);
            cpu.mem_poke(1, 0x34);
            cpu.mem_poke(2, 0x12);
            cpu.set_reg(Reg::AF, u16::from(FLAG_Z));
            cpu.step();
            assert_eq!(cpu.cycle_count(), 15, "CALL NZ untaken");
            checked += 1;
        }

        // 14. CALL NZ taken: 16 base + 5 memory accesses * 3 waits = 31.
        {
            let mut cpu = machine();
            cpu.mem_poke(0, 0xc4);
            cpu.mem_poke(1, 0x34);
            cpu.mem_poke(2, 0x12);
            cpu.set_reg(Reg::SP, 0x2000);
            cpu.step();
            assert_eq!(cpu.cycle_count(), 31, "CALL NZ taken");
            checked += 1;
        }

        // 15. RET NZ untaken: 5 base + 1 memory access * 3 waits = 8.
        {
            let mut cpu = machine();
            cpu.mem_poke(0, 0xc0);
            cpu.set_reg(Reg::AF, u16::from(FLAG_Z));
            cpu.step();
            assert_eq!(cpu.cycle_count(), 8, "RET NZ untaken");
            checked += 1;
        }

        // 16. RET NZ taken: 10 base + 3 memory accesses * 3 waits = 19.
        {
            let mut cpu = machine();
            cpu.mem_poke(0, 0xc0);
            cpu.mem_poke(0x2000, 0x34);
            cpu.mem_poke(0x2001, 0x12);
            cpu.set_reg(Reg::SP, 0x2000);
            cpu.step();
            assert_eq!(cpu.cycle_count(), 19, "RET NZ taken");
            checked += 1;
        }

        // 17. EX DE,HL; EXX: (3 + 1*3) + (3 + 1*3) = 12.
        {
            let mut cpu = machine();
            cpu.mem_poke(0, 0xeb);
            cpu.mem_poke(1, 0xd9);
            cpu.step();
            cpu.step();
            assert_eq!(cpu.cycle_count(), 12, "EX DE,HL; EXX");
            checked += 1;
        }

        // 18. EX (SP),HL: 16 base + 5 memory accesses * 3 waits = 31.
        {
            let mut cpu = machine();
            cpu.mem_poke(0, 0xe3);
            cpu.mem_poke(0x2000, 0x34);
            cpu.mem_poke(0x2001, 0x12);
            cpu.set_reg(Reg::SP, 0x2000);
            cpu.step();
            assert_eq!(cpu.cycle_count(), 31, "EX (SP),HL");
            checked += 1;
        }

        // 19. MLT BC: 17 base + 2 memory accesses * 3 waits = 23.
        {
            let mut cpu = machine();
            cpu.mem_poke(0, 0xed);
            cpu.mem_poke(1, 0x4c);
            cpu.set_reg(Reg::BC, 0x1234);
            cpu.step();
            assert_eq!(cpu.cycle_count(), 23, "MLT BC");
            checked += 1;
        }

        // 20. LDI: 12 base + 4 memory accesses * 3 waits = 24.
        {
            let mut cpu = machine();
            cpu.mem_poke(0, 0xed);
            cpu.mem_poke(1, 0xa0);
            cpu.mem_poke(0x2000, 0x5a);
            cpu.set_reg(Reg::BC, 1);
            cpu.set_reg(Reg::DE, 0x3000);
            cpu.set_reg(Reg::HL, 0x2000);
            cpu.step();
            assert_eq!(cpu.cycle_count(), 24, "LDI");
            checked += 1;
        }

        // 21. Terminal LDIR: 12 base + 4 memory accesses * 3 waits = 24.
        {
            let mut cpu = machine();
            cpu.mem_poke(0, 0xed);
            cpu.mem_poke(1, 0xb0);
            cpu.mem_poke(0x2000, 0x5a);
            cpu.set_reg(Reg::BC, 1);
            cpu.set_reg(Reg::DE, 0x3000);
            cpu.set_reg(Reg::HL, 0x2000);
            cpu.step();
            assert_eq!(cpu.cycle_count(), 24, "LDIR terminal");
            checked += 1;
        }

        // 22. Repeating LDIR: 14 base + 4 memory accesses * 3 waits = 26.
        {
            let mut cpu = machine();
            cpu.mem_poke(0, 0xed);
            cpu.mem_poke(1, 0xb0);
            cpu.mem_poke(0x2000, 0x5a);
            cpu.set_reg(Reg::BC, 2);
            cpu.set_reg(Reg::DE, 0x3000);
            cpu.set_reg(Reg::HL, 0x2000);
            cpu.step();
            assert_eq!(cpu.cycle_count(), 26, "LDIR repeating");
            checked += 1;
        }

        // 23. OTIM: 14 base + 3 memory accesses * 3 waits + 4 I/O waits = 27.
        {
            let mut cpu = machine();
            cpu.mem_poke(0, 0xed);
            cpu.mem_poke(1, 0x83);
            cpu.mem_poke(0x2000, 0x5a);
            cpu.set_reg(Reg::BC, 0x0140);
            cpu.set_reg(Reg::HL, 0x2000);
            cpu.step();
            assert_eq!(cpu.cycle_count(), 27, "OTIM");
            checked += 1;
        }

        // 24. OTIMR B=2: 30 base + 4 memory accesses * 3 waits + 2*4 I/O waits = 50.
        {
            let mut cpu = machine();
            cpu.mem_poke(0, 0xed);
            cpu.mem_poke(1, 0x93);
            cpu.mem_poke(0x2000, 0x5a);
            cpu.mem_poke(0x2001, 0xa5);
            cpu.set_reg(Reg::BC, 0x0240);
            cpu.set_reg(Reg::HL, 0x2000);
            cpu.step();
            assert_eq!(cpu.cycle_count(), 50, "OTIMR B=2");
            checked += 1;
        }

        // 25. IN A,(n): 9 base + 2 memory accesses * 3 waits + 4 I/O waits = 19.
        {
            let mut cpu = machine();
            cpu.mem_poke(0, 0xdb);
            cpu.mem_poke(1, 0x40);
            cpu.step();
            assert_eq!(cpu.cycle_count(), 19, "IN A,(40h)");
            checked += 1;
        }

        // 26. OUT (C),B: 10 base + 2 memory accesses * 3 waits + 4 I/O waits = 20.
        {
            let mut cpu = machine();
            cpu.mem_poke(0, 0xed);
            cpu.mem_poke(1, 0x41);
            cpu.set_reg(Reg::BC, 0x5a40);
            cpu.step();
            assert_eq!(cpu.cycle_count(), 20, "OUT (C),B");
            checked += 1;
        }

        assert_eq!(checked, 26);
    }

    #[test]
    fn timing_applies_dcntl_memory_and_external_io_waits() {
        let mut reset_nop = machine();
        reset_nop.mem_poke(0, 0x00);
        assert_eq!(reset_nop.io_reg_peek(DCNTL as u8), 0xf0);
        assert_eq!(reset_nop.step(), 6);

        let mut minimum_nop = machine();
        minimum_nop.io_regs[DCNTL] = 0x00;
        minimum_nop.mem_poke(0, 0x00);
        assert_eq!(minimum_nop.step(), 3);
        minimum_nop.reset();
        assert_eq!(minimum_nop.io_reg_peek(DCNTL as u8), 0xf0);

        let mut reset_in = machine();
        reset_in.mem_poke(0, 0xdb);
        reset_in.mem_poke(1, 0x40);
        assert_eq!(reset_in.step(), 19);

        let mut programmed_in = machine();
        programmed_in.io_regs[DCNTL] = 0x50;
        programmed_in.mem_poke(0, 0xdb);
        programmed_in.mem_poke(1, 0x40);
        assert_eq!(programmed_in.step(), 13);
    }
}
