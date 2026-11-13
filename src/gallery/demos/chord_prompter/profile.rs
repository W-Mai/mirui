//! What a playback at 128×128 costs the display link and the renderer.
//!
//! The helpers return numbers the budget tests below assert on; the ignored
//! tests print them. `print_primitive_profile` also needs the `perf`
//! feature, and its times mean something only in an optimised build:
//!
//! ```sh
//! cargo test --release --lib --features gallery,std,perf \
//!     print_primitive_profile -- --ignored --nocapture
//! ```

use super::*;
use alloc::{rc::Rc, vec::Vec};
use core::cell::RefCell;

pub(super) const CJK: &str = "---\ntitle: Practice\nkey: C\nbpm: 96.5\ntime: 4/4\nstrum: pop\n---\n--- Verse ---\n[跟]着[节]拍 | | [回]到起点 @ 1/5 6m 4\n--- Chorus | strum:whole ---\n[再]走一步 @ 1\n";

/// Rectangles sent to the display for each frame of a playback.
///
/// `frame_ms` is the frame period and `prepare` runs once before the first
/// full frame, for example to start playback with a count-in.
pub(super) fn flushed_rects(
    source: &str,
    frame_ms: u64,
    frames: u64,
    prepare: impl FnOnce(&SessionHandle),
) -> Vec<Vec<[u16; 4]>> {
    use crate::prelude::*;
    let flushed = Rc::new(RefCell::new(Vec::new()));
    let log = flushed.clone();
    let surface = crate::surface::framebuf::FramebufSurface::new(
        128,
        128,
        move |_: &[u8], area: crate::types::PhysicalRect| {
            log.borrow_mut()
                .push([area.x(), area.y(), area.width(), area.height()]);
        },
    );
    let mut app = App::new(surface);
    app.with_default_widgets().with_default_systems();
    app.add_resource(MonoClock::new(|| 0));
    let root = app.spawn_root().id();
    let session = install(&mut app, root);
    session.load(source).unwrap();
    prepare(&session);
    app.render().unwrap();
    (1..=frames)
        .map(|frame| {
            session.advance(frame * frame_ms * 1000);
            flushed.borrow_mut().clear();
            app.render_dirty().unwrap();
            flushed.borrow().clone()
        })
        .collect()
}

/// Pixels sent to the display in each frame.
pub(super) fn flushed_per_frame(
    source: &str,
    frame_ms: u64,
    frames: u64,
    prepare: impl FnOnce(&SessionHandle),
) -> Vec<u32> {
    flushed_rects(source, frame_ms, frames, prepare)
        .iter()
        .map(|rects| {
            rects
                .iter()
                .map(|r| u32::from(r[2]) * u32::from(r[3]))
                .sum()
        })
        .collect()
}

/// Pixels in a full 128×128 frame.
const SCREEN: u32 = 128 * 128;

#[test]
fn playing_sends_less_than_a_screen_per_frame_and_a_paused_prompter_sends_nothing() {
    // A frame every 33 ms for 12 s: nearly five bars of the practice song.
    for (name, source, least) in [("practice", PRACTICE, 80), ("cjk", CJK, 68)] {
        let frames = flushed_per_frame(source, 33, 360, |session| session.toggle(0));
        let average = frames.iter().map(|px| u64::from(*px)).sum::<u64>() / frames.len() as u64;
        assert!(
            average <= u64::from(SCREEN) * least / 100,
            "{name}: {average} px per frame is over {least}% of the screen"
        );
        // The bar line replaces three pages at once; even that frame may not
        // cost much more than the screen.
        let worst = frames.iter().max().unwrap();
        assert!(
            *worst <= SCREEN * 11 / 10,
            "{name}: a frame sent {worst} px"
        );
    }
    let paused = flushed_per_frame(PRACTICE, 33, 30, |session| {
        session.toggle(0);
        session.pause(0);
    });
    assert!(paused.iter().all(|px| *px == 0), "{paused:?}");
}

#[test]
fn a_steady_frame_sends_the_scrolled_area_once() {
    // Mid-bar, between beats: the lyric area scrolls as a whole, so the
    // marks inside it are part of that area and must not be sent again.
    let frames = flushed_rects(PRACTICE, 33, 20, |session| session.toggle(0));
    for (frame, rects) in frames.iter().enumerate() {
        for (i, a) in rects.iter().enumerate() {
            for (j, b) in rects.iter().enumerate() {
                let inside = b[0] >= a[0]
                    && b[1] >= a[1]
                    && b[0] + b[2] <= a[0] + a[2]
                    && b[1] + b[3] <= a[1] + a[3];
                assert!(
                    i == j || !inside,
                    "frame {frame}: {b:?} is sent again inside {a:?}"
                );
            }
        }
    }
}

#[test]
#[ignore = "prints numbers; run with --ignored --nocapture"]
fn print_flush_profile() {
    for (name, source) in [("practice", PRACTICE), ("cjk", CJK)] {
        let playing = flushed_per_frame(source, 33, 360, |session| session.toggle(0));
        let sum: u64 = playing.iter().map(|px| u64::from(*px)).sum();
        let max = playing.iter().max().unwrap();
        let idle = playing.iter().filter(|px| **px == 0).count();
        std::eprintln!(
            "{name}: avg {} px/frame ({:.1}% of 16384), max {max}, idle frames {idle}/{}",
            sum / playing.len() as u64,
            sum as f64 / playing.len() as f64 / 163.84,
            playing.len()
        );
    }
}

#[cfg(feature = "perf")]
#[test]
#[ignore = "prints numbers; run with --ignored --nocapture"]
fn print_primitive_profile() {
    use crate::core::perf;
    use crate::prelude::*;
    use std::collections::BTreeMap;
    use std::time::Instant;
    let surface = crate::surface::framebuf::FramebufSurface::new(128, 128, |_: &[u8], _| {});
    let mut app = App::new(surface);
    app.with_default_widgets().with_default_systems();
    app.add_resource(MonoClock::new(|| 0));
    let root = app.spawn_root().id();
    let session = install(&mut app, root);
    session.load(PRACTICE).unwrap();
    session.toggle(0);
    app.render().unwrap();
    for frame in 1..=60u64 {
        session.advance(frame * 33_000);
        app.render_dirty().unwrap();
    }
    const FRAMES: u64 = 600;
    let mut capture = perf::Capture::new(perf::DEFAULT_TRACE_BUDGET).unwrap();
    let mut spans: BTreeMap<&'static str, (u64, u64)> = BTreeMap::new();
    let start = Instant::now();
    for frame in 61..61 + FRAMES {
        session.advance(frame * 33_000);
        app.render_dirty().unwrap();
        capture.drain_events(|_, batch| {
            for record in batch {
                if let Some(span) = record.span() {
                    let entry = spans.entry(span.name).or_default();
                    entry.0 += 1;
                    entry.1 += span.end_ns.saturating_sub(span.start_ns);
                }
            }
        });
    }
    let wall = start.elapsed();
    capture.stop();
    std::eprintln!(
        "wall {} us/frame over {FRAMES} frames (includes trace draining)",
        wall.as_micros() as u64 / FRAMES
    );
    let mut rows: Vec<_> = spans.into_iter().collect();
    rows.sort_by_key(|(_, (_, ns))| core::cmp::Reverse(*ns));
    for (name, (count, ns)) in rows.iter().take(14) {
        std::eprintln!(
            "  {name:<26} {:>6.2} calls/frame {:>8.1} us/frame",
            *count as f64 / FRAMES as f64,
            *ns as f64 / FRAMES as f64 / 1000.0
        );
    }
}
