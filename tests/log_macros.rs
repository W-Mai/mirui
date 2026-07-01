use mirui::core::log::sinks::RingBufferSink;
use mirui::core::log::{Level, clear_sinks, set_max_level};

#[test]
fn info_reaches_ring_buffer_from_external_crate() {
    clear_sinks();
    set_max_level(Level::Trace);
    let sink = RingBufferSink::new(8);
    let handle = sink.handle();
    mirui::core::log::install_sink(Box::new(sink));

    mirui::info!("hello {}", 42);
    mirui::warn!("v={:?}", vec![1, 2, 3]);
    mirui::error!(target: "custom", "boom {:x}", 255);

    let records = handle.records();
    assert_eq!(records.len(), 3);
    assert_eq!(records[0].level, Level::Info);
    assert_eq!(records[0].message, "hello 42");
    assert_eq!(records[1].level, Level::Warn);
    assert_eq!(records[1].message, "v=[1, 2, 3]");
    assert_eq!(records[2].level, Level::Error);
    assert_eq!(records[2].target, "custom");
    assert_eq!(records[2].message, "boom ff");
}
