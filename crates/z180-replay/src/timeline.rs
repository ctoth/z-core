use super::*;

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
            action_heap_bytes: 0,
            attempts: Vec::new(),
            checkpoints: VecDeque::new(),
            byte_limit: None,
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
        self.action_heap_bytes = 0;
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
        self.check_retention()?;
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
        self.check_retention()?;
        let drained = execute_drain(&mut self.machine, &output);
        self.action_heap_bytes += match &drained {
            Drained::Events(events) => events.len() * size_of::<Event>(),
            Drained::InstructionTrace(trace) => trace.len() * size_of::<TraceEntry>(),
            Drained::Byte(_) => 0,
        };
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
        self.check_retention()?;
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
            // Preserve coverage across history, not only the initial and newest states.
            let index = (1..self.checkpoints.len())
                .min_by_key(|&i| {
                    let next = self
                        .checkpoints
                        .get(i + 1)
                        .map_or(self.position.attempted_steps, |cp| {
                            cp.position.attempted_steps
                        });
                    next - self.checkpoints[i - 1].position.attempted_steps
                })
                .expect("at least two checkpoints");
            let _ = self.checkpoints.remove(index);
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
        drop(self.machine.drain_events_iter());
        self.machine.clear_events_lost();

        while self.position.attempted_steps < end.attempted_steps {
            let attempted_step = self.position.attempted_steps;
            let _outcome = self.replay_next_attempt(true)?;
            if self.machine.events_lost() {
                return Err(TimelineError::EventHistoryLost);
            }
            if let Some(event) = self
                .machine
                .drain_events_iter()
                .find(|event| matches!(event, Event::MemWrite { phys, .. } if *phys >= base && u64::from(*phys) < u64::from(base) + u64::from(size)))
            {
                return Ok(Some(WriteHit {
                    attempted_step,
                    event,
                }));
            }
        }
        Ok(None)
    }

    pub(super) fn seek_internal(
        &mut self,
        target: Position,
    ) -> Result<(), TimelineError<B::Error>> {
        let checkpoint = self
            .checkpoints
            .iter()
            .rev()
            .find(|checkpoint| {
                checkpoint.position.attempted_steps <= target.attempted_steps
                    && checkpoint.position.actions <= target.actions
            })
            .ok_or(TimelineError::InvalidPosition(target))?;

        self.machine
            .load_state(&checkpoint.state)
            .map_err(TimelineError::State)?;
        let position = checkpoint.position;
        self.bus
            .begin_playback(position.bus_records)
            .map_err(map_control_error)?;
        self.mode = Mode::Playback;
        self.position = position;

        while self.position.attempted_steps < target.attempted_steps {
            let _outcome = self.replay_next_attempt(false)?;
        }
        self.replay_actions_until(target.actions, false)?;
        if self.position != target {
            return Err(TimelineError::InvalidPosition(target));
        }
        Ok(())
    }

    pub(super) fn validate_position(
        &self,
        target: Position,
    ) -> Result<(), TimelineError<B::Error>> {
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

    pub(super) fn replay_next_attempt(
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

    pub(super) fn replay_actions_until(
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
                .ok_or(TimelineError::ActionDivergence { action: index })?;
            if record.attempted_step != self.position.attempted_steps {
                return Err(TimelineError::ActionDivergence { action: index });
            }
            match &record.action {
                RecordedAction::Stimulus { stimulus, outcome } => {
                    if execute_stimulus(&mut self.machine, stimulus) != *outcome {
                        return Err(TimelineError::ActionDivergence { action: index });
                    }
                }
                RecordedAction::Drain { output, drained } => {
                    let matches = match (output, drained) {
                        (Output::Events, Drained::Events(expected)) => self
                            .machine
                            .drain_events_iter()
                            .eq(expected.iter().cloned()),
                        (Output::InstructionTrace, Drained::InstructionTrace(expected)) => self
                            .machine
                            .drain_insn_trace_iter()
                            .eq(expected.iter().cloned()),
                        _ => &execute_drain(&mut self.machine, output) == drained,
                    };
                    if !(matches || ignore_event_drain_mismatch && *output == Output::Events) {
                        return Err(TimelineError::ActionDivergence { action: index });
                    }
                }
            }
            self.position.actions += 1;
        }
        Ok(())
    }
}

pub(super) fn map_control_error<E>(
    error: ReplayBusError<core::convert::Infallible>,
) -> TimelineError<E> {
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

pub(super) fn map_bus_error<E>(error: ReplayBusError<E>) -> TimelineError<E> {
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
