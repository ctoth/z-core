use super::*;

#[cfg(feature = "state")]
pub(super) const STATE_VERSION: u8 = 4;

#[cfg(feature = "state")]
#[derive(serde::Deserialize, serde::Serialize)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "the save-state schema mirrors the independent hardware bits in Z180 exactly"
)]
pub(super) struct SavedState {
    registers: Registers,
    memory: Memory,
    instruction_pc: u16,
    cycle_count: u64,
    variant: Variant,
    pub(super) io_regs: Vec<u8>,
    timing_branch_taken: bool,
    timing_repeat_iterations: u16,
    timing_memory_waits: u32,
    timing_io_waits: u32,
    halted: bool,
    sleeping: bool,
    iff1: bool,
    iff2: bool,
    ei_shadow: bool,
    pub(super) interrupt_mode: u8,
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
    /// Replaces the current machine state from a versioned save-state payload.
    ///
    /// # Errors
    ///
    /// Returns [`StateError`] when the version or serialized payload is invalid.
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
        if state.interrupt_mode > 2
            || state.events.len() > state.event_capacity
            || state.next_watch_id == 0
            || !insn_trace_is_valid
            || !state.memory.is_valid()
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
        self.indexed_displacement = None;
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
}
