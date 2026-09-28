use crate::prelude::draw::*;
use crate::prelude::*;
use crate::render::renderer::Renderer;
use crate::render::scene::{LineCap, LineJoin, Paint};
use crate::types::Transform;
use crate::ui::view::{View, ViewCtx};

pub(super) static DIAMOND_PATH: Path = path!(M 50 0 L 100 50 L 50 100 L 0 50 Z);

pub struct Diamond {
    pub color: Color,
    pub line_width: Fixed,
}

impl Default for Diamond {
    fn default() -> Self {
        Self {
            color: Color::rgb(255, 255, 255),
            line_width: Fixed::from_int(1),
        }
    }
}

fn diamond_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(d) = world.get::<Diamond>(entity) else {
        return;
    };
    let transform = ctx
        .transform
        .compose(&Transform::translate(rect.x, rect.y))
        .compose(&Transform::scale(
            rect.w / Fixed::from_int(100),
            rect.h / Fixed::from_int(100),
        ));
    let paint = Paint::Color(d.color.into());
    let dash: [Fixed; 0] = [];
    ctx.draw(
        renderer,
        &DrawCommand::StrokePath {
            path: &DIAMOND_PATH,
            transform,
            paint: &paint,
            width: d.line_width,
            opa: 255,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            miter_limit: Fixed::from_int(4),
            dash: &dash,
        },
        ctx.clip,
    );
}

pub fn diamond_view() -> View {
    View::new("Diamond", 60, diamond_render)
}

pub const PALETTE: [Color; 3] = [
    Color::rgb(244, 167, 89),
    Color::rgb(140, 211, 255),
    Color::rgb(190, 240, 140),
];
