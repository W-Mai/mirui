extern crate alloc;

use crate::core::log::{Event, Level, Sink};
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::sync::Arc;
use core::fmt::Write;
use std::sync::Mutex;

#[derive(Clone)]
pub struct LoggedRecord {
    pub level: Level,
    pub target: &'static str,
    pub time_ns: u64,
    pub message: String,
}

pub struct RingBufferSink {
    inner: Arc<Mutex<VecDeque<LoggedRecord>>>,
    cap: usize,
}

impl RingBufferSink {
    pub fn new(cap: usize) -> Self {
        let cap = cap.max(1);
        Self {
            inner: Arc::new(Mutex::new(VecDeque::with_capacity(cap))),
            cap,
        }
    }

    pub fn handle(&self) -> RingBufferHandle {
        RingBufferHandle {
            inner: self.inner.clone(),
        }
    }
}

impl Sink for RingBufferSink {
    fn emit(&self, event: &Event<'_>) {
        let mut msg = String::new();
        let _ = write!(&mut msg, "{}", event.args);
        let rec = LoggedRecord {
            level: event.meta.level,
            target: event.meta.target,
            time_ns: event.time_ns,
            message: msg,
        };
        let mut guard = self.inner.lock().expect("ringbuf poisoned");
        if guard.len() == self.cap {
            guard.pop_front();
        }
        guard.push_back(rec);
    }
}

pub struct RingBufferHandle {
    inner: Arc<Mutex<VecDeque<LoggedRecord>>>,
}

impl RingBufferHandle {
    pub fn records(&self) -> alloc::vec::Vec<LoggedRecord> {
        self.inner
            .lock()
            .expect("ringbuf poisoned")
            .iter()
            .cloned()
            .collect()
    }

    pub fn clear(&self) {
        self.inner.lock().expect("ringbuf poisoned").clear();
    }
}
