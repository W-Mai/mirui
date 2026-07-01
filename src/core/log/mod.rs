extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU8, Ordering};

pub mod sinks;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Level {
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
    Trace = 5,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Error => "ERROR",
            Level::Warn => "WARN",
            Level::Info => "INFO",
            Level::Debug => "DEBUG",
            Level::Trace => "TRACE",
        }
    }
}

pub struct Metadata {
    pub level: Level,
    pub target: &'static str,
}

pub struct Event<'a> {
    pub meta: &'static Metadata,
    pub time_ns: u64,
    pub args: &'a core::fmt::Arguments<'a>,
}

#[derive(Clone, Copy)]
pub struct SpanRecord {
    pub name: &'static str,
    pub start_ns: u64,
    pub end_ns: u64,
    pub depth: u8,
}

impl SpanRecord {
    pub fn elapsed_ns(&self) -> u64 {
        self.end_ns.saturating_sub(self.start_ns)
    }
}

pub trait Sink: Send + Sync {
    fn emit(&self, event: &Event<'_>);
    fn emit_span(&self, _span: &SpanRecord) {}
    fn enabled(&self, _level: Level, _target: &'static str) -> bool {
        true
    }
    fn flush(&self) {}
}

static MAX_LEVEL: AtomicU8 = AtomicU8::new(Level::Info as u8);

pub fn set_max_level(level: Level) {
    MAX_LEVEL.store(level as u8, Ordering::Relaxed);
}

pub fn max_level() -> Level {
    match MAX_LEVEL.load(Ordering::Relaxed) {
        1 => Level::Error,
        2 => Level::Warn,
        3 => Level::Info,
        4 => Level::Debug,
        _ => Level::Trace,
    }
}

pub const STATIC_MAX_LEVEL: u8 = static_max_level();

const fn static_max_level() -> u8 {
    if cfg!(feature = "log-max-level-off") {
        0
    } else if cfg!(feature = "log-max-level-error") {
        Level::Error as u8
    } else if cfg!(feature = "log-max-level-warn") {
        Level::Warn as u8
    } else if cfg!(feature = "log-max-level-info") {
        Level::Info as u8
    } else if cfg!(feature = "log-max-level-debug") {
        Level::Debug as u8
    } else if cfg!(feature = "log-max-level-trace") {
        Level::Trace as u8
    } else {
        u8::MAX
    }
}

#[inline]
pub fn level_enabled(level: Level, _target: &'static str) -> bool {
    (level as u8) <= MAX_LEVEL.load(Ordering::Relaxed)
}

#[cfg(feature = "std")]
mod registry {
    use super::*;
    use std::sync::{OnceLock, RwLock};

    static REGISTRY: OnceLock<RwLock<Vec<Box<dyn Sink>>>> = OnceLock::new();

    fn slots() -> &'static RwLock<Vec<Box<dyn Sink>>> {
        REGISTRY.get_or_init(|| RwLock::new(Vec::new()))
    }

    pub fn install(sink: Box<dyn Sink>) {
        slots().write().expect("log registry poisoned").push(sink);
    }

    pub fn clear() {
        slots().write().expect("log registry poisoned").clear();
    }

    pub fn dispatch_to_all(event: &Event<'_>) {
        let guard = slots().read().expect("log registry poisoned");
        for sink in guard.iter() {
            if sink.enabled(event.meta.level, event.meta.target) {
                sink.emit(event);
            }
        }
    }

    pub fn dispatch_span_to_all(span: &SpanRecord) {
        let guard = slots().read().expect("log registry poisoned");
        for sink in guard.iter() {
            sink.emit_span(span);
        }
    }
}

#[cfg(not(feature = "std"))]
mod registry {
    use super::*;

    struct State {
        sinks: Vec<Box<dyn Sink>>,
    }

    static mut STATE: State = State { sinks: Vec::new() };

    fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
        critical_section::with(|_| {
            #[allow(static_mut_refs)]
            unsafe {
                f(&mut STATE)
            }
        })
    }

    pub fn install(sink: Box<dyn Sink>) {
        with_state(|s| s.sinks.push(sink));
    }

    pub fn clear() {
        with_state(|s| s.sinks.clear());
    }

    pub fn dispatch_to_all(event: &Event<'_>) {
        with_state(|s| {
            for sink in s.sinks.iter() {
                if sink.enabled(event.meta.level, event.meta.target) {
                    sink.emit(event);
                }
            }
        });
    }

    pub fn dispatch_span_to_all(span: &SpanRecord) {
        with_state(|s| {
            for sink in s.sinks.iter() {
                sink.emit_span(span);
            }
        });
    }
}

pub fn install_sink(sink: Box<dyn Sink>) {
    registry::install(sink);
}

pub fn clear_sinks() {
    registry::clear();
}

pub fn dispatch(meta: &'static Metadata, args: core::fmt::Arguments<'_>) {
    let event = Event {
        meta,
        time_ns: clock_now_ns(),
        args: &args,
    };
    registry::dispatch_to_all(&event);
}

pub fn dispatch_span(span: &SpanRecord) {
    registry::dispatch_span_to_all(span);
}

fn clock_now_ns() -> u64 {
    crate::core::time::clock_now_ns()
}

#[macro_export]
macro_rules! __mirui_log {
    (target: $target:expr, $lvl:expr, $($arg:tt)+) => {{
        static META: $crate::core::log::Metadata = $crate::core::log::Metadata {
            level: $lvl,
            target: $target,
        };
        if (META.level as u8) <= $crate::core::log::STATIC_MAX_LEVEL
            && $crate::core::log::level_enabled(META.level, META.target)
        {
            $crate::core::log::dispatch(&META, ::core::format_args!($($arg)+));
        }
    }};
    ($lvl:expr, $($arg:tt)+) => {{
        static META: $crate::core::log::Metadata = $crate::core::log::Metadata {
            level: $lvl,
            target: ::core::module_path!(),
        };
        if (META.level as u8) <= $crate::core::log::STATIC_MAX_LEVEL
            && $crate::core::log::level_enabled(META.level, META.target)
        {
            $crate::core::log::dispatch(&META, ::core::format_args!($($arg)+));
        }
    }};
}

#[macro_export]
macro_rules! error {
    (target: $target:expr, $($arg:tt)+) => (
        $crate::__mirui_log!(target: $target, $crate::core::log::Level::Error, $($arg)+)
    );
    ($($arg:tt)+) => (
        $crate::__mirui_log!($crate::core::log::Level::Error, $($arg)+)
    );
}

#[macro_export]
macro_rules! warn {
    (target: $target:expr, $($arg:tt)+) => (
        $crate::__mirui_log!(target: $target, $crate::core::log::Level::Warn, $($arg)+)
    );
    ($($arg:tt)+) => (
        $crate::__mirui_log!($crate::core::log::Level::Warn, $($arg)+)
    );
}

#[macro_export]
macro_rules! info {
    (target: $target:expr, $($arg:tt)+) => (
        $crate::__mirui_log!(target: $target, $crate::core::log::Level::Info, $($arg)+)
    );
    ($($arg:tt)+) => (
        $crate::__mirui_log!($crate::core::log::Level::Info, $($arg)+)
    );
}

#[macro_export]
macro_rules! debug {
    (target: $target:expr, $($arg:tt)+) => (
        $crate::__mirui_log!(target: $target, $crate::core::log::Level::Debug, $($arg)+)
    );
    ($($arg:tt)+) => (
        $crate::__mirui_log!($crate::core::log::Level::Debug, $($arg)+)
    );
}

#[macro_export]
macro_rules! trace {
    (target: $target:expr, $($arg:tt)+) => (
        $crate::__mirui_log!(target: $target, $crate::core::log::Level::Trace, $($arg)+)
    );
    ($($arg:tt)+) => (
        $crate::__mirui_log!($crate::core::log::Level::Trace, $($arg)+)
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::{String, ToString};
    use alloc::sync::Arc;
    use core::fmt::Write;
    use std::sync::Mutex;

    struct CaptureSink {
        rows: Arc<Mutex<Vec<String>>>,
    }

    impl Sink for CaptureSink {
        fn emit(&self, event: &Event<'_>) {
            let mut msg = String::new();
            write!(&mut msg, "{}", event.args).unwrap();
            self.rows.lock().unwrap().push(alloc::format!(
                "{}:{}:{}",
                event.meta.level.as_str(),
                event.meta.target,
                msg
            ));
        }
    }

    fn install_capture() -> Arc<Mutex<Vec<String>>> {
        clear_sinks();
        let rows: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        install_sink(Box::new(CaptureSink { rows: rows.clone() }));
        rows
    }

    #[test]
    fn static_max_level_default_is_open() {
        assert_eq!(STATIC_MAX_LEVEL, u8::MAX);
    }

    #[test]
    fn level_ordering() {
        assert!(Level::Error < Level::Warn);
        assert!(Level::Warn < Level::Info);
        assert!(Level::Info < Level::Debug);
        assert!(Level::Debug < Level::Trace);
    }

    #[test]
    fn info_reaches_sink() {
        let _lock = TEST_MUTEX.lock().unwrap();
        set_max_level(Level::Info);
        let rows = install_capture();
        crate::info!("hello {}", 42);
        let captured = rows.lock().unwrap().clone();
        assert_eq!(captured.len(), 1);
        assert!(captured[0].starts_with("INFO:"));
        assert!(captured[0].ends_with(":hello 42"));
    }

    #[test]
    fn debug_gated_by_max_level() {
        let _lock = TEST_MUTEX.lock().unwrap();
        set_max_level(Level::Info);
        let rows = install_capture();
        crate::debug!("shhh {}", 7);
        assert!(rows.lock().unwrap().is_empty());
        set_max_level(Level::Debug);
        crate::debug!("now {}", 8);
        assert_eq!(rows.lock().unwrap().len(), 1);
    }

    #[test]
    fn filter_miss_short_circuits_args() {
        let _lock = TEST_MUTEX.lock().unwrap();
        set_max_level(Level::Error);
        install_capture();
        let mut side_effect = 0;
        crate::info!("{}", {
            side_effect += 1;
            "boom"
        });
        assert_eq!(side_effect, 0, "format_args should not evaluate when gated");
    }

    #[test]
    fn debug_format_works() {
        let _lock = TEST_MUTEX.lock().unwrap();
        set_max_level(Level::Info);
        let rows = install_capture();
        let v = alloc::vec![1u32, 2, 3];
        crate::info!("{:?}", v);
        let captured = rows.lock().unwrap().clone();
        assert!(captured[0].ends_with(":[1, 2, 3]"), "got {:?}", captured);
    }

    #[test]
    fn custom_target() {
        let _lock = TEST_MUTEX.lock().unwrap();
        set_max_level(Level::Info);
        let rows = install_capture();
        crate::warn!(target: "mirui::custom", "x");
        let captured = rows.lock().unwrap().clone();
        assert!(
            captured[0].starts_with("WARN:mirui::custom:"),
            "got {:?}",
            captured
        );
    }

    #[test]
    fn dispatch_fans_out_to_all_sinks() {
        let _lock = TEST_MUTEX.lock().unwrap();
        set_max_level(Level::Info);
        clear_sinks();
        let a: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let b: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        install_sink(Box::new(CaptureSink { rows: a.clone() }));
        install_sink(Box::new(CaptureSink { rows: b.clone() }));
        crate::info!("fan-out {}", 1);
        assert_eq!(a.lock().unwrap().len(), 1);
        assert_eq!(b.lock().unwrap().len(), 1);
    }

    #[test]
    fn span_dispatch_reaches_sinks() {
        let _lock = TEST_MUTEX.lock().unwrap();
        set_max_level(Level::Info);

        struct SpanCapture(Arc<Mutex<Vec<(&'static str, u64)>>>);
        impl Sink for SpanCapture {
            fn emit(&self, _: &Event<'_>) {}
            fn emit_span(&self, span: &SpanRecord) {
                self.0.lock().unwrap().push((span.name, span.elapsed_ns()));
            }
        }

        clear_sinks();
        let rows: Arc<Mutex<Vec<(&'static str, u64)>>> = Arc::new(Mutex::new(Vec::new()));
        install_sink(Box::new(SpanCapture(rows.clone())));

        crate::core::perf::set_enabled(true);
        {
            let _g = crate::core::perf::enter("test.span");
        }
        crate::core::perf::set_enabled(false);
        let _ = crate::core::perf::drain_events();

        let captured = rows.lock().unwrap().clone();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].0, "test.span");
    }

    #[test]
    fn sink_enabled_gate_skips_emit() {
        let _lock = TEST_MUTEX.lock().unwrap();
        set_max_level(Level::Trace);

        struct MinInfo(Arc<Mutex<Vec<String>>>);
        impl Sink for MinInfo {
            fn emit(&self, event: &Event<'_>) {
                self.0
                    .lock()
                    .unwrap()
                    .push(event.meta.level.as_str().to_string());
            }
            fn enabled(&self, level: Level, _target: &'static str) -> bool {
                level <= Level::Info
            }
        }

        clear_sinks();
        let rows: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        install_sink(Box::new(MinInfo(rows.clone())));
        crate::info!("keep");
        crate::debug!("drop");
        let captured = rows.lock().unwrap().clone();
        assert_eq!(captured, alloc::vec!["INFO".to_string()]);
    }

    static TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());
}
