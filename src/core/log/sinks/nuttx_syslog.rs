#![cfg(all(feature = "log-nuttx-syslog", target_os = "nuttx"))]

use crate::core::log::{Event, Level, Sink};
use alloc::string::String;
use core::ffi::{c_char, c_int};
use core::fmt::Write as _;

const LOG_ERR: c_int = 3;
const LOG_WARNING: c_int = 4;
const LOG_INFO: c_int = 6;
const LOG_DEBUG: c_int = 7;

unsafe extern "C" {
    fn syslog(priority: c_int, fmt: *const c_char, ...);
}

fn syslog_line(priority: c_int, msg: &str) {
    let mut buf = [0u8; 256];
    let n = msg.len().min(buf.len() - 1);
    buf[..n].copy_from_slice(&msg.as_bytes()[..n]);
    buf[n] = 0;
    let fmt = b"%s\n\0".as_ptr() as *const c_char;
    // SAFETY: `fmt` is a valid C string literal; `buf` is NUL-terminated
    // within its 256-byte stack lifetime and syslog reads through the
    // ptr before returning.
    unsafe {
        syslog(priority, fmt, buf.as_ptr());
    }
}

pub struct NuttxSyslogSink {
    min_level: Level,
}

impl NuttxSyslogSink {
    pub fn new(min_level: Level) -> Self {
        Self { min_level }
    }

    pub fn info() -> Self {
        Self::new(Level::Info)
    }
}

impl Default for NuttxSyslogSink {
    fn default() -> Self {
        Self::info()
    }
}

impl Sink for NuttxSyslogSink {
    fn enabled(&self, level: Level, _target: &'static str) -> bool {
        level <= self.min_level
    }

    fn emit(&self, event: &Event<'_>) {
        let priority = match event.meta.level {
            Level::Error => LOG_ERR,
            Level::Warn => LOG_WARNING,
            Level::Info => LOG_INFO,
            Level::Debug | Level::Trace => LOG_DEBUG,
        };
        let mut msg = String::new();
        let _ = write!(&mut msg, "[{}] {}", event.meta.target, event.args);
        syslog_line(priority, &msg);
    }
}
