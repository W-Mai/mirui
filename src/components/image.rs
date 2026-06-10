use crate::draw::command::DrawCommand;
use crate::draw::renderer::Renderer;
use crate::draw::texture::Texture;
use crate::ecs::{Entity, World};
use crate::types::{Point, Rect};
use crate::widget::view::{View, ViewCtx};

#[derive(crate::Component)]
pub struct Image {
    pub texture: &'static Texture<'static>,
}

impl Image {
    pub fn new(texture: &'static Texture<'static>) -> Self {
        Self { texture }
    }

    pub fn build(texture: &'static Texture<'static>) -> ImageBuilder {
        ImageBuilder {
            image: Image::new(texture),
            style: None,
        }
    }
}

pub struct ImageBuilder {
    image: Image,
    style: Option<crate::widget::Style>,
}

impl ImageBuilder {
    pub fn style(mut self, style: crate::widget::Style) -> Self {
        self.style = Some(style);
        self
    }

    pub fn spawn(self, world: &mut World) -> Entity {
        world.spawn(self)
    }
}

impl crate::ecs::IntoBundle for ImageBuilder {
    fn spawn_into(self, world: &mut World, entity: Entity) {
        world.insert(entity, self.image);
        if let Some(style) = self.style {
            world.insert(entity, style);
        }
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
    View::new("Image", 70, image_render).with_filter::<Image>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_spawns_image_with_style() {
        let mut world = World::new();
        let e = Image::build(&crate::components::assets::IMG_THUMBS_UP)
            .style(crate::widget::Style::default())
            .spawn(&mut world);
        assert!(world.has::<Image>(e));
        assert!(world.has::<crate::widget::Style>(e));
        assert!(world.has::<crate::widget::Widget>(e));
    }

    #[test]
    fn build_without_style_omits_it() {
        let mut world = World::new();
        let e = Image::build(&crate::components::assets::IMG_THUMBS_UP).spawn(&mut world);
        assert!(world.has::<Image>(e));
        assert!(!world.has::<crate::widget::Style>(e));
    }
}
