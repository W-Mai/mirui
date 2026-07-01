extern crate alloc;

use crate::core::log::{Event, Level, Sink};
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::sync::Arc;
use core::fmt::Write;

#[cfg(feature = "std")]
type LockCell<T> = std::sync::Mutex<T>;
#[cfg(not(feature = "std"))]
type LockCell<T> = spin::Mutex<T>;

#[derive(Clone)]
pub struct LoggedRecord {
    pub level: Level,
    pub target: &'static str,
    pub time_ns: u64,
    pub message: String,
}

pub struct RingBufferSink {
    inner: Arc<LockCell<VecDeque<LoggedRecord>>>,
    cap: usize,
}

impl RingBufferSink {
    pub fn new(cap: usize) -> Self {
        Self {
            inner: Arc::new(LockCell::new(VecDeque::with_capacity(cap.max(1)))),
            cap: cap.max(1),
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
        #[cfg(feature = "std")]
        let mut guard = self.inner.lock().expect("ringbuf poisoned");
        #[cfg(not(feature = "std"))]
        let mut guard = self.inner.lock();
        if guard.len() == self.cap {
            guard.pop_front();
        }
        guard.push_back(rec);
    }
}

pub struct RingBufferHandle {
    inner: Arc<LockCell<VecDeque<LoggedRecord>>>,
}

impl RingBufferHandle {
    pub fn records(&self) -> alloc::vec::Vec<LoggedRecord> {
        #[cfg(feature = "std")]
        {
            self.inner
                .lock()
                .expect("ringbuf poisoned")
                .iter()
                .cloned()
                .collect()
        }
        #[cfg(not(feature = "std"))]
        {
            self.inner.lock().iter().cloned().collect()
        }
    }

    pub fn clear(&self) {
        #[cfg(feature = "std")]
        self.inner.lock().expect("ringbuf poisoned").clear();
        #[cfg(not(feature = "std"))]
        self.inner.lock().clear();
    }
}
