use alloc::borrow::Cow;

use crate::core::resource::ResourceManager;
use crate::ecs::{Entity, World};
use crate::render::command::DrawCommand;
use crate::render::renderer::Renderer;
use crate::render::texture::Texture;
use crate::types::{Point, Rect};
use crate::ui::view::{View, ViewCtx};

#[derive(crate::Component)]
pub struct Image {
    pub src: Cow<'static, str>,
}

impl Image {
    pub fn new(src: impl Into<Cow<'static, str>>) -> Self {
        Self { src: src.into() }
    }

    pub fn build(src: impl Into<Cow<'static, str>>) -> ImageBuilder {
        ImageBuilder {
            image: Image::new(src),
            style: None,
        }
    }
}

pub struct ImageBuilder {
    image: Image,
    style: Option<crate::ui::Style>,
}

impl ImageBuilder {
    pub fn style(mut self, style: crate::ui::Style) -> Self {
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
    crate::trace_span!("image.render", {
        let img = crate::trace_span!("image.world_get", { world.get::<Image>(entity) });
        let Some(img) = img else { return };
        let mgr = crate::trace_span!("image.world_resource", {
            world.resource::<ResourceManager<Texture<'static>>>()
        });
        let Some(mgr) = mgr else { return };
        let rc = crate::trace_span!("image.resolve", { mgr.resolve(&img.src) });
        crate::trace_span!("image.blit", {
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
                    texture: &rc,
                    opa: 255,
                },
                ctx.clip,
            );
        });
        crate::trace_span!("image.rc_drop", { drop(rc) });
    });
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
        let e = Image::build("thumbs_up")
            .style(crate::ui::Style::default())
            .spawn(&mut world);
        assert!(world.has::<Image>(e));
        assert!(world.has::<crate::ui::Style>(e));
        assert!(world.has::<crate::ui::Widget>(e));
    }

    #[test]
    fn build_without_style_omits_it() {
        let mut world = World::new();
        let e = Image::build("thumbs_up").spawn(&mut world);
        assert!(world.has::<Image>(e));
        assert!(!world.has::<crate::ui::Style>(e));
    }

    #[test]
    fn new_accepts_str_and_string() {
        let a = Image::new("static");
        assert_eq!(a.src, "static");
        let owned: alloc::string::String = "owned".into();
        let b = Image::new(owned);
        assert_eq!(b.src, "owned");
    }
}
