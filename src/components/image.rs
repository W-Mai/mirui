use crate::draw::command::DrawCommand;
use crate::draw::renderer::Renderer;
use crate::draw::texture::Texture;
use crate::ecs::{Entity, World};
use crate::types::{Point, Rect};
use crate::widget::view::{View, ViewCtx};

pub struct Image {
    pub texture: &'static Texture<'static>,
}

impl Image {
    pub fn new(texture: &'static Texture<'static>) -> Self {
        Self { texture }
    }
}

fn image_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(img) = world.get::<Image>(entity) else {
        return;
    };
    renderer.draw(
        &DrawCommand::Blit {
            pos: Point {
                x: rect.x,
                y: rect.y,
            },
            size: Point {
                x: rect.w,
                y: rect.h,
            },
            transform: ctx.transform,
            quad: ctx.quad,
            texture: img.texture,
        },
        ctx.clip,
    );
}

pub fn view() -> View {
    View {
        name: "Image",
        priority: 70,
        render: image_render,
        auto_attach: None,
    }
}
