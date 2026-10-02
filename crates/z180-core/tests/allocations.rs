//! Guard the transient heap cost of constructing and saving embedded machines.

use core::convert::Infallible;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use z180_core::{HostBus, MachineConfig, RegionDef, RegionKind, Z180};

#[derive(Clone, Copy, Default)]
struct AllocationWindow {
    active: bool,
    live: isize,
    peak: usize,
}

thread_local! {
    static WINDOW: Cell<AllocationWindow> = const { Cell::new(AllocationWindow {
        active: false, live: 0, peak: 0,
    }) };
}

fn record(delta: isize) {
    let _ = WINDOW.try_with(|window| {
        let mut current = window.get();
        if current.active {
            current.live += delta;
            current.peak = current.peak.max(current.live.max(0) as usize);
            window.set(current);
        }
    });
}

struct TrackingAllocator;

// SAFETY: Every allocation operation delegates to System with the original
// pointer and layout. Tracking only uses allocation-free thread-local Cells.
unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: The caller supplies a valid allocation layout.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            record(layout.size() as isize);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        record(-(layout.size() as isize));
        // SAFETY: The caller supplies the original allocation and layout.
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: The caller supplies a live allocation and valid new size.
        let result = unsafe { System.realloc(pointer, layout, size) };
        if !result.is_null() {
            record(size as isize - layout.size() as isize);
        }
        result
    }
}

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn measure<T>(operation: impl FnOnce() -> T) -> (T, usize) {
    struct StopTracking;
    impl Drop for StopTracking {
        fn drop(&mut self) {
            WINDOW.with(|window| {
                let mut current = window.get();
                current.active = false;
                window.set(current);
            });
        }
    }
    WINDOW.with(|window| {
        assert!(!window.get().active);
        window.set(AllocationWindow {
            active: true,
            ..AllocationWindow::default()
        });
    });
    let guard = StopTracking;
    let result = operation();
    let peak = WINDOW.with(|window| window.get().peak);
    drop(guard);
    (result, peak)
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
