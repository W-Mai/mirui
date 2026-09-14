#![allow(clippy::needless_update)]

use crate::prelude::draw::*;
use crate::prelude::*;
use crate::render::raster::FillRule;
use crate::render::renderer::DrawRequest;
use crate::render::scene::Paint;
use crate::types::Transform;

#[derive(Default)]
pub struct ClipPath;

fn circle_path(cx: Fixed, cy: Fixed, r: Fixed) -> Path {
    let k = r * Fixed::from_f32(0.552_284_8);
    let mut path = Path::new();
    path.move_to(Point { x: cx + r, y: cy });
    path.cubic_to(
        Point {
            x: cx + r,
            y: cy + k,
        },
        Point {
            x: cx + k,
            y: cy + r,
        },
        Point { x: cx, y: cy + r },
    );
    path.cubic_to(
        Point {
            x: cx - k,
            y: cy + r,
        },
        Point {
            x: cx - r,
            y: cy + k,
        },
        Point { x: cx - r, y: cy },
    );
    path.cubic_to(
        Point {
            x: cx - r,
            y: cy - k,
        },
        Point {
            x: cx - k,
            y: cy - r,
        },
        Point { x: cx, y: cy - r },
    );
    path.cubic_to(
        Point {
            x: cx + k,
            y: cy - r,
        },
        Point {
            x: cx + r,
            y: cy - k,
        },
        Point { x: cx + r, y: cy },
    );
    path.close();
    path
}

fn fill_rect(
    renderer: &mut dyn Renderer,
    ctx: &mut ViewCtx,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: Color,
) {
    ctx.draw(
        renderer,
        &DrawCommand::Fill {
            area: Rect::new(
                Fixed::from_int(x),
                Fixed::from_int(y),
                Fixed::from_int(w),
                Fixed::from_int(h),
            ),
            transform: Transform::IDENTITY,
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
    _world: &World,
    _entity: Entity,
    _rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let grays = [
        Color::rgb(58, 60, 68),
        Color::rgb(72, 74, 82),
        Color::rgb(86, 88, 96),
        Color::rgb(100, 102, 110),
    ];
    let colors = [
        Color::rgb(255, 90, 110),
        Color::rgb(255, 190, 80),
        Color::rgb(80, 210, 160),
        Color::rgb(80, 145, 255),
    ];

    for (i, color) in grays.into_iter().enumerate() {
        fill_rect(renderer, ctx, 30, 42 + i as i32 * 56, 260, 42, color);
    }
    if ctx.error.is_some() {
        return;
    }

    let clip_path = circle_path(
        Fixed::from_int(160),
        Fixed::from_int(160),
        Fixed::from_int(104),
    );
    ctx.draw(
        renderer,
        &DrawCommand::PushClip {
            path: &clip_path,
            transform: Transform::IDENTITY,
            fill_rule: FillRule::EvenOdd,
        },
        ctx.clip,
    );
    if ctx.error.is_some() {
        return;
    }
    for (i, color) in colors.into_iter().enumerate() {
        fill_rect(renderer, ctx, 30, 42 + i as i32 * 56, 260, 42, color);
    }
    ctx.record(renderer.submit(&DrawRequest::new(&DrawCommand::PopClip, *ctx.clip)));

    let outline = Paint::Color(Color::rgb(230, 235, 245).into());
    ctx.draw(
        renderer,
        &DrawCommand::StrokePath {
            path: &clip_path,
            transform: Transform::IDENTITY,
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
            self.draw(request.command, &request.clip);
            Ok(())
        }

        fn draw(&mut self, command: &DrawCommand, _: &Rect) {
            match command {
                DrawCommand::PushClip { .. } => self.pushed = true,
                DrawCommand::PopClip => {
                    self.pushed = false;
                    self.pops += 1;
                }
                _ => {}
            }
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
}

#[compose]
pub fn build_widgets() {
    ui! {
        ClipPath (grow: 1.0)
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
