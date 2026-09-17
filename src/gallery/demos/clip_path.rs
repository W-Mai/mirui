#![allow(clippy::needless_update)]

use crate::prelude::draw::*;
use crate::prelude::*;
use crate::render::raster::FillRule;
use crate::render::renderer::DrawRequest;
use crate::render::scene::Paint;
use crate::types::Transform;
use crate::ui::Theme;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

const LOGICAL_SIZE: i32 = 320;

fn canvas_transform(rect: &Rect, parent: Transform) -> Transform {
    let scale =
        (rect.w / Fixed::from_int(LOGICAL_SIZE)).min(rect.h / Fixed::from_int(LOGICAL_SIZE));
    let size = Fixed::from_int(LOGICAL_SIZE) * scale;
    let x = rect.x + (rect.w - size) / Fixed::from_int(2);
    let y = rect.y + (rect.h - size) / Fixed::from_int(2);
    parent
        .compose(&Transform::translate(x, y))
        .compose(&Transform::scale(scale, scale))
}

#[derive(Default)]
pub struct ClipPath;

static CLIP_CIRCLE: Path = path!(
    M 208 104
    C 208 161.438 161.438 208 104 208
    C 46.562 208 0 161.438 0 104
    C 0 46.562 46.562 0 104 0
    C 161.438 0 208 46.562 208 104
    Z
);

fn fill_rect(
    renderer: &mut dyn Renderer,
    ctx: &mut ViewCtx,
    area: Rect,
    color: Color,
    transform: Transform,
) {
    ctx.draw(
        renderer,
        &DrawCommand::Fill {
            area,
            transform,
            quad: None,
            color,
            radius: Fixed::from_int(10),
            opa: 255,
        },
        ctx.clip,
    );
}

fn clip_path_render(
    renderer: &mut dyn Renderer,
    world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let canvas = canvas_transform(rect, ctx.transform);
    let default_theme = Theme::default();
    let theme = world.resource::<Theme>().unwrap_or(&default_theme);
    let surface = theme.resolve(ColorToken::SurfaceVariant);
    let foreground = theme.resolve(ColorToken::OnSurface);
    let grays = [
        surface,
        surface.blend_with(foreground, Fixed::from_ratio(1, 12)),
        surface.blend_with(foreground, Fixed::from_ratio(1, 6)),
        surface.blend_with(foreground, Fixed::from_ratio(1, 4)),
    ];
    let colors = [
        theme.resolve(ColorToken::Primary),
        theme.resolve(ColorToken::Secondary),
        theme.resolve(ColorToken::Tertiary),
        theme.resolve(ColorToken::Success),
    ];

    for (i, color) in grays.into_iter().enumerate() {
        fill_rect(
            renderer,
            ctx,
            Rect::new(30, 42 + i as i32 * 56, 260, 42),
            color,
            canvas,
        );
    }
    if ctx.error.is_some() {
        return;
    }

    let circle_transform = canvas.compose(&Transform::translate(
        Fixed::from_int(56),
        Fixed::from_int(56),
    ));
    ctx.draw(
        renderer,
        &DrawCommand::PushClip {
            path: &CLIP_CIRCLE,
            transform: circle_transform,
            fill_rule: FillRule::EvenOdd,
        },
        ctx.clip,
    );
    if ctx.error.is_some() {
        return;
    }
    for (i, color) in colors.into_iter().enumerate() {
        fill_rect(
            renderer,
            ctx,
            Rect::new(30, 42 + i as i32 * 56, 260, 42),
            color,
            canvas,
        );
    }
    ctx.record(renderer.submit(&DrawRequest::new(&DrawCommand::PopClip, *ctx.clip)));

    let outline = Paint::Color(theme.resolve(ColorToken::OnSurface).into());
    ctx.draw(
        renderer,
        &DrawCommand::StrokePath {
            path: &CLIP_CIRCLE,
            transform: circle_transform,
            paint: &outline,
            width: Fixed::from_int(2),
            opa: 220,
            line_cap: crate::render::scene::LineCap::Round,
            line_join: crate::render::scene::LineJoin::Round,
            miter_limit: Fixed::from_int(4),
            dash: &[],
        },
        ctx.clip,
    );
}

pub fn clip_path_view() -> View {
    View::new("ClipPath", 60, clip_path_render).with_filter::<ClipPath>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::renderer::{RenderError, RenderRoute};
    use crate::ui::theme::WidgetState;

    #[derive(Default)]
    struct FailingRenderer {
        pushed: bool,
        pops: usize,
    }

    impl Renderer for FailingRenderer {
        fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
            Ok(RenderRoute::Native)
        }

        fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
            if self.pushed && matches!(request.command, DrawCommand::Fill { .. }) {
                return Err(RenderError::BackendFailure);
            }
            match request.command {
                DrawCommand::PushClip { .. } => self.pushed = true,
                DrawCommand::PopClip => {
                    self.pushed = false;
                    self.pops += 1;
                }
                _ => {}
            }
            Ok(())
        }

        fn flush(&mut self) {}
    }

    #[test]
    fn failed_content_still_pops_clip() {
        let mut renderer = FailingRenderer::default();
        let mut world = World::new();
        let entity = world.spawn_empty();
        let style = Style::default();
        let clip = Rect::new(0, 0, 320, 320);
        let mut ctx = ViewCtx {
            style: &style,
            transform: Transform::IDENTITY,
            quad: None,
            clip: &clip,
            bg_handled: false,
            state: WidgetState::Enabled,
            error: None,
        };

        clip_path_render(&mut renderer, &world, entity, &clip, &mut ctx);

        assert_eq!(ctx.error, Some(RenderError::BackendFailure));
        assert!(!renderer.pushed);
        assert_eq!(renderer.pops, 1);
    }

    #[test]
    fn clip_geometry_stays_in_static_storage() {
        assert!(CLIP_CIRCLE.is_borrowed());
    }

    #[test]
    fn logical_canvas_stays_inside_phone_bounds() {
        let rect = Rect::new(8, 72, 304, 480);
        let transform = canvas_transform(&rect, Transform::IDENTITY);
        let top_left = transform.apply_point(Point::ZERO);
        let bottom_right = transform.apply_point(Point::new(
            Fixed::from_int(LOGICAL_SIZE),
            Fixed::from_int(LOGICAL_SIZE),
        ));
        assert!(top_left.x >= rect.x && top_left.y >= rect.y);
        assert!(bottom_right.x <= rect.x + rect.w);
        assert!(bottom_right.y <= rect.y + rect.h);
    }
}

#[compose]
pub fn build_widgets() {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(16),
            row_gap: 6,
            bg_color: ColorToken::Surface
        ) {
            Text (
                "CLIP PATH",
                height: 28,
                font_size: 18,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "one vector boundary, four semantic layers",
                height: 20,
                font_size: 11,
                text_color: ColorToken::OnSurfaceVariant,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            ClipPath (
                grow: 1.0,
                width: Dimension::percent(100),
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 18
            )
        }
    };
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.with_widget(clip_path_view());
    app.compose(parent, build_widgets);
}
