#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

use mirui::render::canvas::{Canvas, Paint};
use mirui::render::command::CompositeMode;
use mirui::render::font::Font;
use mirui::render::path::Path;
use mirui::render::raster::{FillRule, LineCap, LineJoin};
use mirui::render::texture::{ColorFormat, Texture};
use mirui::render::{PosedGlyphs, RendererFactory, SwRendererFactory};
use mirui::surface::framebuf::FramebufSurface;
use mirui::text::{FlowPoint, GlyphId, PositionedGlyph};
use mirui::types::{Color, Fixed, PhysicalRect, Point, Rect, Transform, Viewport};
use textflow::placement::GlyphFrame;
use tracking_allocator::tracked_allocations;

#[allow(clippy::too_many_arguments)]
fn render_frame<F: FnMut(&[u8], PhysicalRect)>(
    factory: &mut SwRendererFactory,
    surface: &mut FramebufSurface<F>,
    viewport: &Viewport,
    path: &Path,
    texture: &Texture<'_>,
    glyphs: &[PositionedGlyph],
    frames: &[GlyphFrame],
    font: &Font,
) {
    let clip = Rect::new(0, 0, 64, 64);
    let color = Color::rgb(72, 180, 240);
    let paint = Paint::Color(color.into());
    let mut renderer = factory.make(surface, viewport);

    renderer.clear(&clip, &Color::rgb(6, 10, 18));
    renderer.push_clip(path, &Transform::IDENTITY, FillRule::EvenOdd);
    renderer.fill_path(path, &clip, &paint, 220, FillRule::EvenOdd);
    renderer.stroke_path(
        path,
        &clip,
        Fixed::from_int(2),
        &paint,
        255,
        LineCap::Round,
        LineJoin::Round,
        Fixed::from_int(4),
        &[],
    );
    renderer.blit(
        texture,
        &Rect::new(0, 0, 2, 2),
        Point::new(4, 4),
        Point::new(8, 8),
        &clip,
        255,
        Fixed::ZERO,
        CompositeMode::SourceOver,
    );
    renderer.draw_glyph_run(
        &Point::new(8, 24),
        glyphs,
        font,
        &clip,
        &Color::rgb(240, 244, 255),
        255,
    );
    renderer.draw_posed_glyph_run(
        &Point::new(8, 40),
        PosedGlyphs::new(glyphs, frames).unwrap(),
        font,
        &clip,
        &Color::rgb(248, 190, 72),
        255,
    );
    renderer.pop_clip();
    renderer.flush();
}

#[test]
fn warmed_software_frame_reuses_all_render_storage() {
    static PIXELS: [u8; 16] = [
        255, 64, 64, 255, 64, 255, 64, 255, 64, 64, 255, 255, 255, 255, 255, 255,
    ];

    let mut surface = FramebufSurface::new(64, 64, |_, _| {});
    let mut factory = SwRendererFactory::new();
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    let path = Path::rounded_rect(
        Fixed::from_int(2),
        Fixed::from_int(2),
        Fixed::from_int(58),
        Fixed::from_int(58),
        Fixed::from_int(8),
    );
    let texture = Texture::from_static(&PIXELS, 2, 2, ColorFormat::RGBA8888);
    let glyphs = [PositionedGlyph::new(
        GlyphId::new(u16::from(b'A')),
        FlowPoint { x: 0, y: 0 },
    )];
    let frames = [GlyphFrame {
        local_origin: FlowPoint { x: 0, y: 0 },
        unit_tangent: FlowPoint { x: 1 << 8, y: 0 },
    }];
    let font = Font::bitmap_8x8();

    render_frame(
        &mut factory,
        &mut surface,
        &viewport,
        &path,
        &texture,
        &glyphs,
        &frames,
        &font,
    );
    let allocations = tracked_allocations(|| {
        render_frame(
            &mut factory,
            &mut surface,
            &viewport,
            &path,
            &texture,
            &glyphs,
            &frames,
            &font,
        );
    });

    assert_eq!(allocations, 0);
}
