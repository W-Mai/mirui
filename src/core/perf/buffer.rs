use super::{PerfEvent, Record, RecordData};
use alloc::boxed::Box;

/// Storage for one registered producer. Exporting never calls a sink under its lock.
pub(super) struct Buffer {
    events: Box<[Record]>,
    head: usize,
    len: usize,
    sequence: u64,
    depth: u8,
    scopes: u32,
    pub closed: bool,
    dropped: u64,
    invalid: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TraceStatus {
    pub dropped: u64,
    pub invalid: u64,
    pub open_scopes: u32,
    pub closed: bool,
}

impl Buffer {
    pub fn invalid_scope(&mut self) {
        self.invalid = self.invalid.saturating_add(1);
    }
    pub fn new(capacity: usize) -> Result<Self, super::CaptureError> {
        let mut events = alloc::vec::Vec::new();
        events
            .try_reserve_exact(capacity)
            .map_err(|_| super::CaptureError::Capacity)?;
        events.resize(capacity, Record::EMPTY);
        Ok(Self {
            events: events.into_boxed_slice(),
            head: 0,
            len: 0,
            sequence: 0,
            depth: 0,
            scopes: 0,
            closed: false,
            dropped: 0,
            invalid: 0,
        })
    }

    pub fn enter(&mut self) -> Option<u8> {
        if self.closed {
            return None;
        }
        let Some(next) = self.depth.checked_add(1) else {
            self.invalid = self.invalid.saturating_add(1);
            return None;
        };
        let depth = self.depth;
        self.depth = next;
        Some(depth)
    }

    pub fn exit(&mut self, event: PerfEvent, valid_clock: bool) -> bool {
        if self.depth != event.depth + 1 {
            self.invalid = self.invalid.saturating_add(1);
        }
        self.depth = self.depth.saturating_sub(1);
        self.record(
            RecordData::Span(event),
            valid_clock && event.end_ns >= event.start_ns,
        )
    }

    pub fn record(&mut self, data: RecordData, valid: bool) -> bool {
        match data {
            RecordData::Context { .. } => self.scopes = self.scopes.saturating_add(1),
            RecordData::Scope { .. } => self.scopes = self.scopes.saturating_sub(1),
            RecordData::IncompleteScope { .. } => {
                self.scopes = self.scopes.saturating_sub(1);
                self.invalid_scope();
            }
            _ => {}
        }
        let Some(sequence) = self.sequence.checked_add(1) else {
            self.closed = true;
            self.invalid = self.invalid.saturating_add(1);
            return false;
        };
        self.sequence = sequence;
        if self.closed || !valid {
            self.invalid = self.invalid.saturating_add(1);
            return false;
        }
        if self.len == self.events.len() {
            self.dropped = self.dropped.saturating_add(1);
            return false;
        }
        let pos = (self.head + self.len) % self.events.len();
        self.events[pos] = Record { sequence, data };
        self.len += 1;
        true
    }

    pub fn snapshot(&mut self) -> (usize, TraceStatus) {
        let status = TraceStatus {
            dropped: core::mem::take(&mut self.dropped),
            invalid: core::mem::take(&mut self.invalid),
            open_scopes: u32::from(self.depth).saturating_add(self.scopes),
            closed: self.closed,
        };
        (self.len, status)
    }

    pub fn read(&mut self, destination: &mut [Record]) -> usize {
        let count = self.len.min(destination.len());
        for out in &mut destination[..count] {
            *out = self.events[self.head];
            self.head = (self.head + 1) % self.events.len();
        }
        self.len -= count;
        count
    }

    #[cfg(feature = "std")]
    pub fn empty(&self) -> bool {
        self.len == 0 && self.dropped == 0 && self.invalid == 0
    }

    #[cfg(feature = "std")]
    pub fn release_storage(&mut self) {
        self.closed = true;
        self.events = Box::default();
        self.head = 0;
        self.len = 0;
    }
}
