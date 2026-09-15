//! Verify a composed renderer with one target and one routed engine.

use std::cell::Cell;

use mirui::render::canvas::{Canvas, Paint};
use mirui::render::command::CompositeMode;
use mirui::render::engine::RenderEngine;
use mirui::render::path::Path;
use mirui::render::renderer::{DrawRequest, RenderError, RenderFeature, RenderRoute, Renderer};
use mirui::render::texture::{ColorFormat, Texture};
use mirui::types::{Color, Fixed, Point, Rect, Transform};
use mirui_macros::compose_backend;

#[derive(Default)]
struct Counts {
    fill_path: Cell<u32>,
    fill_rule: Cell<Option<mirui::render::raster::FillRule>>,
    stroke_path: Cell<u32>,
    blit: Cell<u32>,
    clear: Cell<u32>,
    draw_glyph_run: Cell<u32>,
    draw_posed_glyph_run: Cell<u32>,
    flush: Cell<u32>,
    fill_rect: Cell<u32>,
    stroke_rect: Cell<u32>,
    draw_line: Cell<u32>,
    draw_arc: Cell<u32>,
}

struct Dummy {
    counts: Counts,
}

impl Dummy {
    fn new() -> Self {
        Self {
            counts: Counts::default(),
        }
    }
}

impl Canvas for Dummy {
    fn fill_path(
        &mut self,
        _: &Path,
        _: &Rect,
        _: &Paint,
        _: u8,
        fill_rule: ::mirui::render::raster::FillRule,
    ) {
        self.counts.fill_path.set(self.counts.fill_path.get() + 1);
        self.counts.fill_rule.set(Some(fill_rule));
    }
    fn stroke_path(
        &mut self,
        _: &Path,
        _: &Rect,
        _: Fixed,
        _: &Paint,
        _: u8,
        _: ::mirui::render::raster::LineCap,
        _: ::mirui::render::raster::LineJoin,
        _: ::mirui::types::Fixed,
        _: &[Fixed],
    ) {
        self.counts
            .stroke_path
            .set(self.counts.stroke_path.get() + 1);
    }
    fn blit(
        &mut self,
        _: &Texture,
        _: &Rect,
        _: Point,
        _: Point,
        _: &Rect,
        _: u8,
        _: Fixed,
        _: CompositeMode,
    ) {
        self.counts.blit.set(self.counts.blit.get() + 1);
    }
    fn clear(&mut self, _: &Rect, _: &Color) {
        self.counts.clear.set(self.counts.clear.get() + 1);
    }
    fn draw_glyph_run(
        &mut self,
        _: &Point,
        _: &[mirui::text::PositionedGlyph],
        _: &mirui::render::font::Font,
        _: &Rect,
        _: &Color,
        _: u8,
    ) {
        self.counts
            .draw_glyph_run
            .set(self.counts.draw_glyph_run.get() + 1);
    }
    fn draw_posed_glyph_run(
        &mut self,
        _: &Point,
        _: mirui::render::PosedGlyphs<'_>,
        _: &mirui::render::font::Font,
        _: &Rect,
        _: &Color,
        _: u8,
    ) {
        self.counts
            .draw_posed_glyph_run
            .set(self.counts.draw_posed_glyph_run.get() + 1);
    }
    fn flush(&mut self) {
        self.counts.flush.set(self.counts.flush.get() + 1);
    }
    // Override default impls so the counter actually gets hit without going
    // through fill_path / stroke_path.
    fn fill_rect(&mut self, _: &Rect, _: &Rect, _: &Color, _: Fixed, _: u8) {
        self.counts.fill_rect.set(self.counts.fill_rect.get() + 1);
    }
    fn stroke_rect(&mut self, _: &Rect, _: &Rect, _: Fixed, _: &Color, _: Fixed, _: u8) {
        self.counts
            .stroke_rect
            .set(self.counts.stroke_rect.get() + 1);
    }
    fn draw_line(&mut self, _: Point, _: Point, _: &Rect, _: Fixed, _: &Color, _: u8) {
        self.counts.draw_line.set(self.counts.draw_line.get() + 1);
    }
    fn draw_arc(
        &mut self,
        _: Point,
        _: Fixed,
        _: Fixed,
        _: Fixed,
        _: &Rect,
        _: Fixed,
        _: &Color,
        _: u8,
    ) {
        self.counts.draw_arc.set(self.counts.draw_arc.get() + 1);
    }
}

impl Renderer for Dummy {
    fn route(&self, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        request.validate()?;
        if !request.projective.is_identity() {
            return Err(RenderError::Unsupported(RenderFeature::ProjectiveGeometry));
        }
        if !request.command.transform().is_identity() {
            return Err(RenderError::Unsupported(RenderFeature::AffineGeometry));
        }
        match request.command {
            mirui::render::DrawCommand::Fill { quad: Some(_), .. }
            | mirui::render::DrawCommand::Border { quad: Some(_), .. }
            | mirui::render::DrawCommand::Blit { quad: Some(_), .. } => {
                Err(RenderError::Unsupported(RenderFeature::ProjectiveGeometry))
            }
            mirui::render::DrawCommand::PushClip { .. } | mirui::render::DrawCommand::PopClip => {
                Err(RenderError::Unsupported(RenderFeature::PathClip))
            }
            mirui::render::DrawCommand::ApplyBlur { .. } => {
                Err(RenderError::Unsupported(RenderFeature::Blur))
            }
            _ => Ok(RenderRoute::Native),
        }
    }

    fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        self.route(request)?;
        match request.command {
            mirui::render::DrawCommand::FillPath {
                path,
                paint,
                opa,
                fill_rule,
                ..
            } => self.fill_path(path, &request.clip, paint, *opa, *fill_rule),
            mirui::render::DrawCommand::Blit {
                pos,
                size,
                texture,
                opa,
                radius,
                composite,
                ..
            } => self.blit(
                texture,
                &Rect::new(0, 0, texture.width, texture.height),
                *pos,
                *size,
                &request.clip,
                *opa,
                *radius,
                *composite,
            ),
            _ => {}
        }
        Ok(())
    }

    fn flush(&mut self) {
        Canvas::flush(self);
    }
}

#[derive(Default)]
struct EngineCounts {
    begin: u32,
    submit: u32,
    end: u32,
}

#[derive(Default)]
struct CountingEngine {
    counts: EngineCounts,
    reject_begin: bool,
}

impl<T: Renderer> RenderEngine<T> for CountingEngine {
    fn route(&self, target: &T, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        target.route(request)
    }

    fn begin(&mut self, _: &mut T) -> Result<(), RenderError> {
        self.counts.begin += 1;
        if self.reject_begin {
            Err(RenderError::BackendFailure)
        } else {
            Ok(())
        }
    }

    fn submit(&mut self, target: &mut T, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        self.counts.submit += 1;
        target.submit(request)
    }

    fn end(&mut self, _: &mut T) {
        self.counts.end += 1;
    }
}

compose_backend! {
    pub struct Hybrid {
        sw: Dummy,
        gpu: CountingEngine,
    }
    route {
        default => sw,
        blit => gpu,
        fill_rect => gpu,
    }
}

fn fresh_hybrid() -> Hybrid<Dummy, CountingEngine> {
    Hybrid::new(Dummy::new(), CountingEngine::default())
}

#[test]
fn renderer_preserves_nonzero_path_fill_rule() {
    use mirui::render::renderer::{DrawRequest, Renderer};

    let mut hybrid = fresh_hybrid();
    let path = Path::rect(0.into(), 0.into(), 4.into(), 4.into());
    let paint = Paint::Color(Color::rgb(0, 0, 0).into());
    let command = mirui::render::DrawCommand::FillPath {
        path: &path,
        transform: Transform::IDENTITY,
        paint: &paint,
        opa: 255,
        fill_rule: mirui::render::raster::FillRule::NonZero,
    };
    hybrid
        .submit(&DrawRequest::new(&command, zero_rect()))
        .unwrap();
    assert_eq!(
        hybrid.sw.counts.fill_rule.get(),
        Some(mirui::render::raster::FillRule::NonZero)
    );
}

#[test]
fn checked_renderer_rejects_commands_the_canvas_router_would_change() {
    use mirui::render::renderer::{DrawRequest, RenderError, RenderFeature, Renderer};

    let mut hybrid = fresh_hybrid();
    let clip = zero_rect();
    let command = mirui::render::DrawCommand::Fill {
        area: clip,
        transform: Transform::translate(Fixed::ONE, Fixed::ZERO),
        quad: None,
        color: Color::rgb(10, 20, 30),
        radius: Fixed::ZERO,
        opa: 255,
    };
    assert_eq!(
        hybrid.submit(&DrawRequest::new(&command, clip)),
        Err(RenderError::Unsupported(RenderFeature::AffineGeometry))
    );

    let command = mirui::render::DrawCommand::Fill {
        area: clip,
        transform: Transform::IDENTITY,
        quad: Some([
            Point::ZERO,
            Point::new(4, 0),
            Point::new(4, 4),
            Point::new(0, 4),
        ]),
        color: Color::rgb(10, 20, 30),
        radius: Fixed::ZERO,
        opa: 255,
    };
    assert_eq!(
        hybrid.submit(&DrawRequest::new(&command, clip)),
        Err(RenderError::Unsupported(RenderFeature::ProjectiveGeometry))
    );

    let command = mirui::render::DrawCommand::ApplyBlur {
        alpha: Fixed::ONE,
        region: clip,
    };
    assert_eq!(
        hybrid.submit(&DrawRequest::new(&command, clip)),
        Err(RenderError::Unsupported(RenderFeature::Blur))
    );
    assert_eq!(hybrid.sw.counts.fill_rule.get(), None);
    assert_eq!(hybrid.gpu.counts.begin, 2);
    assert_eq!(hybrid.gpu.counts.submit, 2);
    assert_eq!(hybrid.gpu.counts.end, 2);
}

fn zero_rect() -> Rect {
    Rect::new(0, 0, 4, 4)
}

fn dummy_texture_buf() -> Vec<u8> {
    vec![0u8; 4 * 4 * 4]
}

#[test]
fn default_methods_route_to_sw() {
    let mut h = fresh_hybrid();
    let path = Path::new();
    let rect = zero_rect();
    let color = Color::rgb(0, 0, 0);
    let paint = Paint::Color(color.into());

    h.fill_path(
        &path,
        &rect,
        &paint,
        255,
        ::mirui::render::raster::FillRule::EvenOdd,
    );
    h.stroke_path(
        &path,
        &rect,
        Fixed::ONE,
        &paint,
        255,
        ::mirui::render::raster::LineCap::Butt,
        ::mirui::render::raster::LineJoin::Miter,
        ::mirui::types::Fixed::from_int(4),
        &[],
    );
    let font = mirui::render::font::Font::bitmap_8x8();
    h.draw_glyph_run(
        &Point::ZERO,
        &[mirui::text::PositionedGlyph::default()],
        &font,
        &rect,
        &color,
        255,
    );
    let positioned = [mirui::text::PositionedGlyph::default()];
    let frames = [textflow::placement::GlyphFrame::default()];
    h.draw_posed_glyph_run(
        &Point::ZERO,
        mirui::render::PosedGlyphs::new(&positioned, &frames).unwrap(),
        &font,
        &rect,
        &color,
        255,
    );
    Canvas::flush(&mut h);

    assert_eq!(h.sw.counts.fill_path.get(), 1);
    assert_eq!(h.sw.counts.stroke_path.get(), 1);
    assert_eq!(h.sw.counts.draw_glyph_run.get(), 1);
    assert_eq!(h.sw.counts.draw_posed_glyph_run.get(), 1);
    assert_eq!(h.sw.counts.flush.get(), 1);
    assert_eq!(h.gpu.counts.submit, 0);
}

#[test]
fn direct_canvas_calls_stay_on_the_shared_target() {
    let mut h = fresh_hybrid();
    let mut buf = dummy_texture_buf();
    let tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888);
    let rect = zero_rect();
    let color = Color::rgb(0, 0, 0);

    h.blit(
        &tex,
        &rect,
        Point::ZERO,
        Point::ZERO,
        &rect,
        255,
        Fixed::ZERO,
        CompositeMode::SourceOver,
    );
    h.clear(&rect, &color);
    h.fill_rect(&rect, &rect, &color, Fixed::ZERO, 255);

    assert_eq!(h.sw.counts.blit.get(), 1);
    assert_eq!(h.sw.counts.clear.get(), 1);
    assert_eq!(h.sw.counts.fill_rect.get(), 1);
    assert_eq!(h.gpu.counts.submit, 0);
}

#[test]
fn canvas_helpers_preserve_shared_target_overrides() {
    let mut h = fresh_hybrid();
    let rect = zero_rect();
    let color = Color::rgb(0, 0, 0);

    h.stroke_rect(&rect, &rect, Fixed::ONE, &color, Fixed::ZERO, 255);
    h.draw_line(Point::ZERO, Point::ZERO, &rect, Fixed::ONE, &color, 255);
    h.draw_arc(
        Point::ZERO,
        Fixed::from_int(4),
        Fixed::ZERO,
        Fixed::from_int(90),
        &rect,
        Fixed::ONE,
        &color,
        255,
    );

    assert_eq!(h.sw.counts.stroke_path.get(), 0);
    assert_eq!(h.sw.counts.stroke_rect.get(), 1);
    assert_eq!(h.sw.counts.draw_line.get(), 1);
    assert_eq!(h.sw.counts.draw_arc.get(), 1);
}

/// A backend that borrows a pixel buffer, giving it a real lifetime parameter
/// so we can prove the macro handles `BorrowedDummy<'fb>` as a generic type
/// argument without needing the Hybrid struct itself to declare `'fb`.
struct BorrowedDummy<'fb> {
    buf: &'fb mut [u8],
    fills: Cell<u32>,
}

impl<'fb> BorrowedDummy<'fb> {
    fn new(buf: &'fb mut [u8]) -> Self {
        Self {
            buf,
            fills: Cell::new(0),
        }
    }
}

impl<'fb> Canvas for BorrowedDummy<'fb> {
    fn fill_path(
        &mut self,
        _: &Path,
        _: &Rect,
        _: &Paint,
        _: u8,
        _: ::mirui::render::raster::FillRule,
    ) {
        // Touch the borrowed buffer so the lifetime actually matters at the
        // call site — otherwise `'fb` could be optimised away and the test
        // would be vacuous.
        if !self.buf.is_empty() {
            self.buf[0] = self.buf[0].wrapping_add(1);
        }
        self.fills.set(self.fills.get() + 1);
    }
    fn stroke_path(
        &mut self,
        _: &Path,
        _: &Rect,
        _: Fixed,
        _: &Paint,
        _: u8,
        _: ::mirui::render::raster::LineCap,
        _: ::mirui::render::raster::LineJoin,
        _: ::mirui::types::Fixed,
        _: &[Fixed],
    ) {
    }
    fn blit(
        &mut self,
        _: &Texture,
        _: &Rect,
        _: Point,
        _: Point,
        _: &Rect,
        _: u8,
        _: Fixed,
        _: CompositeMode,
    ) {
    }
    fn clear(&mut self, _: &Rect, _: &Color) {}
    fn draw_glyph_run(
        &mut self,
        _: &Point,
        _: &[mirui::text::PositionedGlyph],
        _: &mirui::render::font::Font,
        _: &Rect,
        _: &Color,
        _: u8,
    ) {
    }
    fn draw_posed_glyph_run(
        &mut self,
        _: &Point,
        _: mirui::render::PosedGlyphs<'_>,
        _: &mirui::render::font::Font,
        _: &Rect,
        _: &Color,
        _: u8,
    ) {
    }
    fn flush(&mut self) {}
}

impl Renderer for BorrowedDummy<'_> {
    fn route(&self, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        request.validate()?;
        Ok(RenderRoute::Native)
    }

    fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        self.route(request)?;
        if let mirui::render::DrawCommand::FillPath {
            path,
            paint,
            opa,
            fill_rule,
            ..
        } = request.command
        {
            self.fill_path(path, &request.clip, paint, *opa, *fill_rule);
        }
        Ok(())
    }

    fn flush(&mut self) {
        Canvas::flush(self);
    }
}

struct PlainDummy;
impl Canvas for PlainDummy {
    fn fill_path(
        &mut self,
        _: &Path,
        _: &Rect,
        _: &Paint,
        _: u8,
        _: ::mirui::render::raster::FillRule,
    ) {
    }
    fn stroke_path(
        &mut self,
        _: &Path,
        _: &Rect,
        _: Fixed,
        _: &Paint,
        _: u8,
        _: ::mirui::render::raster::LineCap,
        _: ::mirui::render::raster::LineJoin,
        _: ::mirui::types::Fixed,
        _: &[Fixed],
    ) {
    }
    fn blit(
        &mut self,
        _: &Texture,
        _: &Rect,
        _: Point,
        _: Point,
        _: &Rect,
        _: u8,
        _: Fixed,
        _: CompositeMode,
    ) {
    }
    fn clear(&mut self, _: &Rect, _: &Color) {}
    fn draw_glyph_run(
        &mut self,
        _: &Point,
        _: &[mirui::text::PositionedGlyph],
        _: &mirui::render::font::Font,
        _: &Rect,
        _: &Color,
        _: u8,
    ) {
    }
    fn draw_posed_glyph_run(
        &mut self,
        _: &Point,
        _: mirui::render::PosedGlyphs<'_>,
        _: &mirui::render::font::Font,
        _: &Rect,
        _: &Color,
        _: u8,
    ) {
    }
    fn flush(&mut self) {}
}

impl<'a> RenderEngine<BorrowedDummy<'a>> for PlainDummy {
    fn route(
        &self,
        target: &BorrowedDummy<'_>,
        request: &DrawRequest<'_, '_>,
    ) -> Result<RenderRoute, RenderError> {
        target.route(request)
    }

    fn submit(
        &mut self,
        target: &mut BorrowedDummy<'_>,
        request: &DrawRequest<'_, '_>,
    ) -> Result<(), RenderError> {
        target.submit(request)
    }
}

compose_backend! {
    pub struct HybridWithLifetime {
        borrowed: BorrowedDummy,
        plain: PlainDummy,
    }
    route {
        default => borrowed,
        blit => plain,
    }
}

#[test]
fn hybrid_is_a_renderer_and_dispatches_drawcommands() {
    // Verifies the Renderer impl the macro emits alongside Canvas.
    // Sending a DrawCommand::Blit should reach the field that owns `blit`
    // in the route table.
    use mirui::render::DrawCommand;
    use mirui::render::renderer::{DrawRequest, Renderer};

    let mut h = fresh_hybrid();
    let mut buf = dummy_texture_buf();
    let tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888);
    let rect = zero_rect();

    let command = DrawCommand::Blit {
        pos: Point::ZERO,
        size: Point::new(Fixed::from_int(4), Fixed::from_int(4)),
        transform: Transform::IDENTITY,
        quad: None,
        texture: &tex,
        opa: 255,
        radius: Fixed::ZERO,
        composite: mirui::render::CompositeMode::SourceOver,
    };
    h.submit(&DrawRequest::new(&command, rect)).unwrap();

    assert_eq!(h.sw.counts.blit.get(), 1);
    assert_eq!(h.gpu.counts.begin, 1);
    assert_eq!(h.gpu.counts.submit, 1);
    assert_eq!(h.gpu.counts.end, 1);
}

#[test]
fn engine_begin_failure_refuses_before_target_submission() {
    let mut h = fresh_hybrid();
    h.gpu.reject_begin = true;
    let mut buf = dummy_texture_buf();
    let tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888);
    let rect = zero_rect();
    let command = mirui::render::DrawCommand::Blit {
        pos: Point::ZERO,
        size: Point::new(4, 4),
        transform: Transform::IDENTITY,
        quad: None,
        texture: &tex,
        opa: 255,
        radius: Fixed::ZERO,
        composite: CompositeMode::SourceOver,
    };

    assert_eq!(
        h.submit(&DrawRequest::new(&command, rect)),
        Err(RenderError::BackendFailure)
    );
    assert_eq!(h.gpu.counts.begin, 1);
    assert_eq!(h.gpu.counts.submit, 0);
    assert_eq!(h.gpu.counts.end, 0);
    assert_eq!(h.sw.counts.blit.get(), 0);
}

#[test]
fn hybrid_accepts_backend_with_lifetime_parameter() {
    let mut buf = [0u8; 8];
    let borrowed = BorrowedDummy::new(&mut buf);
    let plain = PlainDummy;

    // The hybrid type itself takes two generic parameters — no lifetime on
    // Hybrid, the borrow in BorrowedDummy<'_> gets threaded through the
    // generic slot.
    let mut h: HybridWithLifetime<BorrowedDummy<'_>, PlainDummy> =
        HybridWithLifetime::new(borrowed, plain);

    let rect = Rect::new(0, 0, 4, 4);
    let path = Path::new();
    let paint = Paint::Color(Color::rgb(0, 0, 0).into());
    h.fill_path(
        &path,
        &rect,
        &paint,
        255,
        ::mirui::render::raster::FillRule::EvenOdd,
    );

    assert_eq!(h.borrowed.fills.get(), 1);
    // Side effect through the borrowed slice proves the lifetime really is
    // being honoured end-to-end.
    assert_eq!(h.borrowed.buf[0], 1);
}
