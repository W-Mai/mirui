//! Regression coverage for the time-plumbing spec:
//! `mirui::info!` timestamps must be non-zero and monotonic on std,
//! even without any clock plugin installed. Mock installation must
//! override the read source, and Drop must restore the real clock.

use mirui::core::log::sinks::RingBufferSink;
use mirui::core::log::{Level, clear_sinks, install_sink, set_max_level};
use mirui::core::time::{clock_now_ns, is_clock_installed, mock};

#[test]
fn std_auto_anchors_first_log_and_stays_monotonic() {
    let _serial = mock::install();
    // Uninstall the mock: this test asserts the real std clock.
    drop(_serial);

    clear_sinks();
    set_max_level(Level::Info);
    let sink = RingBufferSink::new(4);
    let handle = sink.handle();
    install_sink(Box::new(sink));

    assert!(is_clock_installed(), "std auto-anchors");

    mirui::info!("first");
    std::thread::sleep(std::time::Duration::from_millis(2));
    mirui::info!("second");

    let recs = handle.records();
    assert_eq!(recs.len(), 2);
    // A ~1 µs anchor delay is enough to prove the epoch is real; the
    // key assertion is strict monotonic growth on the second event.
    assert!(
        recs[1].time_ns > recs[0].time_ns,
        "second time_ns {} must exceed first {}",
        recs[1].time_ns,
        recs[0].time_ns,
    );
    // 2 ms sleep on any reasonable clock produces >= 500 µs of delta.
    assert!(
        recs[1].time_ns - recs[0].time_ns > 500_000,
        "delta {} ns too small — clock likely stuck",
        recs[1].time_ns - recs[0].time_ns,
    );
}

#[test]
fn mock_install_routes_clock_reads_through_mock_buffer() {
    let _guard = mock::install();
    mock::set_ns(42_000);
    assert_eq!(clock_now_ns(), 42_000);
    mock::advance_ms(10);
    assert_eq!(clock_now_ns(), 42_000 + 10_000_000);
}

#[test]
fn mock_drop_restores_real_clock_source() {
    let baseline = clock_now_ns();
    {
        let _guard = mock::install();
        mock::set_ns(1);
        assert_eq!(clock_now_ns(), 1);
    }
    let after = clock_now_ns();
    assert!(
        after >= baseline,
        "real clock non-monotonic across mock lifetime: baseline={baseline} after={after}"
    );
    assert_ne!(after, 1, "mock leaked past its guard");
}
