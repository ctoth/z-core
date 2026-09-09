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
fn recording_round_trip_rewinds_and_rejects_forged_checkpoint() {
    let mut timeline = Timeline::new(ram_config(0x1000), NullBus, options(2)).unwrap();
    timeline.setup().unwrap().mem_poke(0, 0x3c);
    timeline.start().unwrap();
    for _ in 0..20 {
        timeline.try_step().unwrap();
    }
    let expected = timeline.machine().save_state();
    let data = timeline.export_recording().unwrap();
    let mut restored = Timeline::import_recording(&data, NullBus).unwrap();
    assert_eq!(restored.mode(), Mode::Playback);
    assert_eq!(restored.machine().save_state(), expected);
    restored
        .seek(restored.position_at_attempt(0).unwrap())
        .unwrap();
    for _ in 0..20 {
        restored.try_step().unwrap();
    }
    assert_eq!(restored.machine().save_state(), expected);
    assert!(restored.try_step().is_err());
    let mut forged: serde_json::Value = serde_json::from_slice(&data).unwrap();
    forged["checkpoints"][1]["state"][0] = 255.into();
    assert!(Timeline::import_recording(&serde_json::to_vec(&forged).unwrap(), NullBus).is_err());
}

#[test]
fn write_probe_filters_preexisting_watches() {
    let mut timeline = Timeline::new(ram_config(0x2000), NullBus, options(2)).unwrap();
    for (address, byte) in [0x32, 0x00, 0x10].into_iter().enumerate() {
        timeline.setup().unwrap().mem_poke(address as u32, byte);
    }
    timeline
        .setup()
        .unwrap()
        .add_mem_watch(0, 0x2000, WatchKind::Write);
    timeline.start().unwrap();
    let start = timeline.position();
    timeline.try_step().unwrap();
    assert!(
        timeline
            .find_first_write(start, timeline.position(), 0x1001, 1)
            .unwrap()
            .is_none()
    );
    assert!(
        timeline
            .find_first_write(start, timeline.position(), 0x1000, 1)
            .unwrap()
            .is_some()
    );
}

#[test]
fn retention_stops_before_mutation_and_export_remains_available() {
    let mut timeline = Timeline::new(ram_config(0x1000), NullBus, options(2)).unwrap();
    timeline.start().unwrap();
    timeline.set_byte_limit(Some(timeline.retained_bytes()));
    let before = timeline.position();
    assert!(matches!(
        timeline.try_step(),
        Err(TimelineError::RetentionLimit { .. })
    ));
    assert_eq!(timeline.position(), before);
    assert!(timeline.export_recording().is_ok());
    timeline.set_byte_limit(None);
    timeline.try_step().unwrap();
}

#[test]
fn bus_reads_and_writes_replay_without_touching_the_live_bus() {
    let observations = Rc::new(RefCell::new(BusObservations::default()));
    let bus = ScriptedBus {
        observations: Rc::clone(&observations),
        read_value: 0x5a,
        fail_memory_write: None,
    };
    let mut timeline = Timeline::new(ram_config(0x1000), bus, options(1)).expect("config is valid");
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
    let mut timeline = Timeline::new(ram_config(0x1000), bus, options(1)).expect("config is valid");
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
