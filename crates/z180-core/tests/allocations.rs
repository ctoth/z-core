//! Guard the transient heap cost of constructing and saving embedded machines.

use core::convert::Infallible;
#[path = "../../../tests/support/heap.rs"]
mod heap;
use heap::{allocation_calls, measure};
use z180_core::{HostBus, MachineConfig, RegionDef, RegionKind, Z180};

#[test]
fn disassembly_allocates_only_its_result_string() {
    for (bytes, expected) in [
        (&[0x00][..], "NOP"),
        (&[0xfd, 0xcb, 0xfe, 0x46][..], "BIT 0,(IY-02h)"),
    ] {
        let (instruction, _) = measure(|| z180_core::disassemble_one(bytes, 0).unwrap());
        let calls = allocation_calls();
        assert_eq!(instruction.text, expected);
        assert_eq!(calls, 1, "disassembly allocated or grew {calls} buffers");
    }
}

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

#[test]
fn construction_transfers_rom_without_a_second_image_allocation() {
    let config = MachineConfig {
        event_capacity: 0,
        regions: vec![RegionDef {
            base: 0,
            size: 64 * 1024,
            kind: RegionKind::Rom(vec![0x5a; 64 * 1024]),
        }],
        ..MachineConfig::default()
    };
    let (machine, peak) = measure(|| Z180::new(config, NullBus).unwrap());
    println!("64 KiB ROM construction: {peak} extra heap bytes");
    assert_eq!(machine.mem_peek(0), 0x5a);
    assert_eq!(machine.mem_peek(65535), 0x5a);
    assert!(
        peak < 16 * 1024,
        "construction allocated {peak} extra bytes for an already-owned ROM"
    );
}

#[cfg(feature = "state")]
#[test]
fn saving_allocates_only_the_output_buffer() {
    let config = MachineConfig {
        event_capacity: 0,
        regions: vec![
            RegionDef {
                base: 0,
                size: 64 * 1024,
                kind: RegionKind::Ram,
            },
            RegionDef {
                base: 64 * 1024,
                size: 16 * 1024,
                kind: RegionKind::Rom(vec![0xa5; 16 * 1024]),
            },
        ],
        ..MachineConfig::default()
    };
    let mut machine = Z180::new(config, NullBus).unwrap();
    machine.mem_poke(0x1234, 0x5a);
    let (saved, peak) = measure(|| machine.save_state());
    println!(
        "80 KiB guest save: {peak} extra heap bytes, output length={}, capacity={}",
        saved.len(),
        saved.capacity(),
    );
    assert!(
        peak <= saved.capacity() + 4096,
        "saving used {peak} bytes beyond the live machine for an output buffer of {} bytes",
        saved.capacity(),
    );
    assert_eq!(machine.mem_peek(0x1234), 0x5a, "saving preserves RAM");
    let mut resumed = Z180::new(MachineConfig::default(), NullBus).unwrap();
    resumed.load_state(&saved).unwrap();
    assert_eq!(resumed.mem_peek(0x1234), 0x5a);
    assert_eq!(resumed.mem_peek(64 * 1024), 0xa5);
    assert_eq!(resumed.save_state(), saved);
}

#[cfg(feature = "state")]
#[test]
fn caller_owned_save_buffers_do_not_allocate_when_sized() {
    let machine = Z180::new(MachineConfig::default(), NullBus).unwrap();
    let expected = machine.save_state();
    let mut buffer = Vec::with_capacity(expected.len());
    let (result, peak) = measure(|| machine.save_state_into(&mut buffer));
    result.unwrap();
    assert_eq!(allocation_calls(), 0);
    assert_eq!(peak, 0);
    assert_eq!(buffer, expected);
    let mut slice = vec![0; expected.len()];
    let (saved, peak) = measure(|| machine.save_state_to_slice(&mut slice).unwrap());
    assert_eq!(allocation_calls(), 0);
    assert_eq!(peak, 0);
    assert_eq!(saved, expected);
}

#[test]
fn iterator_drains_do_not_allocate() {
    let mut machine = Z180::new(MachineConfig::default(), NullBus).unwrap();
    machine.set_insn_trace(Some(8));
    machine.add_mem_watch(0, 1, z180_core::WatchKind::Read);
    machine.step();
    let (counts, peak) = measure(|| {
        (
            machine.drain_events_iter().count(),
            machine.drain_insn_trace_iter().count(),
        )
    });
    assert_eq!(counts, (1, 1));
    assert_eq!(allocation_calls(), 0);
    assert_eq!(peak, 0);
}

#[test]
fn all_opcode_pages_format_with_one_result_allocation() {
    for opcode in 0..=255 {
        for bytes in [
            [opcode, 0x80, 0x12, 0],
            [0xcb, opcode, 0, 0],
            [0xed, opcode, 0x34, 0x12],
            [0xdd, opcode, 0x80, 0x12],
            [0xfd, opcode, 0x7f, 0x12],
            [0xdd, 0xcb, 0x80, opcode],
            [0xfd, 0xcb, 0x7f, opcode],
        ] {
            let (instruction, _) = measure(|| z180_core::disassemble_one(&bytes, 0xffff).unwrap());
            assert_eq!(allocation_calls(), 1, "{bytes:02x?}: {}", instruction.text);
        }
    }
}
