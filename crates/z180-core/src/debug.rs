use super::*;

#[cfg_attr(feature = "state", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Copy)]
pub(super) struct MemWatch {
    pub(super) id: WatchId,
    pub(super) base: u32,
    pub(super) size: u32,
    pub(super) kind: WatchKind,
}

pub(super) struct TraceCapture {
    pub(super) entry: TraceEntry,
    pub(super) captured: u8,
}

impl<B: HostBus> Z180<B> {
    pub fn add_mem_watch(&mut self, base: u32, size: u32, kind: WatchKind) -> WatchId {
        let id = WatchId(self.next_watch_id);
        self.next_watch_id = self.next_watch_id.wrapping_add(1);
        if self.next_watch_id == 0 {
            self.next_watch_id = 1;
        }
        self.mem_watches.push(MemWatch {
            id,
            base,
            size,
            kind,
        });
        let _ = self.ensure_event_storage();
        id
    }
    pub fn remove_mem_watch(&mut self, id: WatchId) {
        self.mem_watches.retain(|watch| watch.id != id);
    }
    pub fn set_io_trace(&mut self, enabled: bool) {
        self.io_trace = enabled;
        if enabled {
            let _ = self.ensure_event_storage();
        }
    }
    pub fn set_irq_trace(&mut self, enabled: bool) {
        self.irq_trace = enabled;
        if enabled {
            let _ = self.ensure_event_storage();
        }
    }
    pub fn set_pc_watch(&mut self, addr: Option<u16>) {
        self.pc_watch = addr;
        self.pc_watch_hits = 0;
    }
    pub fn pc_watch_hits(&self) -> u64 {
        self.pc_watch_hits
    }
    /// Drain retained entries in chronological order without allocating.
    /// Dropping the iterator discards unconsumed entries. Ring storage is retained.
    /// The sticky event-loss flag is unchanged.
    pub fn drain_events_iter(
        &mut self,
    ) -> impl ExactSizeIterator<Item = Event> + DoubleEndedIterator + '_ {
        self.events.drain(..)
    }
    pub fn drain_events(&mut self) -> Vec<Event> {
        self.drain_events_iter().collect()
    }
    pub fn events_lost(&self) -> bool {
        self.events_lost
    }
    pub fn clear_events_lost(&mut self) {
        self.events_lost = false;
    }
    pub fn set_insn_trace(&mut self, capacity: Option<usize>) {
        self.insn_trace_capture = None;
        let Some(capacity) = capacity else {
            self.insn_trace_capacity = None;
            self.insn_trace = VecDeque::new();
            return;
        };

        if self.insn_trace.capacity() < capacity
            && self
                .insn_trace
                .try_reserve_exact(capacity - self.insn_trace.len())
                .is_err()
        {
            return;
        }
        while self.insn_trace.len() > capacity {
            let _ = self.insn_trace.pop_front();
        }
        self.insn_trace_capacity = Some(capacity);
    }
    /// Drain retained entries in chronological order without allocating.
    /// Dropping the iterator discards unconsumed entries. Ring storage is retained.
    pub fn drain_insn_trace_iter(
        &mut self,
    ) -> impl ExactSizeIterator<Item = TraceEntry> + DoubleEndedIterator + '_ {
        self.insn_trace.drain(..)
    }
    pub fn drain_insn_trace(&mut self) -> Vec<TraceEntry> {
        self.drain_insn_trace_iter().collect()
    }
    pub(super) fn begin_insn_trace(&mut self, pc: u16) {
        let Some(capacity) = self.insn_trace_capacity else {
            return;
        };
        if capacity == 0 {
            return;
        }
        self.insn_trace_capture = Some(TraceCapture {
            entry: TraceEntry {
                cycle: self.cycle_count,
                pc,
                phys_pc: self.mmu_translate(pc),
                bytes: [0; 4],
                len: 0,
            },
            captured: 0,
        });
    }
    pub(super) fn capture_insn_byte(&mut self, logical: u16, value: u8) {
        let Some(capture) = &mut self.insn_trace_capture else {
            return;
        };
        let offset = logical.wrapping_sub(capture.entry.pc);
        if offset >= 4 {
            return;
        }
        let bit = 1_u8 << offset;
        if capture.captured & bit == 0 {
            capture.entry.bytes[usize::from(offset)] = value;
            capture.captured |= bit;
        }
    }
    pub(super) fn finish_insn_trace(&mut self, len: u8) {
        let Some(mut capture) = self.insn_trace_capture.take() else {
            return;
        };
        let used = usize::from(len).min(capture.entry.bytes.len());
        capture.entry.len = used as u8;
        for byte in &mut capture.entry.bytes[used..] {
            *byte = 0;
        }
        self.push_insn_trace(capture.entry);
    }
    fn push_insn_trace(&mut self, entry: TraceEntry) {
        let Some(capacity) = self.insn_trace_capacity else {
            return;
        };
        if capacity == 0 {
            return;
        }
        if self.insn_trace.capacity() < capacity
            && self
                .insn_trace
                .try_reserve_exact(capacity - self.insn_trace.len())
                .is_err()
        {
            return;
        }
        if self.insn_trace.len() == capacity {
            let _ = self.insn_trace.pop_front();
        }
        self.insn_trace.push_back(entry);
    }
    fn ensure_event_storage(&mut self) -> bool {
        if self.event_capacity == 0 || self.events.capacity() >= self.event_capacity {
            return self.event_capacity != 0;
        }
        self.events
            .try_reserve_exact(self.event_capacity - self.events.len())
            .is_ok()
    }
    pub(super) fn push_event(&mut self, event: Event) {
        if !self.ensure_event_storage() {
            self.events_lost = true;
            return;
        }
        if self.events.len() == self.event_capacity {
            let _ = self.events.pop_front();
            self.events_lost = true;
        }
        self.events.push_back(event);
    }
}
