#![cfg(feature = "tracing-bridge")]

use crate::core::log::{Event, Level, Sink};

pub struct TracingBridge;

impl TracingBridge {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TracingBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl Sink for TracingBridge {
    fn emit(&self, event: &Event<'_>) {
        match event.meta.level {
            Level::Error => tracing::error!(target: "mirui", "{}", event.args),
            Level::Warn => tracing::warn!(target: "mirui", "{}", event.args),
            Level::Info => tracing::info!(target: "mirui", "{}", event.args),
            Level::Debug => tracing::debug!(target: "mirui", "{}", event.args),
            Level::Trace => tracing::trace!(target: "mirui", "{}", event.args),
        }
    }
}
