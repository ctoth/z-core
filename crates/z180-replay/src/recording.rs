use super::*;
use std::borrow::Cow;

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Recording<'a> {
    format: Cow<'a, str>,
    version: u32,
    options: Options,
    position: Position,
    records: Cow<'a, [BusRecord]>,
    actions: Cow<'a, [ActionRecord]>,
    attempts: Cow<'a, [AttemptRecord]>,
    checkpoints: Cow<'a, VecDeque<Checkpoint>>,
}

impl<B: HostBus> Timeline<B> {
    /// Stop before the next host action or attempted step once retained heap
    /// storage reaches this threshold. One operation can exceed it; history is
    /// never silently discarded. None explicitly selects unlimited retention.
    pub fn set_byte_limit(&mut self, limit: Option<usize>) {
        self.byte_limit = limit;
    }

    /// Retained collection allocations, excluding the live core/host, allocator
    /// overhead, and temporary export/probe allocations.
    pub fn retained_bytes(&self) -> usize {
        let Ok(bus) = self.bus.shared.try_borrow() else {
            return usize::MAX;
        };

        bus.records.capacity() * size_of::<BusRecord>()
            + self.actions.capacity() * size_of::<ActionRecord>()
            + self.attempts.capacity() * size_of::<AttemptRecord>()
            + self.checkpoints.capacity() * size_of::<Checkpoint>()
            + self
                .checkpoints
                .iter()
                .map(|cp| cp.state.capacity())
                .sum::<usize>()
            + self.action_heap_bytes
    }

    pub(super) fn check_retention(&self) -> Result<(), TimelineError<B::Error>> {
        if let Some(limit) = self.byte_limit {
            let bytes = self.retained_bytes();
            if bytes >= limit {
                return Err(TimelineError::RetentionLimit { bytes, limit });
            }
        }
        Ok(())
    }

    /// Canonical position immediately after an attempted step (before subsequent
    /// host actions). Attempt zero is the initial checkpoint.
    pub fn position_at_attempt(&self, attempt: u64) -> Option<Position> {
        if attempt == 0 {
            return self.oldest_position();
        }
        usize::try_from(attempt - 1)
            .ok()
            .and_then(|i| self.attempts.get(i))
            .map(|record| record.end)
    }

    pub fn recorded_end(&self) -> Option<Position> {
        let mut end = self
            .attempts
            .last()
            .map_or_else(|| self.oldest_position(), |a| Some(a.end))?;
        end.actions = self.actions.len();
        Some(end)
    }

    /// Versioned, self-contained bus/input journal and indexed core checkpoints.
    /// Host wiring (including an external address mapper) is not serialized.
    pub fn export_recording(&self) -> Result<Vec<u8>, String> {
        if self.mode == Mode::Setup {
            return Err("start recording before exporting".into());
        }
        let bus = self
            .bus
            .shared
            .try_borrow()
            .map_err(|_| "bus is borrowed")?;
        let recording = Recording {
            format: Cow::Borrowed("z180-replay"),
            version: 1,
            options: self.options,
            position: self.position,
            records: Cow::Borrowed(&bus.records),
            actions: Cow::Borrowed(&self.actions),
            attempts: Cow::Borrowed(&self.attempts),
            checkpoints: Cow::Borrowed(&self.checkpoints),
        };
        serde_json::to_vec(&recording).map_err(|error| error.to_string())
    }

    /// Import into a fresh machine in playback mode. Never invokes the live bus.
    /// Archives requiring an external mapper are rejected by replay validation;
    /// use in-process timelines for hosts with custom mapper wiring.
    pub fn import_recording(data: &[u8], bus: B) -> Result<Self, String> {
        let recording: Recording = serde_json::from_slice(data).map_err(|e| e.to_string())?;
        if recording.format != "z180-replay" || recording.version != 1 {
            return Err("unsupported replay format/version".into());
        }
        if recording.checkpoints.is_empty()
            || recording.checkpoints.len() > recording.options.max_checkpoints
        {
            return Err("invalid checkpoint count".into());
        }
        let mut timeline = Self::new(MachineConfig::default(), bus, recording.options)
            .map_err(|e| e.to_string())?;
        timeline.actions = recording.actions.into_owned();
        timeline.action_heap_bytes = timeline
            .actions
            .iter()
            .map(|record| match &record.action {
                RecordedAction::Drain {
                    drained: Drained::Events(events),
                    ..
                } => events.capacity() * size_of::<Event>(),
                RecordedAction::Drain {
                    drained: Drained::InstructionTrace(trace),
                    ..
                } => trace.capacity() * size_of::<TraceEntry>(),
                _ => 0,
            })
            .sum();
        timeline.attempts = recording.attempts.into_owned();
        timeline.checkpoints = recording.checkpoints.into_owned();
        timeline.bus.shared.borrow_mut().records = recording.records.into_owned();
        let start = timeline.checkpoints[0].position;
        if start.attempted_steps != 0 || start.actions != 0 || start.bus_records != 0 {
            return Err("missing initial checkpoint".into());
        }
        let mut previous = start;
        for (i, record) in timeline.attempts.iter().enumerate() {
            if record.end.attempted_steps != i as u64 + 1
                || record.end.actions < previous.actions
                || record.end.actions > timeline.actions.len()
                || record.end.bus_records < previous.bus_records
                || record.end.bus_records > timeline.bus.shared.borrow().records.len()
                || record.end.cycle < previous.cycle
            {
                return Err("invalid attempt index".into());
            }
            previous = record.end;
        }
        if timeline
            .actions
            .windows(2)
            .any(|pair| pair[0].attempted_step > pair[1].attempted_step)
            || timeline
                .actions
                .last()
                .is_some_and(|a| a.attempted_step > timeline.attempts.len() as u64)
        {
            return Err("invalid action index".into());
        }
        if timeline
            .checkpoints
            .iter()
            .zip(timeline.checkpoints.iter().skip(1))
            .any(|(a, b)| a.position.attempted_steps >= b.position.attempted_steps)
        {
            return Err("unordered checkpoints".into());
        }
        for cp in &timeline.checkpoints {
            if timeline.position_at_attempt(cp.position.attempted_steps) != Some(cp.position) {
                return Err("checkpoint is not at an attempt boundary".into());
            }
            timeline
                .validate_position(cp.position)
                .map_err(|_| "invalid checkpoint position")?;
            timeline
                .machine
                .load_state(&cp.state)
                .map_err(|e| e.to_string())?;
        }
        timeline
            .validate_position(recording.position)
            .map_err(|_| "invalid saved position")?;
        timeline
            .seek_internal(start)
            .map_err(|_| "invalid initial state")?;
        if timeline.machine.cycle_count() != start.cycle {
            return Err("initial cycle differs".into());
        }
        // Validate transitions and checkpoint contents, not just JSON shape. A
        // forged checkpoint must not be able to bypass deterministic replay.
        let end = timeline.recorded_end().ok_or("missing end")?;
        while timeline.position.attempted_steps < end.attempted_steps {
            timeline
                .replay_next_attempt(false)
                .map_err(|_| "recording diverges during validation")?;
            if let Some(cp) = timeline
                .checkpoints
                .iter()
                .find(|cp| cp.position == timeline.position)
                && cp.state != timeline.machine.save_state()
            {
                return Err("checkpoint disagrees with journal".into());
            }
        }
        timeline
            .replay_actions_until(end.actions, false)
            .map_err(|_| "final actions diverge")?;
        if timeline.bus.cursor().map_err(|_| "bus borrowed")?
            != timeline.bus.shared.borrow().records.len()
        {
            return Err("unused bus records".into());
        }
        timeline
            .seek_internal(recording.position)
            .map_err(|_| "saved position diverges")?;
        Ok(timeline)
    }
}
