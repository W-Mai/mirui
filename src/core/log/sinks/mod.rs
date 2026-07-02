#[cfg(feature = "log-bridge")]
pub mod log_bridge;
#[cfg(all(feature = "log-nuttx-syslog", target_os = "nuttx"))]
pub mod nuttx_syslog;
#[cfg(feature = "std")]
pub mod ringbuf;
#[cfg(feature = "std")]
pub mod stderr;
#[cfg(feature = "tracing-bridge")]
pub mod tracing_bridge;
#[cfg(all(feature = "log-web-console", target_arch = "wasm32"))]
pub mod web_console;

#[cfg(feature = "log-bridge")]
pub use log_bridge::LogBridge;
#[cfg(all(feature = "log-nuttx-syslog", target_os = "nuttx"))]
pub use nuttx_syslog::NuttxSyslogSink;
#[cfg(feature = "std")]
pub use ringbuf::{LoggedRecord, RingBufferHandle, RingBufferSink};
#[cfg(feature = "std")]
pub use stderr::StderrSink;
#[cfg(feature = "tracing-bridge")]
pub use tracing_bridge::TracingBridge;
#[cfg(all(feature = "log-web-console", target_arch = "wasm32"))]
pub use web_console::WebConsoleSink;
