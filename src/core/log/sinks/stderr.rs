use crate::core::log::{Event, Level, Sink};
use std::io::Write;

/// Writes directly to standard error without using the test harness capture buffer.
pub struct StderrSink {
    min_level: Level,
}

impl StderrSink {
    pub fn new(min_level: Level) -> Self {
        // Prepare the standard library's platform lock before logging.
        drop(std::io::stderr().lock());
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
        writeln!(
            std::io::stderr().lock(),
            "[{} {}] {}",
            event.meta.level.as_str(),
            event.meta.target,
            event.args
        )
        .expect("failed to write log to stderr");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::log::Metadata;

    #[test]
    fn stderr_output_does_not_allocate() {
        static META: Metadata = Metadata {
            level: Level::Warn,
            target: "mirui::test",
        };
        let sink = StderrSink::warn();
        let allocations = crate::test_support::tracked_allocations(|| {
            sink.emit(&Event {
                meta: &META,
                time_ns: 0,
                args: &format_args!("warning {}", 42),
            });
        });
        assert_eq!(allocations, 0);
    }
}
