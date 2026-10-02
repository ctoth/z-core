use super::*;

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
    cpu.bus.read_value = 0x5a;
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
    assert_eq!(cpu.bus.reads, vec![0x0040, CNTLA0 as u16, 0x0042]);

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
    let mut original = Z180::new(config, NullBus).expect("debug-state configuration must be valid");
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
            0x1_4000, 0x1_4001, 0x1_4002, 0x1_4003, 0x1_4004, 0x1_4005, 0x1_5001, 0x1_4006,
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
fn partial_iterator_drains_discard_the_tail_and_keep_ring_storage() {
    let mut cpu = machine();
    cpu.event_capacity = 3;
    cpu.events = VecDeque::with_capacity(3);
    cpu.set_insn_trace(Some(3));
    cpu.add_mem_watch(0, 16, WatchKind::Read);
    for _ in 0..5 {
        cpu.step();
    }
    let expected_events = cpu.events.clone();
    let expected_trace = cpu.insn_trace.clone();
    let event_capacity = cpu.events.capacity();
    let trace_capacity = cpu.insn_trace.capacity();
    assert!(cpu.events_lost());
    {
        let mut events = cpu.drain_events_iter();
        assert_eq!(events.len(), 3);
        assert_eq!(events.next(), expected_events.front().cloned());
        assert_eq!(events.next_back(), expected_events.back().cloned());
    }
    {
        let mut trace = cpu.drain_insn_trace_iter();
        assert_eq!(trace.len(), 3);
        assert_eq!(trace.next(), expected_trace.front().cloned());
    }
    assert!(cpu.events.is_empty());
    assert!(cpu.insn_trace.is_empty());
    assert_eq!(cpu.events.capacity(), event_capacity);
    assert_eq!(cpu.insn_trace.capacity(), trace_capacity);
    assert!(cpu.events_lost());
    cpu.step();
    assert_eq!(cpu.drain_insn_trace_iter().count(), 1);
}
