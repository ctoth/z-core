use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

#[derive(Clone, Copy, Default)]
struct AllocationWindow {
    active: bool,
    live: isize,
    peak: usize,
    calls: usize,
}

thread_local! {
    static WINDOW: Cell<AllocationWindow> = const { Cell::new(AllocationWindow {
        active: false, live: 0, peak: 0, calls: 0,
    }) };
}

fn record(delta: isize, allocation: bool) {
    let _ = WINDOW.try_with(|window| {
        let mut current = window.get();
        if current.active {
            if allocation {
                current.calls += 1;
            }
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
            record(layout.size() as isize, true);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        record(-(layout.size() as isize), false);
        // SAFETY: The caller supplies the original allocation and layout.
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: The caller supplies a live allocation and valid new size.
        let result = unsafe { System.realloc(pointer, layout, size) };
        if !result.is_null() {
            record(size as isize - layout.size() as isize, true);
        }
        result
    }
}

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

pub fn measure<T>(operation: impl FnOnce() -> T) -> (T, usize) {
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

pub fn allocation_calls() -> usize {
    WINDOW.with(|window| window.get().calls)
}
