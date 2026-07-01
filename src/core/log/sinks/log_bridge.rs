#![cfg(feature = "log-bridge")]

use crate::core::log::{Event, Level, Sink};

pub struct LogBridge;

impl LogBridge {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LogBridge {
    fn default() -> Self {
        Self::new()
    }
}

fn map_level(level: Level) -> log::Level {
    match level {
        Level::Error => log::Level::Error,
        Level::Warn => log::Level::Warn,
        Level::Info => log::Level::Info,
        Level::Debug => log::Level::Debug,
        Level::Trace => log::Level::Trace,
    }
}

impl Sink for LogBridge {
    fn emit(&self, event: &Event<'_>) {
        log::logger().log(
            &log::RecordBuilder::new()
                .level(map_level(event.meta.level))
                .target(event.meta.target)
                .args(*event.args)
                .build(),
        );
    }
}
