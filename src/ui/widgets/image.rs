use alloc::borrow::Cow;
use core::cell::RefCell;

use crate::core::resource::{ResourceHandle, ResourceManager};
use crate::ecs::{Entity, World};
use crate::render::command::{CompositeMode, DrawCommand};
use crate::render::path::Path;
use crate::render::renderer::Renderer;
use crate::render::texture::Texture;
use crate::types::{Color, Fixed, Point, Rect, Transform};
use crate::ui::theme::{ColorToken, ThemedColor};
use crate::ui::view::{View, ViewCtx};

#[derive(Clone, Debug)]
pub enum ImageSource {
    Texture(Cow<'static, str>),
    Vector(Path),
}

impl Default for ImageSource {
    fn default() -> Self {
        Self::Texture(Cow::Borrowed(""))
    }
}

impl From<Cow<'static, str>> for ImageSource {
    fn from(value: Cow<'static, str>) -> Self {
        Self::Texture(value)
    }
}

impl From<&'static str> for ImageSource {
    fn from(value: &'static str) -> Self {
        Self::Texture(Cow::Borrowed(value))
    }
}

impl From<alloc::string::String> for ImageSource {
    fn from(value: alloc::string::String) -> Self {
        Self::Texture(Cow::Owned(value))
    }
}

impl From<Path> for ImageSource {
    fn from(value: Path) -> Self {
        Self::Vector(value)
    }
}

impl From<crate::ui::icons::IconAsset> for ImageSource {
    fn from(value: crate::ui::icons::IconAsset) -> Self {
        Self::Vector(value.into())
    }
}

#[derive(crate::Component)]
pub struct Image {
    pub src: ImageSource,
    pub composite: CompositeMode,
    pub radius: Fixed,
    pub color: ThemedColor,
    pub viewbox: Fixed,
    pub scale: Fixed,
}

impl Default for Image {
    fn default() -> Self {
        Self {
            src: ImageSource::default(),
            composite: CompositeMode::SourceOver,
            radius: Fixed::ZERO,
            color: ThemedColor::Token(ColorToken::OnSurface),
            viewbox: Fixed::from_int(24),
            scale: Fixed::ONE,
        }
    }
}

impl Image {
    pub fn new(src: impl Into<ImageSource>) -> Self {
        Self {
            src: src.into(),
            ..Self::default()
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

    pub fn with_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.color = color.into();
        self
    }

    pub fn with_viewbox(mut self, viewbox: impl Into<Fixed>) -> Self {
        self.viewbox = viewbox.into();
        self
    }

    pub fn with_scale(mut self, scale: impl Into<Fixed>) -> Self {
        self.scale = scale.into();
        self
    }

    pub fn build(src: impl Into<ImageSource>) -> ImageBuilder {
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
    let handle = world
        .get::<Image>(entity)
        .and_then(|image| match &image.src {
            ImageSource::Texture(token) => world
                .resource::<ResourceManager<Texture<'static>>>()
                .map(|manager| manager.load(token.clone())),
            ImageSource::Vector(_) => None,
        });
    world.insert(entity, ImageResource(RefCell::new(handle)));
}

fn resolve_image(
    world: &World,
    entity: Entity,
    image: &Image,
) -> Option<alloc::rc::Rc<Texture<'static>>> {
    let ImageSource::Texture(token) = &image.src else {
        return None;
    };
    if let Some(cache) = world.get::<ImageResource>(entity) {
        let mut handle = cache.0.borrow_mut();
        if let Some(current) = handle.as_ref()
            && current.token() == token.as_ref()
            && let Some(texture) = current.get_cached()
        {
            return Some(texture);
        }
        let manager = world.resource::<ResourceManager<Texture<'static>>>()?;
        if handle
            .as_ref()
            .is_none_or(|current| current.token() != token.as_ref())
        {
            *handle = Some(manager.load(token.clone()));
        }
        return handle.as_ref().map(ResourceHandle::get);
    }
    world
        .resource::<ResourceManager<Texture<'static>>>()
        .map(|manager| manager.resolve(token))
}

pub(crate) fn draw_vector_transformed(
    renderer: &mut dyn Renderer,
    ctx: &mut ViewCtx,
    path: &Path,
    color: Color,
    transform: Transform,
) {
    let paint = crate::render::canvas::Paint::Color(color.into());
    ctx.draw(
        renderer,
        &DrawCommand::FillPath {
            path,
            transform,
            paint: &paint,
            opa: 255,
            fill_rule: crate::render::raster::FillRule::EvenOdd,
        },
        ctx.clip,
    );
}

pub(crate) fn vector_transform(
    rect: &Rect,
    viewbox: Fixed,
    rendered_size: Fixed,
    parent: Transform,
) -> Option<Transform> {
    if viewbox <= Fixed::ZERO || rendered_size <= Fixed::ZERO {
        return None;
    }
    let x = rect.x + (rect.w - rendered_size) / Fixed::from_int(2);
    let y = rect.y + (rect.h - rendered_size) / Fixed::from_int(2);
    Some(
        parent
            .compose(&Transform::translate(x, y))
            .compose(&Transform::scale(
                rendered_size / viewbox,
                rendered_size / viewbox,
            )),
    )
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
        if let ImageSource::Vector(path) = &img.src {
            let size = rect.w.min(rect.h) * img.scale;
            let Some(transform) = vector_transform(rect, img.viewbox, size, ctx.transform) else {
                return;
            };
            draw_vector_transformed(
                renderer,
                ctx,
                path,
                img.color.resolve_in(ctx.theme(world), ctx.state),
                transform,
            );
            return;
        }
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
        assert!(matches!(a.src, ImageSource::Texture(ref token) if token == "static"));
        let owned: alloc::string::String = "owned".into();
        let b = Image::new(owned);
        assert!(matches!(b.src, ImageSource::Texture(ref token) if token == "owned"));
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

        world.get_mut::<Image>(entity).unwrap().src = ImageSource::from("missing");
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
    fn new_accepts_borrowed_vector_paths_without_allocating_commands() {
        use crate::render::path::PathCmd;

        static COMMANDS: &[PathCmd] = &[PathCmd::Close];
        let image = Image::new(Path::from_static(COMMANDS));
        assert!(matches!(image.src, ImageSource::Vector(ref path) if path.is_borrowed()));
    }

    #[test]
    fn new_accepts_typed_icon_assets_without_allocating_commands() {
        let image = Image::new(crate::ui::icons::ICON_HOME);
        assert!(matches!(image.src, ImageSource::Vector(ref path) if path.is_borrowed()));
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
