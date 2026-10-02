#[path = "../../../tests/support/heap.rs"]
mod heap;
use heap::{allocation_calls, measure};
use std::convert::Infallible;
use z180_core::{HostBus, MachineConfig, RegionDef, RegionKind, Z180};
use z180_replay::{Options, Output, Timeline};

struct NullBus;
impl HostBus for NullBus {
    type Error = Infallible;
    fn mem_read(&mut self, _: u32) -> Result<u8, Self::Error> {
        Ok(0xff)
    }
    fn mem_write(&mut self, _: u32, _: u8) -> Result<(), Self::Error> {
        Ok(())
    }
    fn io_read(&mut self, _: u16) -> Result<u8, Self::Error> {
        Ok(0xff)
    }
    fn io_write(&mut self, _: u16, _: u8) -> Result<(), Self::Error> {
        Ok(())
    }
}
fn timeline(event_capacity: usize) -> Timeline<NullBus> {
    Timeline::new(
        MachineConfig {
            event_capacity,
            regions: vec![RegionDef {
                base: 0,
                size: 64 * 1024,
                kind: RegionKind::Ram,
            }],
            ..MachineConfig::default()
        },
        NullBus,
        Options {
            checkpoint_interval_attempts: 10000,
            max_checkpoints: 8,
        },
    )
    .unwrap()
}

#[test]
fn export_borrows_history_and_allocates_only_the_json_output() {
    let mut timeline = timeline(0);
    timeline.start().unwrap();
    for _ in 0..8 {
        timeline.try_step().unwrap();
    }
    let (exported, peak) = measure(|| timeline.export_recording().unwrap());
    println!(
        "export: {peak} extra bytes; output capacity {}",
        exported.capacity()
    );
    assert!(
        peak <= exported.capacity() + 4096,
        "export copied history: {peak} bytes for {} output bytes",
        exported.capacity()
    );
    let imported = Timeline::import_recording(&exported, NullBus).unwrap();
    assert_eq!(imported.export_recording().unwrap(), exported);
}

#[test]
fn seek_borrows_the_checkpoint_state() {
    let mut timeline = timeline(0);
    timeline.start().unwrap();
    let start = timeline.position();
    let saved = timeline.machine().save_state();
    let mut machine = Z180::new(MachineConfig::default(), NullBus).unwrap();
    let (_, load_peak) = measure(|| machine.load_state(&saved).unwrap());
    let load_calls = allocation_calls();
    timeline.try_step().unwrap();
    let (_, seek_peak) = measure(|| timeline.seek(start).unwrap());
    let seek_calls = allocation_calls();
    println!("load peak {load_peak}; seek peak {seek_peak}; calls {load_calls}/{seek_calls}");
    assert_eq!(seek_calls, load_calls, "seek allocated a checkpoint copy");
    assert_eq!(seek_peak, load_peak);
    assert_eq!(timeline.machine().save_state(), saved);
}

#[test]
fn playback_drain_does_not_clone_the_expected_batch() {
    for output in [Output::Events, Output::InstructionTrace] {
        let mut timeline = timeline(16);
        timeline.setup().unwrap().set_insn_trace(Some(8));
        timeline
            .setup()
            .unwrap()
            .add_mem_watch(0, 8, z180_core::WatchKind::Read);
        timeline.start().unwrap();
        let start = timeline.position();
        let snapshot = timeline.machine().save_state();
        for _ in 0..8 {
            timeline.try_step().unwrap();
        }
        let expected = timeline.drain(output).unwrap();
        let end = timeline.position();
        timeline.seek(start).unwrap();
        for _ in 0..8 {
            timeline.try_step().unwrap();
        }
        let before_drain = timeline.position();
        let mut machine = Z180::new(MachineConfig::default(), NullBus).unwrap();
        machine.load_state(&snapshot).unwrap();
        let (_, load_peak) = measure(|| machine.load_state(&snapshot).unwrap());
        let load_calls = allocation_calls();
        let (_, peak) = measure(|| timeline.seek(end).unwrap());
        let calls = allocation_calls();
        println!("drain replay extra peak {peak}, calls {calls}; load {load_peak}/{load_calls}");
        // Seeking the initial checkpoint necessarily reloads the machine and its
        // trace capacity. Playback compares both batches directly without allocating.
        assert_eq!(calls, load_calls, "playback drain allocated a batch");
        assert_eq!(timeline.position(), end);
        assert!(match expected {
            z180_replay::Drained::Events(entries) => entries.len() == 8,
            z180_replay::Drained::InstructionTrace(entries) => entries.len() == 8,
            z180_replay::Drained::Byte(_) => false,
        });
        assert!(before_drain.actions < end.actions);
    }
}
