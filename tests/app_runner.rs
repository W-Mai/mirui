//! Smoke tests for the `App::tick` / `App::into_runner` / `Runner`
//! API surface introduced by Phase A of the surface-platforms cycle.
//!
//! Behavioural coverage of the loop body lives in the surrounding
//! integration tests (input dispatch, plugin hooks, render); these
//! tests confirm only that the new wrapper compiles and that types
//! line up correctly when an `App` is consumed into a `Runner`.

use mirui::app::{App, Runner};
use mirui::draw::texture::ColorFormat;
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::Rect;

fn dummy_backend() -> FramebufSurface<impl FnMut(&[u8], &Rect)> {
    FramebufSurface::with_format(8, 8, ColorFormat::RGBA8888, |_bytes, _area| {})
}

#[test]
fn app_into_runner_yields_runner() {
    let app: App<_, _> = App::new(dummy_backend());
    let _runner: Runner<_, _> = app.into_runner();
}

#[test]
fn tick_returns_false_when_no_quit_event() {
    let mut app = App::new(dummy_backend());
    assert!(
        !app.tick(),
        "tick() must return false when no Quit event was processed"
    );
}
