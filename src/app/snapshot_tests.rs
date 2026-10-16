use super::*;
use alloc::rc::Rc;
use core::cell::{Cell, RefCell};

use crate::render::RenderError;
use crate::render::texture::{ColorFormat, Texture};
use crate::surface::framebuf::FramebufSurface;
use crate::types::{Color, Fixed};
use crate::ui::Style;

type Trace = Rc<RefCell<Vec<&'static str>>>;
type Flush = Box<dyn FnMut(&[u8], PhysicalRect)>;
type TestSurface = FramebufSurface<Flush>;

#[test]
fn snapshot_copy_owns_packed_rows_without_source_padding() {
    for format in [
        ColorFormat::RGB565,
        ColorFormat::RGB565Swapped,
        ColorFormat::RGB888,
        ColorFormat::RGBA8888,
        ColorFormat::BGRA8888,
    ] {
        let packed = crate::render::scratch::PlaneLayout::packed(2, 2, format)
            .unwrap()
            .stride_bytes();
        let stride = packed + 3;
        let mut source = Texture::from_vec(alloc::vec![0; stride + packed], 2, 2, format).unwrap();
        source.stride = stride;
        source.alpha_mode = crate::render::texture::AlphaMode::Blend;
        source
            .pixels_mut()
            .unwrap()
            .set_pixel(0, 0, &Color::rgb(255, 0, 0));
        source
            .pixels_mut()
            .unwrap()
            .set_pixel(1, 1, &Color::rgb(0, 255, 0));
        let copy = clone_texture_owned(&source).unwrap();
        assert_eq!(copy.alpha_mode, source.alpha_mode);
        assert_eq!(copy.stride, packed);
        assert_eq!(copy.read_pixels().unwrap().bytes().len(), packed * 2);
        assert_ne!(
            copy.read_pixels().unwrap().bytes().as_ptr(),
            source.read_pixels().unwrap().bytes().as_ptr()
        );
        source
            .pixels_mut()
            .unwrap()
            .set_pixel(1, 1, &Color::rgb(0, 0, 255));
        assert_eq!(
            copy.read_pixels().unwrap().get_pixel(0, 0),
            Color::rgb(255, 0, 0)
        );
        assert_eq!(
            copy.read_pixels().unwrap().get_pixel(1, 1),
            Color::rgb(0, 255, 0)
        );
    }
}

struct Hooks {
    trace: Trace,
    root: Entity,
}

impl Plugin<TestSurface, SwRendererFactory> for Hooks {
    fn build(&mut self, _: &mut App<TestSurface>) {}

    fn pre_render(&mut self, world: &mut World) {
        self.trace.borrow_mut().push("prepare");
        world.get_mut::<Style>(self.root).unwrap().bg_color = Some(Color::rgb(255, 0, 0).into());
    }

    fn post_render(&mut self, _: &mut World, _: u64) {
        self.trace.borrow_mut().push("complete");
    }
}

fn app(format: ColorFormat, scale: Fixed, trace: Trace) -> App<TestSurface> {
    let flush: Flush = Box::new(move |_, _| {
        trace.borrow_mut().push("present");
    });
    let mut app = App::new(FramebufSurface::with_scale_and_format(
        24, 18, scale, format, flush,
    ));
    app.with_default_widgets();
    app
}

#[test]
fn snapshot_preserves_target_pixels_format_and_scale_without_offscreen_request() {
    for format in [
        ColorFormat::RGBA8888,
        ColorFormat::RGB565,
        ColorFormat::BGRA8888,
    ] {
        for scale in [Fixed::ONE, Fixed::from_ratio(3, 2), Fixed::from_int(2)] {
            let trace = Trace::default();
            let mut app = app(format, scale, trace.clone());
            let root = app.spawn_root().bg_color(Color::rgb(0, 0, 255)).id();
            app.add_plugin(Hooks {
                trace: trace.clone(),
                root,
            });
            let image = app.snapshot().unwrap().unwrap();
            assert_eq!(image.format, format);
            assert_eq!((image.width, image.height), (24, 18));
            assert_eq!(
                image.read_pixels().unwrap().get_pixel(23, 17),
                Color::rgb(255, 0, 0)
            );
            assert_eq!(
                image.read_pixels().unwrap().bytes(),
                app.backend.framebuffer().read_pixels().unwrap().bytes()
            );
            assert_eq!(&*trace.borrow(), &["prepare", "present", "complete"]);
            assert!(app.world.get::<crate::ui::OffscreenRender>(root).is_none());
            assert!(
                app.world
                    .resource::<Outputs<WidgetTexture>>()
                    .unwrap()
                    .get(root)
                    .is_none()
            );
            assert!(app.dirty_regions().unwrap().is_empty());
        }
    }
}

#[test]
fn frame_readback_runs_before_presentation_and_errors_do_not_discard_painted_baseline() {
    let trace = Trace::default();
    let mut app = app(ColorFormat::RGBA8888, Fixed::ONE, trace.clone());
    let root = app.spawn_root().id();
    app.add_plugin(Hooks {
        trace: trace.clone(),
        root,
    });
    for error in [
        RenderError::BackendFailure,
        RenderError::Unsupported(crate::render::RenderFeature::Readback),
    ] {
        trace.borrow_mut().clear();
        let result = app.render_frame(|renderer, viewport| {
            trace.borrow_mut().push("read");
            let (width, height) = viewport.logical_size();
            let area = Rect::new(0, 0, width, height);
            renderer.prepare_readback(&area)?;
            let image = renderer.sample_target_region(&area)?.unwrap();
            assert_eq!(
                image.read_pixels().unwrap().get_pixel(0, 0),
                Color::rgb(255, 0, 0)
            );
            Err::<(), _>(error)
        });
        assert_eq!(result, Err(error));
        assert_eq!(
            &*trace.borrow(),
            &["prepare", "read", "present", "complete"]
        );
        assert!(!app.needs_full_redraw);
        assert!(app.dirty_regions().unwrap().is_empty());
    }
}

#[test]
fn snapshot_without_root_does_not_run_frame_hooks_or_present() {
    let trace = Trace::default();
    let mut app = app(ColorFormat::RGBA8888, Fixed::ONE, trace.clone());
    assert!(app.snapshot().unwrap().is_none());
    assert!(trace.borrow().is_empty());
}

#[crate::component]
struct FailingPaint;

#[crate::view(component = FailingPaint)]
fn failing_paint(paint: paint!()) {
    paint.fail(RenderError::BackendFailure);
}

#[test]
fn failed_drawing_skips_readback_and_surface_submission() {
    let trace = Trace::default();
    let mut app = app(ColorFormat::RGBA8888, Fixed::ONE, trace.clone());
    app.with_widget(failing_paint::view());
    let root = app.spawn_root().id();
    app.world.insert(root, FailingPaint);
    let called = Cell::new(false);
    let result = app.render_frame(|_, _| {
        called.set(true);
        Ok(())
    });
    assert_eq!(result, Err(RenderError::BackendFailure));
    assert!(!called.get());
    assert!(trace.borrow().is_empty());
    assert!(app.needs_full_redraw);
}

#[test]
fn snapshot_baseline_supports_subsequent_dirty_rendering() {
    let mut app = App::headless(24, 18);
    app.with_default_widgets();
    let root = app.spawn_root().bg_color(Color::rgb(255, 0, 0)).id();
    let captured = app.snapshot().unwrap().unwrap();
    app.world.get_mut::<Style>(root).unwrap().bg_color = Some(Color::rgb(0, 0, 255).into());
    app.render_dirty().unwrap();
    let dirty = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    let full = app.snapshot().unwrap().unwrap();
    assert_eq!(full.read_pixels().unwrap().bytes(), dirty);
    assert_eq!(
        captured.read_pixels().unwrap().get_pixel(0, 0),
        Color::rgb(255, 0, 0)
    );
    assert_eq!(
        full.read_pixels().unwrap().get_pixel(0, 0),
        Color::rgb(0, 0, 255)
    );
}

#[test]
fn hidden_root_snapshot_contains_the_same_clear_as_render() {
    let mut app = App::headless(24, 18);
    app.with_default_widgets();
    let root = app.spawn_root().bg_color(Color::rgb(255, 0, 0)).id();
    app.render().unwrap();
    app.world.insert(root, crate::ui::Hidden);
    let image = app.snapshot().unwrap().unwrap();
    let expected = app.theme().resolve(crate::ui::ColorToken::Surface);
    assert_eq!(image.read_pixels().unwrap().get_pixel(0, 0), expected);
    assert_eq!(image.read_pixels().unwrap().get_pixel(23, 17), expected);
    app.render().unwrap();
    assert_eq!(
        image.read_pixels().unwrap().bytes(),
        app.backend.framebuffer().read_pixels().unwrap().bytes()
    );
}

struct SubmissionFactory {
    trace: Trace,
    fail: Rc<Cell<bool>>,
}

struct SubmissionRenderer {
    trace: Trace,
    fail: Rc<Cell<bool>>,
}

impl RendererFactory<TestSurface> for SubmissionFactory {
    type Renderer<'a> = SubmissionRenderer;

    fn make<'a>(
        &'a mut self,
        _: &'a mut TestSurface,
        _: &crate::types::Viewport,
    ) -> Self::Renderer<'a> {
        SubmissionRenderer {
            trace: self.trace.clone(),
            fail: self.fail.clone(),
        }
    }
}

impl Renderer for SubmissionRenderer {
    type TargetState = ();

    fn route(
        &self,
        _: &crate::render::DrawRequest<'_, '_>,
    ) -> Result<crate::render::RenderRoute, RenderError> {
        Ok(crate::render::RenderRoute::Native)
    }

    fn submit(&mut self, _: &crate::render::DrawRequest<'_, '_>) -> Result<(), RenderError> {
        Ok(())
    }

    fn flush(&mut self) -> Result<(), RenderError> {
        self.trace.borrow_mut().push("submit");
        if self.fail.get() {
            Err(RenderError::BackendFailure)
        } else {
            Ok(())
        }
    }
}

impl crate::render::canvas::Canvas for SubmissionRenderer {
    fn fill_path(
        &mut self,
        _: &crate::render::path::Path,
        _: &Rect,
        _: &crate::render::canvas::Paint,
        _: u8,
        _: crate::render::raster::FillRule,
    ) {
    }

    fn stroke_path(
        &mut self,
        _: &crate::render::path::Path,
        _: &Rect,
        _: Fixed,
        _: &crate::render::canvas::Paint,
        _: u8,
        _: crate::render::raster::LineCap,
        _: crate::render::raster::LineJoin,
        _: Fixed,
        _: &[Fixed],
    ) {
    }

    fn draw_glyph_run(
        &mut self,
        _: &crate::types::Point,
        _: &[textflow::shaping::PositionedGlyph],
        _: &crate::render::font::Font,
        _: &Rect,
        _: &Color,
        _: u8,
    ) {
    }

    fn draw_posed_glyph_run(
        &mut self,
        _: &crate::types::Point,
        _: crate::render::PosedGlyphs<'_>,
        _: &crate::render::font::Font,
        _: &Rect,
        _: &Color,
        _: u8,
    ) {
    }

    fn blit(
        &mut self,
        _: &crate::render::texture::Texture<'_>,
        _: &Rect,
        _: crate::types::Point,
        _: crate::types::Point,
        _: &Rect,
        _: u8,
        _: Fixed,
        _: crate::render::command::CompositeMode,
    ) {
    }

    fn clear(&mut self, _: &Rect, _: &Color) {}

    fn flush(&mut self) -> Result<(), RenderError> {
        Renderer::flush(self)
    }
}

#[test]
fn frame_submission_failure_never_presents_or_accepts_full_dirty_or_snapshot_frames() {
    let trace = Trace::default();
    let fail = Rc::new(Cell::new(false));
    let output = trace.clone();
    let flush: Flush = Box::new(move |_, _| output.borrow_mut().push("present"));
    let mut app = App::with_factory(
        FramebufSurface::new(24, 18, flush),
        SubmissionFactory {
            trace: trace.clone(),
            fail: fail.clone(),
        },
    );
    app.with_default_widgets();
    let root = app.spawn_root().id();
    app.render().unwrap();
    assert_eq!(&*trace.borrow(), &["submit", "present"]);

    for mode in 0..3 {
        trace.borrow_mut().clear();
        fail.set(true);
        app.world
            .get_mut::<Style>(root)
            .unwrap()
            .set_bg_color(Color::rgb(80, mode, 0));
        let result = match mode {
            0 => app.render(),
            1 => app.render_dirty(),
            _ => app
                .render_frame(|_, _| {
                    trace.borrow_mut().push("read");
                    Ok(())
                })
                .map(|_| ()),
        };
        assert_eq!(result, Err(RenderError::BackendFailure));
        assert!(app.needs_full_redraw);
        assert_eq!(
            trace
                .borrow()
                .iter()
                .filter(|&&step| step == "submit")
                .count(),
            1
        );
        assert!(!trace.borrow().contains(&"present"));
        assert_eq!(
            app.dirty_regions().unwrap().rects,
            [Rect::new(0, 0, 24, 18)]
        );
        trace.borrow_mut().clear();
        fail.set(false);
        app.render_dirty().unwrap();
        assert_eq!(&*trace.borrow(), &["submit", "present"]);
        assert!(!app.needs_full_redraw);
        assert!(app.dirty_regions().unwrap().is_empty());
    }

    trace.borrow_mut().clear();
    app.with_widget(failing_paint::view());
    app.world.insert(root, FailingPaint);
    assert_eq!(app.render(), Err(RenderError::BackendFailure));
    assert!(trace.borrow().is_empty());
}
