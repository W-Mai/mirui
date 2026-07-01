#![cfg(all(feature = "log-web-console", target_arch = "wasm32"))]

use crate::core::log::{Event, Level, Sink};
use alloc::string::String;
use core::fmt::Write as _;

pub struct WebConsoleSink;

impl WebConsoleSink {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WebConsoleSink {
    fn default() -> Self {
        Self::new()
    }
}

impl Sink for WebConsoleSink {
    fn emit(&self, event: &Event<'_>) {
        let mut msg = String::new();
        let _ = write!(
            &mut msg,
            "[{} {}] {}",
            event.meta.level.as_str(),
            event.meta.target,
            event.args
        );
        let js = wasm_bindgen::JsValue::from_str(&msg);
        match event.meta.level {
            Level::Error => web_sys::console::error_1(&js),
            Level::Warn => web_sys::console::warn_1(&js),
            Level::Info => web_sys::console::info_1(&js),
            Level::Debug => web_sys::console::debug_1(&js),
            Level::Trace => web_sys::console::log_1(&js),
        }
    }
}
