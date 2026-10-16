use super::*;
use crate::render::backends::sw::SwRenderer;
use crate::render::path::PathCmd;
use crate::render::texture::{AlphaMode, ColorFormat};
use crate::test_support::tracked_allocations;
use crate::types::{Color, Fixed, Viewport};

const PATH: Path = Path::from_static(&[
    PathCmd::MoveTo(Point {
        x: Fixed::from_int(2),
        y: Fixed::from_int(2),
    }),
    PathCmd::LineTo(Point {
        x: Fixed::from_int(6),
        y: Fixed::from_int(2),
    }),
    PathCmd::LineTo(Point {
        x: Fixed::from_int(2),
        y: Fixed::from_int(6),
    }),
    PathCmd::Close,
]);

struct ScopeRenderer<'a> {
    target: SwRenderer<'a>,
    plan_error: Option<RenderError>,
    exit_error: Option<RenderError>,
    scopes: usize,
    submissions: usize,
}

impl Renderer for ScopeRenderer<'_> {
    type TargetState = ();

    fn route(&self, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        if matches!(request.command, DrawCommand::PushClip { .. }) {
            Err(RenderError::Unsupported(RenderFeature::PathClip))
        } else {
            self.target.route(request)
        }
    }

    fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        self.submissions += 1;
        self.route(request)?;
        self.target.submit(request)
    }

    fn flush(&mut self) -> Result<(), RenderError> {
        self.target.flush()
    }

    fn output_scale(&self) -> Fixed {
        self.target.output_scale()
    }

    fn plan_scope(&self, bounds: &Rect) -> Result<FallbackRegion, RenderError> {
        if let Some(error) = self.plan_error {
            return Err(error);
        }
        FallbackRegion::from_logical_bounds(*bounds, self.target.viewport, Some(16 * 16 * 4))
    }

    fn render_scope(
        &mut self,
        region: FallbackRegion,
        body: &mut dyn FnMut(&mut dyn Renderer) -> Result<(), RenderError>,
    ) -> Result<bool, RenderError> {
        self.scopes += 1;
        let result = self.target.render_scope(region, body);
        self.exit_error.map_or(result, Err)
    }
}

fn software(bytes: &mut [u8]) -> SwRenderer<'_> {
    let texture = Texture::new(bytes, 16, 16, ColorFormat::RGBA8888).unwrap();
    let mut renderer = SwRenderer::new(texture).with_alpha_mode(AlphaMode::Blend);
    renderer.viewport = Viewport::new(16, 16, Fixed::from_int(2));
    renderer
}

fn fallback(bytes: &mut [u8]) -> ScopeRenderer<'_> {
    ScopeRenderer {
        target: software(bytes),
        plan_error: None,
        exit_error: None,
        scopes: 0,
        submissions: 0,
    }
}

fn draw_nested(renderer: &mut dyn Renderer) {
    let style = Style::default();
    let rect = Rect::new(1, 1, 6, 6);
    let quad = [Point::new(1, 1); 4];
    let transform = Transform::translate(Fixed::ONE, Fixed::ONE);
    let mut paint = PaintCtx::new(
        renderer,
        &style,
        &rect,
        &rect,
        transform,
        Some(quad),
        WidgetState::Enabled,
    );
    let workspace = PaintWorkspace::default();
    paint.set_workspace(&workspace);
    paint.set_layout_offset(Point::new(3, 4));
    paint.query_reads_global = false;
    let mut calls = 0;
    paint.with_clip_path(&PATH, Transform::IDENTITY, FillRule::EvenOdd, |paint| {
        calls += 1;
        assert!(core::ptr::eq(paint.style(), &style));
        assert!(core::ptr::eq(paint.workspace.unwrap(), &workspace));
        assert_eq!(paint.rect(), rect);
        assert_eq!(paint.clip(), rect);
        assert_eq!(paint.transform(), transform);
        assert_eq!(paint.quad(), Some(quad));
        assert_eq!(paint.output_scale(), Fixed::from_int(2));
        assert_eq!(paint.layout_offset, Point::new(3, 4));
        assert!(!paint.query_reads_global);
        paint.with_clip_path(&PATH, Transform::IDENTITY, FillRule::NonZero, |paint| {
            calls += 1;
            paint.query_reads_global = true;
            paint.set_background_handled(true);
            paint.request_frame();
            paint.draw(&DrawCommand::Fill {
                area: rect,
                transform: Transform::IDENTITY,
                quad: None,
                color: Color::rgba(30, 90, 150, 128),
                radius: Fixed::ZERO,
                opa: 255,
            });
        });
    });
    assert_eq!(calls, 2);
    assert!(paint.background_handled());
    assert!(paint.query_reads_global);
    assert!(matches!(paint.frame_request(), FrameRequest::Next));
    assert_eq!(paint.error(), None);
}

#[test]
fn fallback_clip_preserves_nested_geometry_context_and_single_execution() {
    let mut expected = [0u8; 16 * 16 * 4];
    draw_nested(&mut software(&mut expected));
    let mut actual = [0u8; 16 * 16 * 4];
    let mut renderer = fallback(&mut actual);
    draw_nested(&mut renderer);
    assert_eq!(renderer.scopes, 1);
    assert_eq!(renderer.submissions, 0);
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert_eq!(
            *actual,
            expected,
            "pixel {}, channel {}",
            index / 4,
            index % 4
        );
    }
    assert!(actual.iter().any(|byte| *byte != 0));
    assert_eq!(&actual[..16 * 4], &[0; 16 * 4]);
}

#[test]
fn clip_planning_failure_skips_body_without_submissions_or_allocations() {
    let mut bytes = [0u8; 16 * 16 * 4];
    let mut renderer = fallback(&mut bytes);
    renderer.plan_error = Some(RenderError::MissingWorkspace);
    let style = Style::default();
    let rect = Rect::new(0, 0, 8, 8);
    let allocations = tracked_allocations(|| {
        let mut paint = PaintCtx::new(
            &mut renderer,
            &style,
            &rect,
            &rect,
            Transform::IDENTITY,
            None,
            WidgetState::Enabled,
        );
        paint.with_clip_path(&PATH, Transform::IDENTITY, FillRule::NonZero, |_| {
            panic!("unprepared scope must not execute");
        });
        assert_eq!(paint.error(), Some(RenderError::MissingWorkspace));
    });
    assert_eq!(allocations, 0);
    assert_eq!(renderer.scopes, 0);
    assert_eq!(renderer.submissions, 0);
    assert_eq!(bytes, [0; 16 * 16 * 4]);
}

#[test]
fn clip_body_failure_precedes_region_writeback_failure_without_replay() {
    let mut bytes = [0u8; 16 * 16 * 4];
    let mut renderer = fallback(&mut bytes);
    renderer.exit_error = Some(RenderError::BackendFailure);
    let style = Style::default();
    let rect = Rect::new(0, 0, 8, 8);
    let mut paint = PaintCtx::new(
        &mut renderer,
        &style,
        &rect,
        &rect,
        Transform::IDENTITY,
        None,
        WidgetState::Enabled,
    );
    let mut calls = 0;
    paint.with_clip_path(&PATH, Transform::IDENTITY, FillRule::NonZero, |paint| {
        calls += 1;
        paint.fail(RenderError::InvalidGeometry);
    });
    assert_eq!(paint.error(), Some(RenderError::InvalidGeometry));
    paint.with_clip_path(&PATH, Transform::IDENTITY, FillRule::NonZero, |_| {
        panic!("failed paint must not execute another scope");
    });
    assert_eq!(calls, 1);
    assert_eq!(renderer.scopes, 1);
    assert_eq!(renderer.submissions, 0);
}
