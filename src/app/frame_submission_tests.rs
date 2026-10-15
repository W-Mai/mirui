use super::*;
use crate::render::RenderError;
use crate::surface::{DisplayInfo, framebuf::FramebufSurface};
use crate::types::{Color, Fixed};
use crate::ui::dirty::PaintedCoverage;

type Pixels = FramebufSurface<fn(&[u8], PhysicalRect)>;

struct CheckedSurface {
    pixels: Pixels,
    fail: Option<&'static str>,
    trace: Vec<&'static str>,
}

impl CheckedSurface {
    fn step(&mut self, name: &'static str) -> Result<(), RenderError> {
        self.trace.push(name);
        if self.fail == Some(name) {
            Err(if name == "begin" {
                RenderError::SurfaceUnavailable
            } else {
                RenderError::BackendFailure
            })
        } else {
            Ok(())
        }
    }
}

impl crate::core::cache::InspectCaches for CheckedSurface {}

impl Surface for CheckedSurface {
    fn display_info(&self) -> DisplayInfo {
        self.pixels.display_info()
    }
    fn poll_event(&mut self) -> Option<InputEvent> {
        None
    }
    fn begin_flush(&mut self) -> Result<(), RenderError> {
        self.step("begin")
    }
    fn flush(&mut self, _: PhysicalRect) -> Result<(), RenderError> {
        self.step("flush")
    }
    fn end_flush(&mut self) -> Result<(), RenderError> {
        self.step("end")
    }
    fn abort_flush(&mut self) {
        self.trace.push("abort");
    }
    fn frame_end(&mut self) -> Result<(), RenderError> {
        self.step("maintain")
    }
}

impl FramebufferAccess for CheckedSurface {
    fn framebuffer(&mut self) -> crate::render::texture::Texture<'_> {
        self.trace.push("draw");
        self.pixels.framebuffer()
    }
}

#[crate::compose]
fn marker() {
    crate::ui! {
        View (
            id: "marker",
            position: crate::ui::layout::Position::Absolute,
            left: 0,
            top: 0,
            width: 8,
            height: 8,
            bg_color: Color::rgb(255, 0, 0)
        )
    };
}

fn fixture() -> (App<CheckedSurface>, Entity) {
    let mut app = App::new(CheckedSurface {
        pixels: FramebufSurface::new(40, 30, |_, _| {}),
        fail: None,
        trace: Vec::new(),
    });
    app.with_default_widgets();
    let root = app.spawn_root().id();
    app.compose(root, marker());
    let node = app.world.find_by_id("marker").unwrap();
    app.render().unwrap();
    app.backend.trace.clear();
    (app, node)
}

fn assert_submission(trace: &[&str], finish: &[&str]) {
    assert_eq!(&trace[..2], &["begin", "draw"]);
    let end = trace.len() - finish.len();
    assert!(end > 2);
    assert!(trace[2..end].iter().all(|&step| step == "flush"));
    assert_eq!(&trace[end..], finish);
}

#[test]
fn surface_failures_preserve_the_accepted_baseline_and_retry_latest_state() {
    for entry in 0..3 {
        for failure in ["begin", "flush", "end"] {
            let (mut app, node) = fixture();
            let baseline = app.world.get::<PaintedCoverage>(node).unwrap().0.outer;
            let pixels = app.backend.pixels.framebuffer().buf.as_slice().to_vec();
            crate::ui::set_position(&mut app.world, node, 16, 0);
            app.backend.fail = Some(failure);
            for _ in 0..3 {
                assert!(!app.dirty_regions().unwrap().is_empty());
            }
            assert!(
                app.backend.trace.is_empty(),
                "queries must not acquire a surface"
            );
            let result = match entry {
                0 => app.render(),
                1 => app.render_dirty(),
                _ => app.snapshot().map(|_| ()),
            };
            assert!(result.is_err());
            assert_eq!(
                app.world.get::<PaintedCoverage>(node).unwrap().0.outer,
                baseline
            );
            assert_eq!(app.backend.trace.last(), Some(&"abort"));
            assert_eq!(app.needs_full_redraw, failure != "begin");
            if failure == "begin" {
                assert_eq!(&app.backend.trace, &["begin", "abort"]);
                assert_eq!(app.backend.pixels.framebuffer().buf.as_slice(), pixels);
            }
            crate::ui::set_position(&mut app.world, node, 20, 0);
            app.backend.fail = None;
            app.backend.trace.clear();
            app.render_dirty().unwrap();
            assert_submission(&app.backend.trace, &["end"]);
            assert_eq!(
                app.world.get::<PaintedCoverage>(node).unwrap().0.outer.x,
                Fixed::from_int(20)
            );
            assert!(app.dirty_regions().unwrap().is_empty());
            let accepted = app.backend.pixels.framebuffer().buf.as_slice().to_vec();
            app.render().unwrap();
            assert_eq!(app.backend.pixels.framebuffer().buf.as_slice(), accepted);
        }
    }
}

#[test]
fn idle_and_failed_ticks_still_run_surface_maintenance() {
    let (mut app, node) = fixture();
    assert!(!app.tick());
    assert_eq!(&app.backend.trace, &["maintain"]);
    app.backend.trace.clear();
    app.backend.fail = Some("end");
    crate::ui::set_position(&mut app.world, node, 16, 0);
    assert!(!app.tick());
    assert_submission(&app.backend.trace, &["end", "abort", "maintain"]);
}
