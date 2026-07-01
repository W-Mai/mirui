use crate::core::log::{Event, Level, Sink};

pub struct StderrSink {
    min_level: Level,
}

impl StderrSink {
    pub fn new(min_level: Level) -> Self {
        Self { min_level }
    }

    pub fn info() -> Self {
        Self::new(Level::Info)
    }

    pub fn warn() -> Self {
        Self::new(Level::Warn)
    }

    pub fn debug() -> Self {
        Self::new(Level::Debug)
    }

    pub fn trace() -> Self {
        Self::new(Level::Trace)
    }
}

impl Sink for StderrSink {
    fn enabled(&self, level: Level, _target: &'static str) -> bool {
        level <= self.min_level
    }

    fn emit(&self, event: &Event<'_>) {
        eprintln!(
            "[{} {}] {}",
            event.meta.level.as_str(),
            event.meta.target,
            event.args
        );
    }
}
