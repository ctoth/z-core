#![forbid(unsafe_code)]

use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use z180_core::{
    ConfigError, Event, HostBus, IrqLine, MachineConfig, StateError, TraceEntry, WatchKind, Z180,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Setup,
    Live,
    Playback,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BusAccess {
    MemRead { address: u32 },
    MemWrite { address: u32, value: u8 },
    IoRead { port: u16 },
    IoWrite { port: u16, value: u8 },
}

#[derive(Debug)]
pub enum ReplayBusError<E> {
    Live(E),
    RecordedHostFailure {
        record: usize,
    },
    Divergence {
        record: usize,
        expected: Option<BusAccess>,
        actual: BusAccess,
    },
    Borrowed,
}

impl<E: core::fmt::Display> core::fmt::Display for ReplayBusError<E> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Live(error) => write!(formatter, "live host bus failed: {error}"),
            Self::RecordedHostFailure { record } => {
                write!(formatter, "recorded host bus failure at record {record}")
            }
            Self::Divergence { record, .. } => {
                write!(formatter, "host bus diverged at record {record}")
            }
            Self::Borrowed => write!(formatter, "replay bus is already borrowed"),
        }
    }
}

impl<E: core::error::Error + 'static> core::error::Error for ReplayBusError<E> {}

#[derive(Clone, Debug)]
struct BusRecord {
    access: BusAccess,
    read_value: Option<u8>,
    failed: bool,
}

struct SharedBus<B> {
    inner: B,
    mode: Mode,
    records: Vec<BusRecord>,
    cursor: usize,
}

pub struct ReplayBus<B> {
    shared: Rc<RefCell<SharedBus<B>>>,
}

impl<B> Clone for ReplayBus<B> {
    fn clone(&self) -> Self {
        Self {
            shared: Rc::clone(&self.shared),
        }
    }
}

impl<B> ReplayBus<B> {
    fn new(inner: B) -> Self {
        Self {
            shared: Rc::new(RefCell::new(SharedBus {
                inner,
                mode: Mode::Setup,
                records: Vec::new(),
                cursor: 0,
            })),
        }
    }

    fn begin_live(&self) -> Result<(), ReplayBusError<core::convert::Infallible>> {
        let mut shared = self
            .shared
            .try_borrow_mut()
            .map_err(|_| ReplayBusError::Borrowed)?;
        shared.mode = Mode::Live;
        shared.records.clear();
        shared.cursor = 0;
        Ok(())
    }

    fn begin_playback(
        &self,
        cursor: usize,
    ) -> Result<(), ReplayBusError<core::convert::Infallible>> {
        let mut shared = self
            .shared
            .try_borrow_mut()
            .map_err(|_| ReplayBusError::Borrowed)?;
        if cursor > shared.records.len() {
            return Err(ReplayBusError::Divergence {
                record: cursor,
                expected: None,
                actual: BusAccess::MemRead { address: 0 },
            });
        }
        shared.mode = Mode::Playback;
        shared.cursor = cursor;
        Ok(())
    }

    fn restore_mode(
        &self,
        mode: Mode,
        cursor: usize,
    ) -> Result<(), ReplayBusError<core::convert::Infallible>> {
        let mut shared = self
            .shared
            .try_borrow_mut()
            .map_err(|_| ReplayBusError::Borrowed)?;
        shared.mode = mode;
        shared.cursor = cursor;
        Ok(())
    }

    fn record_count(&self) -> Result<usize, ReplayBusError<core::convert::Infallible>> {
        self.shared
            .try_borrow()
            .map(|shared| shared.records.len())
            .map_err(|_| ReplayBusError::Borrowed)
    }

    fn cursor(&self) -> Result<usize, ReplayBusError<core::convert::Infallible>> {
        self.shared
            .try_borrow()
            .map(|shared| shared.cursor)
            .map_err(|_| ReplayBusError::Borrowed)
    }

    fn playback_read(
        shared: &mut SharedBus<B>,
        actual: BusAccess,
    ) -> Result<u8, ReplayBusError<core::convert::Infallible>> {
        let index = shared.cursor;
        let Some(record) = shared.records.get(index) else {
            return Err(ReplayBusError::Divergence {
                record: index,
                expected: None,
                actual,
            });
        };
        if record.access != actual {
            return Err(ReplayBusError::Divergence {
                record: index,
                expected: Some(record.access.clone()),
                actual,
            });
        }
        let failed = record.failed;
        let value = record.read_value;
        shared.cursor += 1;
        if failed {
            return Err(ReplayBusError::RecordedHostFailure { record: index });
        }
        value.ok_or_else(|| ReplayBusError::Divergence {
            record: index,
            expected: Some(record.access.clone()),
            actual: record.access.clone(),
        })
    }

    fn playback_write(
        shared: &mut SharedBus<B>,
        actual: BusAccess,
    ) -> Result<(), ReplayBusError<core::convert::Infallible>> {
        let index = shared.cursor;
        let Some(record) = shared.records.get(index) else {
            return Err(ReplayBusError::Divergence {
                record: index,
                expected: None,
                actual,
            });
        };
        if record.access != actual {
            return Err(ReplayBusError::Divergence {
                record: index,
                expected: Some(record.access.clone()),
                actual,
            });
        }
        let failed = record.failed;
        shared.cursor += 1;
        if failed {
            Err(ReplayBusError::RecordedHostFailure { record: index })
        } else {
            Ok(())
        }
    }
}

impl<B: HostBus> HostBus for ReplayBus<B> {
    type Error = ReplayBusError<B::Error>;

    fn mem_read(&mut self, address: u32) -> Result<u8, Self::Error> {
        let mut shared = self
            .shared
            .try_borrow_mut()
            .map_err(|_| ReplayBusError::Borrowed)?;
        let access = BusAccess::MemRead { address };
        if shared.mode == Mode::Playback {
            return ReplayBus::<B>::playback_read(&mut shared, access)
                .map_err(widen_infallible_error);
        }
        match shared.inner.mem_read(address) {
            Ok(value) => {
                shared.records.push(BusRecord {
                    access,
                    read_value: Some(value),
                    failed: false,
                });
                Ok(value)
            }
            Err(error) => {
                shared.records.push(BusRecord {
                    access,
                    read_value: None,
                    failed: true,
                });
                Err(ReplayBusError::Live(error))
            }
        }
    }

    fn mem_write(&mut self, address: u32, value: u8) -> Result<(), Self::Error> {
        let mut shared = self
            .shared
            .try_borrow_mut()
            .map_err(|_| ReplayBusError::Borrowed)?;
        let access = BusAccess::MemWrite { address, value };
        if shared.mode == Mode::Playback {
            return ReplayBus::<B>::playback_write(&mut shared, access)
                .map_err(widen_infallible_error);
        }
        match shared.inner.mem_write(address, value) {
            Ok(()) => {
                shared.records.push(BusRecord {
                    access,
                    read_value: None,
                    failed: false,
                });
                Ok(())
            }
            Err(error) => {
                shared.records.push(BusRecord {
                    access,
                    read_value: None,
                    failed: true,
                });
                Err(ReplayBusError::Live(error))
            }
        }
    }

    fn io_read(&mut self, port: u16) -> Result<u8, Self::Error> {
        let mut shared = self
            .shared
            .try_borrow_mut()
            .map_err(|_| ReplayBusError::Borrowed)?;
        let access = BusAccess::IoRead { port };
        if shared.mode == Mode::Playback {
            return ReplayBus::<B>::playback_read(&mut shared, access)
                .map_err(widen_infallible_error);
        }
        match shared.inner.io_read(port) {
            Ok(value) => {
                shared.records.push(BusRecord {
                    access,
                    read_value: Some(value),
                    failed: false,
                });
                Ok(value)
            }
            Err(error) => {
                shared.records.push(BusRecord {
                    access,
                    read_value: None,
                    failed: true,
                });
                Err(ReplayBusError::Live(error))
            }
        }
    }

    fn io_write(&mut self, port: u16, value: u8) -> Result<(), Self::Error> {
        let mut shared = self
            .shared
            .try_borrow_mut()
            .map_err(|_| ReplayBusError::Borrowed)?;
        let access = BusAccess::IoWrite { port, value };
        if shared.mode == Mode::Playback {
            return ReplayBus::<B>::playback_write(&mut shared, access)
                .map_err(widen_infallible_error);
        }
        match shared.inner.io_write(port, value) {
            Ok(()) => {
                shared.records.push(BusRecord {
                    access,
                    read_value: None,
                    failed: false,
                });
                Ok(())
            }
            Err(error) => {
                shared.records.push(BusRecord {
                    access,
                    read_value: None,
                    failed: true,
                });
                Err(ReplayBusError::Live(error))
            }
        }
    }
}

fn widen_infallible_error<E>(
    error: ReplayBusError<core::convert::Infallible>,
) -> ReplayBusError<E> {
    match error {
        ReplayBusError::Live(never) => match never {},
        ReplayBusError::RecordedHostFailure { record } => {
            ReplayBusError::RecordedHostFailure { record }
        }
        ReplayBusError::Divergence {
            record,
            expected,
            actual,
        } => ReplayBusError::Divergence {
            record,
            expected,
            actual,
        },
        ReplayBusError::Borrowed => ReplayBusError::Borrowed,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Position {
    pub attempted_steps: u64,
    pub cycle: u64,
    pub actions: usize,
    pub bus_records: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Options {
    pub checkpoint_interval_attempts: u64,
    pub max_checkpoints: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            checkpoint_interval_attempts: 10_000,
            max_checkpoints: 64,
        }
    }
}

#[derive(Debug)]
pub enum TimelineConfigError {
    Machine(ConfigError),
    ZeroCheckpointInterval,
    TooFewCheckpoints,
}

impl core::fmt::Display for TimelineConfigError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Machine(error) => write!(formatter, "invalid machine configuration: {error}"),
            Self::ZeroCheckpointInterval => {
                write!(formatter, "checkpoint interval must be greater than zero")
            }
            Self::TooFewCheckpoints => {
                write!(formatter, "at least two checkpoints must be retained")
            }
        }
    }
}

impl core::error::Error for TimelineConfigError {}

#[derive(Debug)]
pub enum TimelineError<E> {
    WrongMode {
        expected: Mode,
        actual: Mode,
    },
    LiveHost(E),
    RecordedHostFailure {
        record: usize,
    },
    BusDivergence {
        record: usize,
        expected: Option<BusAccess>,
        actual: BusAccess,
    },
    BusBorrowed,
    State(StateError),
    InvalidCheckpoint,
    InvalidPosition(Position),
    ActionDivergence {
        action: usize,
    },
    EventHistoryLost,
    RestoreFailed(StateError),
}

impl<E: core::fmt::Display> core::fmt::Display for TimelineError<E> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::WrongMode { expected, actual } => {
                write!(
                    formatter,
                    "operation requires {expected:?} mode, found {actual:?}"
                )
            }
            Self::LiveHost(error) => write!(formatter, "live host bus failed: {error}"),
            Self::RecordedHostFailure { record } => {
                write!(formatter, "recorded host bus failure at record {record}")
            }
            Self::BusDivergence { record, .. } => {
                write!(formatter, "host bus diverged at record {record}")
            }
            Self::BusBorrowed => write!(formatter, "replay bus is already borrowed"),
            Self::State(error) => write!(formatter, "checkpoint state is invalid: {error}"),
            Self::InvalidCheckpoint => write!(formatter, "checkpoint payload is invalid"),
            Self::InvalidPosition(position) => {
                write!(formatter, "timeline position is invalid: {position:?}")
            }
            Self::ActionDivergence { action } => {
                write!(formatter, "host action diverged at record {action}")
            }
            Self::EventHistoryLost => {
                write!(formatter, "event history was lost during the write probe")
            }
            Self::RestoreFailed(error) => {
                write!(formatter, "failed to restore state after probe: {error}")
            }
        }
    }
}

impl<E: core::error::Error + 'static> core::error::Error for TimelineError<E> {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Stimulus {
    Irq { line: IrqLine, level: bool },
    Nmi(bool),
    Dreq { channel: usize, level: bool },
    AsciRx { channel: usize, byte: u8 },
    CsioRx(u8),
    AsciCts { channel: usize, level: bool },
    AsciDcd { channel: usize, level: bool },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StimulusOutcome {
    Applied,
    Rejected,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Output {
    AsciTx(usize),
    CsioTx,
    Events,
    InstructionTrace,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Drained {
    Byte(Option<u8>),
    Events(Vec<Event>),
    InstructionTrace(Vec<TraceEntry>),
}

#[derive(Clone, Debug)]
enum RecordedAction {
    Stimulus {
        stimulus: Stimulus,
        outcome: StimulusOutcome,
    },
    Drain {
        output: Output,
        drained: Drained,
    },
}

#[derive(Clone, Debug)]
struct ActionRecord {
    attempted_step: u64,
    action: RecordedAction,
}

#[derive(Clone, Debug)]
enum AttemptOutcome {
    Success(u32),
    HostFailure { bus_record: usize },
}

#[derive(Clone, Debug)]
struct AttemptRecord {
    outcome: AttemptOutcome,
    end: Position,
}

#[derive(Clone)]
struct Checkpoint {
    position: Position,
    state: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WriteHit {
    pub attempted_step: u64,
    pub event: Event,
}

pub struct Timeline<B: HostBus> {
    machine: Z180<ReplayBus<B>>,
    bus: ReplayBus<B>,
    options: Options,
    mode: Mode,
    position: Position,
    actions: Vec<ActionRecord>,
    attempts: Vec<AttemptRecord>,
    checkpoints: VecDeque<Checkpoint>,
}

impl<B: HostBus> Timeline<B> {
    pub fn new(
        config: MachineConfig,
        bus: B,
        options: Options,
    ) -> Result<Self, TimelineConfigError> {
        if options.checkpoint_interval_attempts == 0 {
            return Err(TimelineConfigError::ZeroCheckpointInterval);
        }
        if options.max_checkpoints < 2 {
            return Err(TimelineConfigError::TooFewCheckpoints);
        }
        let replay_bus = ReplayBus::new(bus);
        let machine =
            Z180::new(config, replay_bus.clone()).map_err(TimelineConfigError::Machine)?;
        Ok(Self {
            machine,
            bus: replay_bus,
            options,
            mode: Mode::Setup,
            position: Position {
                attempted_steps: 0,
                cycle: 0,
                actions: 0,
                bus_records: 0,
            },
            actions: Vec::new(),
            attempts: Vec::new(),
            checkpoints: VecDeque::new(),
        })
    }

    pub fn setup(&mut self) -> Result<&mut Z180<ReplayBus<B>>, TimelineError<B::Error>> {
        if self.mode != Mode::Setup {
            return Err(TimelineError::WrongMode {
                expected: Mode::Setup,
                actual: self.mode,
            });
        }
        Ok(&mut self.machine)
    }

    pub fn start(&mut self) -> Result<(), TimelineError<B::Error>> {
        if self.mode != Mode::Setup {
            return Err(TimelineError::WrongMode {
                expected: Mode::Setup,
                actual: self.mode,
            });
        }
        self.bus.begin_live().map_err(map_control_error)?;
        self.actions.clear();
        self.attempts.clear();
        self.checkpoints.clear();
        self.mode = Mode::Live;
        self.position = Position {
            attempted_steps: 0,
            cycle: self.machine.cycle_count(),
            actions: 0,
            bus_records: 0,
        };
        self.store_checkpoint()
    }

    pub fn machine(&self) -> &Z180<ReplayBus<B>> {
        &self.machine
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn position(&self) -> Position {
        self.position
    }

    pub fn oldest_position(&self) -> Option<Position> {
        self.checkpoints
            .front()
            .map(|checkpoint| checkpoint.position)
    }

    pub fn apply(
        &mut self,
        stimulus: Stimulus,
    ) -> Result<StimulusOutcome, TimelineError<B::Error>> {
        if self.mode != Mode::Live {
            return Err(TimelineError::WrongMode {
                expected: Mode::Live,
                actual: self.mode,
            });
        }
        let outcome = execute_stimulus(&mut self.machine, &stimulus);
        self.actions.push(ActionRecord {
            attempted_step: self.position.attempted_steps,
            action: RecordedAction::Stimulus { stimulus, outcome },
        });
        self.position.actions = self.actions.len();
        Ok(outcome)
    }

    pub fn drain(&mut self, output: Output) -> Result<Drained, TimelineError<B::Error>> {
        if self.mode != Mode::Live {
            return Err(TimelineError::WrongMode {
                expected: Mode::Live,
                actual: self.mode,
            });
        }
        let drained = execute_drain(&mut self.machine, &output);
        self.actions.push(ActionRecord {
            attempted_step: self.position.attempted_steps,
            action: RecordedAction::Drain {
                output,
                drained: drained.clone(),
            },
        });
        self.position.actions = self.actions.len();
        Ok(drained)
    }

    pub fn try_step(&mut self) -> Result<u32, TimelineError<B::Error>> {
        match self.mode {
            Mode::Setup => Err(TimelineError::WrongMode {
                expected: Mode::Live,
                actual: Mode::Setup,
            }),
            Mode::Live => self.try_step_live(),
            Mode::Playback => self.try_step_playback_public(),
        }
    }

    pub fn try_run(&mut self, cycles: u32) -> Result<u32, TimelineError<B::Error>> {
        let mut consumed = 0_u32;
        while consumed < cycles {
            let step_cycles = self.try_step()?;
            consumed = consumed.saturating_add(step_cycles);
            if step_cycles == 0 {
                break;
            }
        }
        Ok(consumed)
    }

    fn try_step_live(&mut self) -> Result<u32, TimelineError<B::Error>> {
        let result = self.machine.try_step();
        self.position.attempted_steps = self.position.attempted_steps.saturating_add(1);
        self.position.cycle = self.machine.cycle_count();
        self.position.actions = self.actions.len();
        self.position.bus_records = self.bus.record_count().map_err(map_control_error)?;

        let public_result = match result {
            Ok(cycles) => {
                self.attempts.push(AttemptRecord {
                    outcome: AttemptOutcome::Success(cycles),
                    end: self.position,
                });
                Ok(cycles)
            }
            Err(ReplayBusError::Live(error)) => {
                let bus_record = self.position.bus_records.saturating_sub(1);
                self.attempts.push(AttemptRecord {
                    outcome: AttemptOutcome::HostFailure { bus_record },
                    end: self.position,
                });
                Err(TimelineError::LiveHost(error))
            }
            Err(error) => return Err(map_bus_error(error)),
        };

        self.maybe_store_checkpoint()?;
        public_result
    }

    fn maybe_store_checkpoint(&mut self) -> Result<(), TimelineError<B::Error>> {
        if self
            .position
            .attempted_steps
            .is_multiple_of(self.options.checkpoint_interval_attempts)
        {
            self.store_checkpoint()?;
        }
        Ok(())
    }

    fn store_checkpoint(&mut self) -> Result<(), TimelineError<B::Error>> {
        let state = self.machine.save_state();
        if state.len() <= 1 {
            return Err(TimelineError::InvalidCheckpoint);
        }
        if self.checkpoints.len() == self.options.max_checkpoints {
            let _ = self.checkpoints.remove(1);
        }
        self.checkpoints.push_back(Checkpoint {
            position: self.position,
            state,
        });
        Ok(())
    }
}

fn execute_stimulus<B: HostBus>(
    machine: &mut Z180<ReplayBus<B>>,
    stimulus: &Stimulus,
) -> StimulusOutcome {
    match *stimulus {
        Stimulus::Irq { line, level } => machine.set_irq(line, level),
        Stimulus::Nmi(level) => machine.set_nmi(level),
        Stimulus::Dreq { channel, level } if channel < 2 => machine.set_dreq(channel, level),
        Stimulus::AsciRx { channel, byte } => {
            return if machine.asci_rx_push(channel, byte) {
                StimulusOutcome::Applied
            } else {
                StimulusOutcome::Rejected
            };
        }
        Stimulus::CsioRx(byte) => {
            return if machine.csio_rx_push(byte) {
                StimulusOutcome::Applied
            } else {
                StimulusOutcome::Rejected
            };
        }
        Stimulus::AsciCts { channel, level } if channel < 2 => {
            machine.set_asci_cts(channel, level);
        }
        Stimulus::AsciDcd { channel, level } if channel < 2 => {
            machine.set_asci_dcd(channel, level);
        }
        Stimulus::Dreq { .. } | Stimulus::AsciCts { .. } | Stimulus::AsciDcd { .. } => {
            return StimulusOutcome::Rejected;
        }
    }
    StimulusOutcome::Applied
}

fn execute_drain<B: HostBus>(machine: &mut Z180<ReplayBus<B>>, output: &Output) -> Drained {
    match *output {
        Output::AsciTx(channel) => Drained::Byte(machine.asci_tx_pop(channel)),
        Output::CsioTx => Drained::Byte(machine.csio_tx_pop()),
        Output::Events => Drained::Events(machine.drain_events()),
        Output::InstructionTrace => Drained::InstructionTrace(machine.drain_insn_trace()),
    }
}

impl<B: HostBus> Timeline<B> {
    pub fn seek(&mut self, target: Position) -> Result<(), TimelineError<B::Error>> {
        if self.mode == Mode::Setup {
            return Err(TimelineError::WrongMode {
                expected: Mode::Live,
                actual: Mode::Setup,
            });
        }
        self.validate_position(target)?;
        self.seek_internal(target)
    }

    pub fn find_first_write(
        &mut self,
        start: Position,
        end: Position,
        base: u32,
        size: u32,
    ) -> Result<Option<WriteHit>, TimelineError<B::Error>> {
        if self.mode == Mode::Setup {
            return Err(TimelineError::WrongMode {
                expected: Mode::Live,
                actual: Mode::Setup,
            });
        }
        self.validate_position(start)?;
        self.validate_position(end)?;
        if start.attempted_steps > end.attempted_steps {
            return Err(TimelineError::InvalidPosition(end));
        }

        let saved_state = self.machine.save_state();
        if saved_state.len() <= 1 {
            return Err(TimelineError::InvalidCheckpoint);
        }
        let saved_position = self.position;
        let saved_mode = self.mode;
        let saved_cursor = self.bus.cursor().map_err(map_control_error)?;

        let probe_result = self.find_first_write_probe(start, end, base, size);
        let restore_result = self
            .machine
            .load_state(&saved_state)
            .map_err(TimelineError::RestoreFailed);
        let mode_result = self
            .bus
            .restore_mode(saved_mode, saved_cursor)
            .map_err(map_control_error);
        self.mode = saved_mode;
        self.position = saved_position;

        restore_result?;
        mode_result?;
        probe_result
    }

    fn find_first_write_probe(
        &mut self,
        start: Position,
        end: Position,
        base: u32,
        size: u32,
    ) -> Result<Option<WriteHit>, TimelineError<B::Error>> {
        self.seek_internal(start)?;
        let _watch = self.machine.add_mem_watch(base, size, WatchKind::Write);
        let _ = self.machine.drain_events();
        self.machine.clear_events_lost();

        while self.position.attempted_steps < end.attempted_steps {
            let attempted_step = self.position.attempted_steps;
            let _outcome = self.replay_next_attempt(true)?;
            if self.machine.events_lost() {
                return Err(TimelineError::EventHistoryLost);
            }
            if let Some(event) = self
                .machine
                .drain_events()
                .into_iter()
                .find(|event| matches!(event, Event::MemWrite { .. }))
            {
                return Ok(Some(WriteHit {
                    attempted_step,
                    event,
                }));
            }
        }
        Ok(None)
    }

    fn seek_internal(&mut self, target: Position) -> Result<(), TimelineError<B::Error>> {
        let checkpoint = self
            .checkpoints
            .iter()
            .rev()
            .find(|checkpoint| {
                checkpoint.position.attempted_steps <= target.attempted_steps
                    && checkpoint.position.actions <= target.actions
            })
            .cloned()
            .ok_or(TimelineError::InvalidPosition(target))?;

        self.machine
            .load_state(&checkpoint.state)
            .map_err(TimelineError::State)?;
        self.bus
            .begin_playback(checkpoint.position.bus_records)
            .map_err(map_control_error)?;
        self.mode = Mode::Playback;
        self.position = checkpoint.position;

        while self.position.attempted_steps < target.attempted_steps {
            let _outcome = self.replay_next_attempt(false)?;
        }
        self.replay_actions_until(target.actions, false)?;
        if self.position != target {
            return Err(TimelineError::InvalidPosition(target));
        }
        Ok(())
    }

    fn validate_position(&self, target: Position) -> Result<(), TimelineError<B::Error>> {
        if target.attempted_steps > self.attempts.len() as u64
            || target.actions > self.actions.len()
        {
            return Err(TimelineError::InvalidPosition(target));
        }

        let expected = if target.attempted_steps == 0 {
            self.checkpoints
                .front()
                .map(|checkpoint| checkpoint.position)
                .ok_or(TimelineError::InvalidPosition(target))?
        } else {
            self.attempts[(target.attempted_steps - 1) as usize].end
        };
        if target.cycle != expected.cycle || target.bus_records != expected.bus_records {
            return Err(TimelineError::InvalidPosition(target));
        }

        if self.actions[..target.actions]
            .iter()
            .any(|action| action.attempted_step > target.attempted_steps)
            || self.actions[target.actions..]
                .first()
                .is_some_and(|action| action.attempted_step < target.attempted_steps)
        {
            return Err(TimelineError::InvalidPosition(target));
        }
        Ok(())
    }

    fn try_step_playback_public(&mut self) -> Result<u32, TimelineError<B::Error>> {
        match self.replay_next_attempt(false)? {
            AttemptOutcome::Success(cycles) => Ok(cycles),
            AttemptOutcome::HostFailure { bus_record } => {
                Err(TimelineError::RecordedHostFailure { record: bus_record })
            }
        }
    }

    fn replay_next_attempt(
        &mut self,
        ignore_event_drain_mismatch: bool,
    ) -> Result<AttemptOutcome, TimelineError<B::Error>> {
        let attempt_index = self.position.attempted_steps as usize;
        let expected = self
            .attempts
            .get(attempt_index)
            .cloned()
            .ok_or(TimelineError::InvalidPosition(self.position))?;
        self.replay_boundary_actions(ignore_event_drain_mismatch)?;

        let actual = self.machine.try_step();
        self.position.attempted_steps = self.position.attempted_steps.saturating_add(1);
        self.position.cycle = self.machine.cycle_count();
        self.position.bus_records = self.bus.cursor().map_err(map_control_error)?;

        let outcome = match (&expected.outcome, actual) {
            (AttemptOutcome::Success(expected_cycles), Ok(actual_cycles))
                if *expected_cycles == actual_cycles =>
            {
                AttemptOutcome::Success(actual_cycles)
            }
            (
                AttemptOutcome::HostFailure {
                    bus_record: expected_record,
                },
                Err(ReplayBusError::RecordedHostFailure {
                    record: actual_record,
                }),
            ) if *expected_record == actual_record => AttemptOutcome::HostFailure {
                bus_record: actual_record,
            },
            (_, Err(error)) => return Err(map_bus_error(error)),
            _ => {
                return Err(TimelineError::BusDivergence {
                    record: self.position.bus_records,
                    expected: None,
                    actual: BusAccess::MemRead { address: 0 },
                });
            }
        };

        if self.position != expected.end {
            return Err(TimelineError::InvalidPosition(expected.end));
        }
        Ok(outcome)
    }

    fn replay_boundary_actions(
        &mut self,
        ignore_event_drain_mismatch: bool,
    ) -> Result<(), TimelineError<B::Error>> {
        let mut limit = self.position.actions;
        while let Some(record) = self.actions.get(limit)
            && record.attempted_step == self.position.attempted_steps
        {
            limit += 1;
        }
        self.replay_actions_until(limit, ignore_event_drain_mismatch)
    }

    fn replay_actions_until(
        &mut self,
        limit: usize,
        ignore_event_drain_mismatch: bool,
    ) -> Result<(), TimelineError<B::Error>> {
        if limit > self.actions.len() {
            return Err(TimelineError::InvalidPosition(self.position));
        }
        while self.position.actions < limit {
            let index = self.position.actions;
            let record = self
                .actions
                .get(index)
                .cloned()
                .ok_or(TimelineError::ActionDivergence { action: index })?;
            if record.attempted_step != self.position.attempted_steps {
                return Err(TimelineError::ActionDivergence { action: index });
            }
            match record.action {
                RecordedAction::Stimulus { stimulus, outcome } => {
                    if execute_stimulus(&mut self.machine, &stimulus) != outcome {
                        return Err(TimelineError::ActionDivergence { action: index });
                    }
                }
                RecordedAction::Drain { output, drained } => {
                    let actual = execute_drain(&mut self.machine, &output);
                    if actual != drained
                        && !(ignore_event_drain_mismatch && output == Output::Events)
                    {
                        return Err(TimelineError::ActionDivergence { action: index });
                    }
                }
            }
            self.position.actions += 1;
        }
        Ok(())
    }
}

fn map_control_error<E>(error: ReplayBusError<core::convert::Infallible>) -> TimelineError<E> {
    match error {
        ReplayBusError::Live(never) => match never {},
        ReplayBusError::RecordedHostFailure { record } => {
            TimelineError::RecordedHostFailure { record }
        }
        ReplayBusError::Divergence {
            record,
            expected,
            actual,
        } => TimelineError::BusDivergence {
            record,
            expected,
            actual,
        },
        ReplayBusError::Borrowed => TimelineError::BusBorrowed,
    }
}

fn map_bus_error<E>(error: ReplayBusError<E>) -> TimelineError<E> {
    match error {
        ReplayBusError::Live(error) => TimelineError::LiveHost(error),
        ReplayBusError::RecordedHostFailure { record } => {
            TimelineError::RecordedHostFailure { record }
        }
        ReplayBusError::Divergence {
            record,
            expected,
            actual,
        } => TimelineError::BusDivergence {
            record,
            expected,
            actual,
        },
        ReplayBusError::Borrowed => TimelineError::BusBorrowed,
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, convert::Infallible, rc::Rc};

    use proptest::prelude::*;
    use z180_core::{Reg, RegionDef, RegionKind};

    use super::*;

    #[derive(Default)]
    struct BusObservations {
        reads: Vec<u16>,
        writes: Vec<(u16, u8)>,
        memory_writes: Vec<(u32, u8)>,
    }

    struct ScriptedBus {
        observations: Rc<RefCell<BusObservations>>,
        read_value: u8,
        fail_memory_write: Option<u32>,
    }

    impl HostBus for ScriptedBus {
        type Error = &'static str;

        fn mem_read(&mut self, _address: u32) -> Result<u8, Self::Error> {
            Ok(self.read_value)
        }

        fn mem_write(&mut self, address: u32, value: u8) -> Result<(), Self::Error> {
            self.observations
                .borrow_mut()
                .memory_writes
                .push((address, value));
            if self.fail_memory_write == Some(address) {
                Err("memory write failed")
            } else {
                Ok(())
            }
        }

        fn io_read(&mut self, port: u16) -> Result<u8, Self::Error> {
            self.observations.borrow_mut().reads.push(port);
            Ok(self.read_value)
        }

        fn io_write(&mut self, port: u16, value: u8) -> Result<(), Self::Error> {
            self.observations.borrow_mut().writes.push((port, value));
            Ok(())
        }
    }

    struct NullBus;

    impl HostBus for NullBus {
        type Error = Infallible;

        fn mem_read(&mut self, _address: u32) -> Result<u8, Self::Error> {
            Ok(0xff)
        }

        fn mem_write(&mut self, _address: u32, _value: u8) -> Result<(), Self::Error> {
            Ok(())
        }

        fn io_read(&mut self, _port: u16) -> Result<u8, Self::Error> {
            Ok(0xff)
        }

        fn io_write(&mut self, _port: u16, _value: u8) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    fn ram_config(size: u32) -> MachineConfig {
        MachineConfig {
            regions: vec![RegionDef {
                base: 0,
                size,
                kind: RegionKind::Ram,
            }],
            event_capacity: 32,
            ..MachineConfig::default()
        }
    }

    fn options(interval: u64) -> Options {
        Options {
            checkpoint_interval_attempts: interval,
            max_checkpoints: 8,
        }
    }

    #[test]
    fn bus_reads_and_writes_replay_without_touching_the_live_bus() {
        let observations = Rc::new(RefCell::new(BusObservations::default()));
        let bus = ScriptedBus {
            observations: Rc::clone(&observations),
            read_value: 0x5a,
            fail_memory_write: None,
        };
        let mut timeline =
            Timeline::new(ram_config(0x1000), bus, options(1)).expect("config is valid");
        timeline
            .setup()
            .expect("setup is available")
            .ram_region_mut(0)
            .expect("RAM exists")[..6]
            .copy_from_slice(&[0xed, 0x38, 0x40, 0xed, 0x39, 0x41]);
        timeline.start().expect("recording starts");
        let initial = timeline.position();

        assert!(timeline.try_step().expect("IN0 executes") > 0);
        assert!(timeline.try_step().expect("OUT0 executes") > 0);
        let final_position = timeline.position();
        let final_state = timeline.machine().save_state();
        assert_eq!(observations.borrow().reads, vec![0x40]);
        assert_eq!(observations.borrow().writes, vec![(0x41, 0x5a)]);

        timeline.seek(initial).expect("initial state is seekable");
        assert_eq!(timeline.mode(), Mode::Playback);
        assert!(timeline.try_step().expect("recorded IN0 replays") > 0);
        assert!(timeline.try_step().expect("recorded OUT0 replays") > 0);
        assert_eq!(timeline.position(), final_position);
        assert_eq!(timeline.machine().save_state(), final_state);
        assert_eq!(observations.borrow().reads, vec![0x40]);
        assert_eq!(
            observations.borrow().writes,
            vec![(0x41, 0x5a)],
            "historical replay suppresses the external write"
        );
    }

    #[test]
    fn internal_io_duplicate_cycles_are_part_of_the_bus_transcript() {
        let observations = Rc::new(RefCell::new(BusObservations::default()));
        let bus = ScriptedBus {
            observations: Rc::clone(&observations),
            read_value: 0xa5,
            fail_memory_write: None,
        };
        let mut timeline =
            Timeline::new(ram_config(0x1000), bus, options(1)).expect("config is valid");
        timeline
            .setup()
            .expect("setup is available")
            .ram_region_mut(0)
            .expect("RAM exists")[..3]
            .copy_from_slice(&[0xed, 0x00, 0x00]);
        timeline.start().expect("recording starts");
        let initial = timeline.position();

        assert!(timeline.try_step().expect("internal IN0 executes") > 0);
        let final_position = timeline.position();
        let final_state = timeline.machine().save_state();
        assert_eq!(final_position.bus_records, 1);
        assert_eq!(observations.borrow().reads, vec![0x0000]);

        timeline.seek(initial).expect("initial state is seekable");
        assert!(timeline.try_step().expect("internal IN0 replays") > 0);
        assert_eq!(timeline.position(), final_position);
        assert_eq!(timeline.machine().save_state(), final_state);
        assert_eq!(
            observations.borrow().reads,
            vec![0x0000],
            "the duplicate cycle is supplied from history"
        );
    }

    #[test]
    fn failed_attempts_preserve_partial_effects_and_replay_at_zero_cycles() {
        let observations = Rc::new(RefCell::new(BusObservations::default()));
        let bus = ScriptedBus {
            observations: Rc::clone(&observations),
            read_value: 0xff,
            fail_memory_write: Some(0x1000),
        };
        let config = MachineConfig {
            regions: vec![
                RegionDef {
                    base: 0,
                    size: 0x1000,
                    kind: RegionKind::Ram,
                },
                RegionDef {
                    base: 0x1000,
                    size: 0x1000,
                    kind: RegionKind::External,
                },
            ],
            ..MachineConfig::default()
        };
        let mut timeline = Timeline::new(config, bus, options(1)).expect("config is valid");
        let machine = timeline.setup().expect("setup is available");
        machine.ram_region_mut(0).expect("RAM exists")[..3].copy_from_slice(&[0x22, 0xff, 0x0f]);
        machine.set_reg(Reg::HL, 0x1234);
        timeline.start().expect("recording starts");
        let initial = timeline.position();

        assert!(matches!(
            timeline.try_step(),
            Err(TimelineError::LiveHost("memory write failed"))
        ));
        assert_eq!(timeline.position().attempted_steps, 1);
        assert_eq!(timeline.position().cycle, 0);
        assert_eq!(timeline.machine().mem_peek(0x0fff), 0x34);
        let failed_state = timeline.machine().save_state();
        assert_eq!(observations.borrow().memory_writes, vec![(0x1000, 0x12)]);

        timeline
            .seek(initial)
            .expect("pre-failure state is seekable");
        assert!(matches!(
            timeline.try_step(),
            Err(TimelineError::RecordedHostFailure { record: 0 })
        ));
        assert_eq!(timeline.position().attempted_steps, 1);
        assert_eq!(timeline.position().cycle, 0);
        assert_eq!(timeline.machine().mem_peek(0x0fff), 0x34);
        assert_eq!(timeline.machine().save_state(), failed_state);
        assert_eq!(
            observations.borrow().memory_writes,
            vec![(0x1000, 0x12)],
            "replaying the failed write does not call the live device"
        );
    }

    #[test]
    fn stimuli_and_output_actions_replay_at_their_attempt_boundaries() {
        let mut timeline =
            Timeline::new(ram_config(0x1000), NullBus, options(1)).expect("config is valid");
        timeline.start().expect("recording starts");
        let initial = timeline.position();
        assert_eq!(
            timeline
                .apply(Stimulus::Dreq {
                    channel: 7,
                    level: true,
                })
                .expect("stimulus is recorded"),
            StimulusOutcome::Rejected
        );
        assert_eq!(
            timeline
                .apply(Stimulus::Irq {
                    line: IrqLine::Int2,
                    level: true,
                })
                .expect("stimulus is recorded"),
            StimulusOutcome::Applied
        );
        assert!(timeline.try_step().expect("step executes") > 0);
        assert_eq!(
            timeline.drain(Output::CsioTx).expect("drain is recorded"),
            Drained::Byte(None)
        );
        let final_position = timeline.position();
        let final_state = timeline.machine().save_state();

        timeline.seek(initial).expect("initial state is seekable");
        assert!(timeline.try_step().expect("stimuli replay before step") > 0);
        timeline
            .seek(final_position)
            .expect("post-drain position is seekable");
        assert_eq!(timeline.machine().save_state(), final_state);
    }

    #[test]
    fn first_write_probe_restores_live_state_and_history() {
        let mut timeline =
            Timeline::new(ram_config(0x2000), NullBus, options(1)).expect("config is valid");
        timeline
            .setup()
            .expect("setup is available")
            .ram_region_mut(0)
            .expect("RAM exists")[..7]
            .copy_from_slice(&[0x3e, 0x5a, 0x32, 0x34, 0x12, 0x00, 0x00]);
        timeline.start().expect("recording starts");
        let initial = timeline.position();
        for _ in 0..3 {
            assert!(timeline.try_step().expect("program executes") > 0);
        }
        let end = timeline.position();
        let saved = timeline.machine().save_state();

        let hit = timeline
            .find_first_write(initial, end, 0x1234, 1)
            .expect("probe completes")
            .expect("the store is found");
        assert_eq!(hit.attempted_step, 1);
        assert!(matches!(
            hit.event,
            Event::MemWrite {
                phys: 0x1234,
                val: 0x5a,
                ..
            }
        ));
        assert_eq!(timeline.mode(), Mode::Live);
        assert_eq!(timeline.position(), end);
        assert_eq!(timeline.machine().save_state(), saved);
        assert!(timeline.try_step().expect("live recording can continue") > 0);
    }

    #[test]
    fn first_write_probe_reports_event_loss_instead_of_a_false_negative() {
        let mut config = ram_config(0x2000);
        config.event_capacity = 0;
        let mut timeline = Timeline::new(config, NullBus, options(1)).expect("config is valid");
        timeline
            .setup()
            .expect("setup is available")
            .ram_region_mut(0)
            .expect("RAM exists")[..5]
            .copy_from_slice(&[0x3e, 0x5a, 0x32, 0x34, 0x12]);
        timeline.start().expect("recording starts");
        let initial = timeline.position();
        assert!(timeline.try_step().expect("load executes") > 0);
        assert!(timeline.try_step().expect("store executes") > 0);
        let end = timeline.position();
        let saved = timeline.machine().save_state();

        assert!(matches!(
            timeline.find_first_write(initial, end, 0x1234, 1),
            Err(TimelineError::EventHistoryLost)
        ));
        assert_eq!(timeline.mode(), Mode::Live);
        assert_eq!(timeline.position(), end);
        assert_eq!(timeline.machine().save_state(), saved);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn replay_matches_live_state_for_generated_programs(values in prop::collection::vec(any::<u8>(), 1..20)) {
            let mut timeline =
                Timeline::new(ram_config(0x2000), NullBus, options(7)).expect("config is valid");
            let mut program = Vec::new();
            for (index, value) in values.iter().copied().enumerate() {
                let address = 0x1000_u16 + index as u16;
                program.extend_from_slice(&[
                    0x3e,
                    value,
                    0x3c,
                    0x32,
                    address as u8,
                    (address >> 8) as u8,
                    0x00,
                ]);
            }
            timeline
                .setup()
                .expect("setup is available")
                .ram_region_mut(0)
                .expect("RAM exists")[..program.len()]
                .copy_from_slice(&program);
            timeline.start().expect("recording starts");
            let initial = timeline.position();
            let attempts = values.len() * 4;
            for _ in 0..attempts {
                prop_assert!(timeline.try_step().expect("live step executes") > 0);
            }
            let final_position = timeline.position();
            let final_state = timeline.machine().save_state();

            timeline.seek(initial).expect("initial state is seekable");
            for _ in 0..attempts {
                prop_assert!(timeline.try_step().expect("playback step executes") > 0);
            }
            prop_assert_eq!(timeline.position(), final_position);
            prop_assert_eq!(timeline.machine().save_state(), final_state);
        }
    }
}
