use alloc::borrow::Cow;
use core::cell::RefCell;

use crate::core::resource::{ResourceHandle, ResourceManager};
use crate::ecs::{Entity, World};
use crate::render::command::{CompositeMode, DrawCommand};
use crate::render::renderer::Renderer;
use crate::render::texture::Texture;
use crate::types::{Fixed, Point, Rect};
use crate::ui::view::{View, ViewCtx};

#[derive(crate::Component)]
pub struct Image {
    pub src: Cow<'static, str>,
    pub composite: CompositeMode,
    pub radius: Fixed,
}

impl Default for Image {
    fn default() -> Self {
        Self {
            src: Cow::Borrowed(""),
            composite: CompositeMode::SourceOver,
            radius: Fixed::ZERO,
        }
    }
}

impl Image {
    pub fn new(src: impl Into<Cow<'static, str>>) -> Self {
        Self {
            src: src.into(),
            composite: CompositeMode::SourceOver,
            radius: Fixed::ZERO,
        }
    }

    pub fn with_composite(mut self, mode: CompositeMode) -> Self {
        self.composite = mode;
        self
    }

    pub fn with_radius(mut self, radius: impl Into<Fixed>) -> Self {
        self.radius = radius.into();
        self
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

struct ImageResource(RefCell<Option<ResourceHandle<Texture<'static>>>>);

pub(crate) fn attach_image_resource(world: &mut World, entity: Entity) {
    if !world.has::<Image>(entity) || world.has::<ImageResource>(entity) {
        return;
    }
    let handle = world.get::<Image>(entity).and_then(|image| {
        world
            .resource::<ResourceManager<Texture<'static>>>()
            .map(|manager| manager.load(image.src.clone()))
    });
    world.insert(entity, ImageResource(RefCell::new(handle)));
}

fn resolve_image(
    world: &World,
    entity: Entity,
    image: &Image,
) -> Option<alloc::rc::Rc<Texture<'static>>> {
    if let Some(cache) = world.get::<ImageResource>(entity) {
        let mut handle = cache.0.borrow_mut();
        if let Some(current) = handle.as_ref()
            && current.token() == image.src.as_ref()
            && let Some(texture) = current.get_cached()
        {
            return Some(texture);
        }
        let manager = world.resource::<ResourceManager<Texture<'static>>>()?;
        if handle
            .as_ref()
            .is_none_or(|current| current.token() != image.src.as_ref())
        {
            *handle = Some(manager.load(image.src.clone()));
        }
        return handle.as_ref().map(ResourceHandle::get);
    }
    world
        .resource::<ResourceManager<Texture<'static>>>()
        .map(|manager| manager.resolve(&image.src))
}

impl ImageBuilder {
    pub fn style(mut self, style: crate::ui::Style) -> Self {
        self.style = Some(style);
        self
    }

    pub fn composite(mut self, mode: CompositeMode) -> Self {
        self.image.composite = mode;
        self
    }

    pub fn radius(mut self, radius: impl Into<Fixed>) -> Self {
        self.image.radius = radius.into();
        self
    }

    pub fn spawn(self, world: &mut World) -> Entity {
        world.spawn(self)
    }
}

impl crate::ecs::IntoBundle for ImageBuilder {
    fn spawn_into(self, world: &mut World, entity: Entity) {
        world.insert(entity, self.image);
        attach_image_resource(world, entity);
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
        let rc = crate::trace_span!("image.resolve", { resolve_image(world, entity, img) });
        let Some(rc) = rc else { return };
        crate::trace_span!("image.blit", {
            ctx.draw(
                renderer,
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
                    radius: img.radius,
                    composite: img.composite,
                },
                ctx.clip,
            );
        });
        crate::trace_span!("image.rc_drop", { drop(rc) });
    });
}

pub fn view() -> View {
    View::new("Image", 70, image_render)
        .with_filter::<Image>()
        .with_attach(attach_image_resource)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::cache::MaxSize;
    use crate::render::texture::ColorFormat;

    static RED: [u8; 4] = [255, 0, 0, 255];
    static BLUE: [u8; 4] = [0, 0, 255, 255];

    fn image_manager() -> ResourceManager<Texture<'static>> {
        ResourceManager::new(
            MaxSize::Bytes(64),
            Texture::from_static(&RED, 1, 1, ColorFormat::RGBA8888),
        )
        .with_static(
            "blue",
            Texture::from_static(&BLUE, 1, 1, ColorFormat::RGBA8888),
        )
    }

    #[test]
    fn build_spawns_image_with_style() {
        let mut world = World::new();
        let e = Image::build("thumbs_up")
            .style(crate::ui::Style::default())
            .spawn(&mut world);
        assert!(world.has::<Image>(e));
        assert!(world.has::<ImageResource>(e));
        assert!(world.has::<crate::ui::Style>(e));
        assert!(world.has::<crate::ui::Widget>(e));
    }

    #[test]
    fn build_without_style_omits_it() {
        let mut world = World::new();
        let e = Image::build("thumbs_up").spawn(&mut world);
        assert!(world.has::<Image>(e));
        assert!(world.has::<ImageResource>(e));
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

    #[test]
    fn attached_image_reuses_lease_and_tracks_token_changes() {
        let mut world = World::new();
        world.insert_resource(image_manager());
        let entity = world.spawn(Image::new("blue"));
        attach_image_resource(&mut world, entity);

        let image = world.get::<Image>(entity).unwrap();
        let first = resolve_image(&world, entity, image).unwrap();
        let second = resolve_image(&world, entity, image).unwrap();
        assert!(alloc::rc::Rc::ptr_eq(&first, &second));
        assert_eq!(first.buf.as_slice(), &BLUE);

        world.get_mut::<Image>(entity).unwrap().src = Cow::Borrowed("missing");
        let image = world.get::<Image>(entity).unwrap();
        let fallback = resolve_image(&world, entity, image).unwrap();
        assert_eq!(fallback.buf.as_slice(), &RED);
        assert_eq!(
            world
                .get::<ImageResource>(entity)
                .unwrap()
                .0
                .borrow()
                .as_ref()
                .unwrap()
                .token(),
            "missing"
        );
    }

    #[test]
    fn unresolved_component_keeps_the_direct_manager_path() {
        let mut world = World::new();
        world.insert_resource(image_manager());
        let entity = world.spawn_empty();
        world.insert(entity, Image::new("blue"));

        let image = world.get::<Image>(entity).unwrap();
        assert_eq!(
            resolve_image(&world, entity, image).unwrap().buf.as_slice(),
            &BLUE
        );
    }
}
