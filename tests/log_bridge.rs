#![cfg(feature = "log-bridge")]

use std::sync::Mutex;

struct CaptureLogger {
    rows: Mutex<Vec<(log::Level, String, String)>>,
}

impl log::Log for CaptureLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        self.rows.lock().unwrap().push((
            record.level(),
            record.target().to_string(),
            record.args().to_string(),
        ));
    }
    fn flush(&self) {}
}

static LOGGER: CaptureLogger = CaptureLogger {
    rows: Mutex::new(Vec::new()),
};

#[test]
fn mirui_events_reach_log_crate_logger() {
    log::set_logger(&LOGGER).ok();
    log::set_max_level(log::LevelFilter::Trace);

    mirui::core::log::clear_sinks();
    mirui::core::log::set_max_level(mirui::core::log::Level::Trace);
    mirui::core::log::install_sink(Box::new(mirui::core::log::sinks::LogBridge::new()));

    LOGGER.rows.lock().unwrap().clear();
    mirui::info!(target: "mirui::bridge_test", "answer={}", 42);
    mirui::warn!("plain {:?}", vec![1, 2]);

    let rows = LOGGER.rows.lock().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, log::Level::Info);
    assert_eq!(rows[0].1, "mirui::bridge_test");
    assert_eq!(rows[0].2, "answer=42");
    assert_eq!(rows[1].0, log::Level::Warn);
    assert_eq!(rows[1].2, "plain [1, 2]");
}
