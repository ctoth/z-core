#![forbid(unsafe_code)]

use std::{cell::RefCell, collections::VecDeque, rc::Rc};

mod bus;
mod recording;
mod timeline;
use timeline::{map_bus_error, map_control_error};

use z180_core::{
    ConfigError, Event, HostBus, IrqLine, MachineConfig, StateError, TraceEntry, WatchKind, Z180,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Setup,
    Live,
    Playback,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
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

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Position {
    pub attempted_steps: u64,
    pub cycle: u64,
    pub actions: usize,
    pub bus_records: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
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
    RetentionLimit {
        bytes: usize,
        limit: usize,
    },
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
            Self::RetentionLimit { bytes, limit } => write!(
                formatter,
                "recording uses {bytes} bytes; retention limit is {limit}; export before continuing"
            ),
            Self::RestoreFailed(error) => {
                write!(formatter, "failed to restore state after probe: {error}")
            }
        }
    }
}

impl<E: core::error::Error + 'static> core::error::Error for TimelineError<E> {}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Stimulus {
    Irq { line: IrqLine, level: bool },
    Nmi(bool),
    Dreq { channel: usize, level: bool },
    AsciRx { channel: usize, byte: u8 },
    CsioRx(u8),
    AsciCts { channel: usize, level: bool },
    AsciDcd { channel: usize, level: bool },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum StimulusOutcome {
    Applied,
    Rejected,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Output {
    AsciTx(usize),
    CsioTx,
    Events,
    InstructionTrace,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Drained {
    Byte(Option<u8>),
    Events(Vec<Event>),
    InstructionTrace(Vec<TraceEntry>),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
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

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct ActionRecord {
    attempted_step: u64,
    action: RecordedAction,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
enum AttemptOutcome {
    Success(u32),
    HostFailure { bus_record: usize },
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct AttemptRecord {
    outcome: AttemptOutcome,
    end: Position,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
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
    action_heap_bytes: usize,
    attempts: Vec<AttemptRecord>,
    checkpoints: VecDeque<Checkpoint>,
    byte_limit: Option<usize>,
}

#[cfg(test)]
mod tests;
