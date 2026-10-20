use alloc::borrow::Cow;
use core::cell::RefCell;

use crate::core::resource::{ResourceHandle, ResourceManager};
use crate::ecs::{Entity, World};
use crate::render::command::{CompositeMode, DrawCommand};
use crate::render::path::Path;
use crate::render::texture::Texture;
use crate::types::{Color, Fixed, Point, Rect, Transform};
use crate::ui::theme::{ColorToken, ThemedColor};
use crate::ui::view::PaintCtx;

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

#[crate::component]
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
            properties: (),
            bindings: (),
        }
    }
}

pub struct ImageBuilder<P = (), B = ()> {
    image: Image,
    style: Option<crate::ui::Style>,
    properties: P,
    bindings: B,
}

#[crate::component]
struct ImageResource(RefCell<Option<ResourceHandle<Texture<'static>>>>);

#[crate::event]
pub(crate) fn attach_image_resource(
    entity: entity!(),
    images: query!(Image, optional ImageResource),
    manager: res!(optional ResourceManager<Texture<'static>>),
    commands: commands!(),
) {
    let Some((image, None)) = images.get(entity) else {
        return;
    };
    let handle = match &image.src {
        ImageSource::Texture(token) => manager.map(|manager| manager.load(token.clone())),
        ImageSource::Vector(_) => None,
    };
    commands.insert(entity, ImageResource(RefCell::new(handle)));
}

fn resolve_image(
    image: &Image,
    cache: Option<&ImageResource>,
    manager: Option<&ResourceManager<Texture<'static>>>,
) -> Option<alloc::rc::Rc<Texture<'static>>> {
    let ImageSource::Texture(token) = &image.src else {
        return None;
    };
    if let Some(cache) = cache {
        let mut handle = cache.0.borrow_mut();
        if let Some(current) = handle.as_ref()
            && current.token() == token.as_ref()
            && let Some(texture) = current.get_cached()
        {
            return Some(texture);
        }
        let manager = manager?;
        if handle
            .as_ref()
            .is_none_or(|current| current.token() != token.as_ref())
        {
            *handle = Some(manager.load(token.clone()));
        }
        return handle.as_ref().map(ResourceHandle::get);
    }
    manager.map(|manager| manager.resolve(token))
}

pub(crate) fn draw_vector_transformed(
    paint: &mut PaintCtx<'_>,
    path: &Path,
    color: Color,
    transform: Transform,
) {
    let fill = crate::render::canvas::Paint::Color(color.into());
    paint.draw(&DrawCommand::FillPath {
        path,
        transform,
        paint: &fill,
        opa: 255,
        fill_rule: crate::render::raster::FillRule::EvenOdd,
    });
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

impl<P, B> ImageBuilder<P, B> {
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
}

crate::ui::recipe::widget_builder_bindings!(ImageBuilder, Image, image, style);

impl<P: crate::__macro_support::PropertyBindings, B: crate::__macro_support::EntityEventBindings>
    crate::ui::recipe::WidgetRecipe for ImageBuilder<P, B>
{
    type Properties = P;
    type Events = B;

    fn insert(self, world: &mut World, entity: Entity) -> (P, B) {
        world.insert(entity, self.image);
        if let Some(style) = self.style {
            world.insert(entity, style);
        }
        (self.properties, self.bindings)
    }
}

#[crate::view(component = Image, name = "Image", priority = 70, attach = attach_image_resource::callback())]
fn image_render(
    rows: query!(Image, optional ImageResource),
    entity: entity!(),
    manager: res!(optional ResourceManager<Texture<'static>>),
    theme: res!(crate::ui::Theme),
    paint: paint!(),
) {
    crate::trace_span!("image.render", {
        let row = crate::trace_span!("image.query_get", { rows.get(entity) });
        let Some((img, cache)) = row else { return };
        let rect = paint.rect();
        if let ImageSource::Vector(path) = &img.src {
            let size = rect.w.min(rect.h) * img.scale;
            let Some(transform) = vector_transform(&rect, img.viewbox, size, paint.transform())
            else {
                return;
            };
            draw_vector_transformed(
                paint,
                path,
                img.color.resolve_in(&theme, paint.state()),
                transform,
            );
            return;
        }
        let rc = crate::trace_span!("image.resolve", {
            resolve_image(img, cache, manager.as_deref())
        });
        let Some(rc) = rc else { return };
        crate::trace_span!("image.blit", {
            paint.draw(&DrawCommand::Blit {
                pos: Point {
                    x: rect.x,
                    y: rect.y,
                },
                size: Point {
                    x: rect.w,
                    y: rect.h,
                },
                transform: paint.transform(),
                quad: paint.quad(),
                texture: &rc,
                opa: 255,
                radius: img.radius,
                composite: img.composite,
            });
        });
        crate::trace_span!("image.rc_drop", { drop(rc) });
    });
}

pub fn view() -> crate::ui::view::View {
    image_render::view().with_measurement(image_measurement)
}

fn image_measurement(
    world: &World,
    entity: Entity,
    rect: Rect,
    _source: &dyn Fn(Entity) -> Option<crate::ui::view::SourceGeometry>,
) -> Option<crate::ui::view::PaintMeasurement> {
    let image = world.get::<Image>(entity)?;
    match image.src {
        ImageSource::Texture(_) => Some(crate::ui::view::PaintMeasurement {
            coverage: crate::ui::dirty::PaintCoverage::solid(rect),
            workspace: None,
        }),
        ImageSource::Vector(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::cache::MaxSize;
    use crate::render::command::Paint;
    use crate::render::path::PathCmd;
    use crate::render::raster::FillRule;
    use crate::render::renderer::{DrawRequest, RenderError, Renderer};
    use crate::render::texture::ColorFormat;
    use crate::test_support::tracked_allocations;
    use crate::ui::theme::WidgetState;
    use crate::ui::{Style, Theme};

    static RED: [u8; 4] = [255, 0, 0, 255];
    static BLUE: [u8; 4] = [0, 0, 255, 255];

    #[test]
    fn texture_measurement_bounds_destination_without_resolving_source() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert(entity, Image::new("unloaded"));
        let measure = view().measurement().expect("image measurement");
        let rect = Rect {
            x: Fixed::from_f32(-2.5),
            y: Fixed::from_f32(3.25),
            w: Fixed::from_int(16),
            h: Fixed::from_int(12),
        };
        let mut measured = None;
        let allocations = tracked_allocations(|| {
            measured = measure(&world, entity, rect, &|_| unreachable!());
        });
        let measured = measured.expect("texture destination bounds");
        assert_eq!(allocations, 0);
        assert_eq!(measured.coverage.outer, rect);
        assert!(measured.workspace.is_none());
        let empty = world.spawn_empty();
        assert!(measure(&world, empty, rect, &|_| None).is_none());
    }

    #[test]
    fn vector_measurement_preserves_conservative_fallback() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert(entity, Image::new(Path::new()));
        let measure = view().measurement().expect("image measurement");
        assert!(measure(&world, entity, Rect::new(0, 0, 16, 16), &|_| None).is_none());
    }

    fn image_manager() -> ResourceManager<Texture<'static>> {
        ResourceManager::new(
            MaxSize::Bytes(64),
            Texture::from_static(&RED, 1, 1, ColorFormat::RGBA8888).expect("valid pixel storage"),
        )
        .with_static(
            "blue",
            Texture::from_static(&BLUE, 1, 1, ColorFormat::RGBA8888).expect("valid pixel storage"),
        )
    }

    #[test]
    fn build_spawns_image_with_style() {
        let mut app = crate::app::App::headless(32, 32);
        app.with_widget(view());
        let parent = app.spawn_root().id();
        let e = app.compose(parent, |tree| {
            tree.spawn(Image::build("thumbs_up").style(crate::ui::Style::default()))
        });
        let world = &app.world;
        assert!(world.has::<Image>(e));
        assert!(world.has::<ImageResource>(e));
        assert!(world.has::<crate::ui::Style>(e));
        assert!(world.has::<crate::ui::Widget>(e));
    }

    #[test]
    fn build_without_style_initializes_default_style() {
        let mut app = crate::app::App::headless(32, 32);
        app.with_widget(view());
        let parent = app.spawn_root().id();
        let e = app.compose(parent, |tree| tree.spawn(Image::build("thumbs_up")));
        let world = &app.world;
        assert!(world.has::<Image>(e));
        assert!(world.has::<ImageResource>(e));
        assert!(world.has::<crate::ui::Style>(e));
    }

    #[test]
    fn static_sources_are_final_before_loading() {
        let mut app = crate::app::App::headless(32, 32);
        app.with_widget(view());
        app.add_resource(image_manager());
        let root = app.spawn_root().id();
        let (dsl, builder) = app.compose(root, |cx| {
            let dsl = crate::ui! {
                Image ("unused") [
                    Image::new("blue"),
                ]
            };
            let builder = cx.spawn((Image::build("unused"), Image::new("blue")));
            (dsl, builder)
        });
        for entity in [dsl, builder] {
            let resource = app.world.get::<ImageResource>(entity).unwrap().0.borrow();
            assert_eq!(resource.as_ref().unwrap().token(), "blue");
        }
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
        view().auto_attach().unwrap().invoke(&mut world, entity);

        let image = world.get::<Image>(entity).unwrap();
        let cache = world.get::<ImageResource>(entity);
        let manager = world.resource::<ResourceManager<Texture<'static>>>();
        let first = resolve_image(image, cache, manager).unwrap();
        let second = resolve_image(image, cache, manager).unwrap();
        assert!(alloc::rc::Rc::ptr_eq(&first, &second));
        assert_eq!(first.read_pixels().unwrap().bytes(), &BLUE);

        world.get_mut::<Image>(entity).unwrap().src = ImageSource::from("missing");
        let image = world.get::<Image>(entity).unwrap();
        let fallback = resolve_image(
            image,
            world.get::<ImageResource>(entity),
            world.resource::<ResourceManager<Texture<'static>>>(),
        )
        .unwrap();
        assert_eq!(fallback.read_pixels().unwrap().bytes(), &RED);
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
            resolve_image(
                image,
                None,
                world.resource::<ResourceManager<Texture<'static>>>(),
            )
            .unwrap()
            .read_pixels()
            .unwrap()
            .bytes(),
            &BLUE
        );
    }

    #[test]
    fn resource_leases_keep_their_manager_and_cached_values_across_replacement() {
        let original = image_manager();
        let replacement = ResourceManager::new(
            MaxSize::Bytes(64),
            Texture::from_static(&BLUE, 1, 1, ColorFormat::RGBA8888).expect("valid pixel storage"),
        )
        .with_static(
            "blue",
            Texture::from_static(&RED, 1, 1, ColorFormat::RGBA8888).expect("valid pixel storage"),
        )
        .with_static(
            "other",
            Texture::from_static(&RED, 1, 1, ColorFormat::RGBA8888).expect("valid pixel storage"),
        );
        let mut image = Image::new("blue");
        let cache = ImageResource(RefCell::new(Some(original.load("blue"))));
        assert!(resolve_image(&image, Some(&cache), None).is_none());
        let first = resolve_image(&image, Some(&cache), Some(&original)).unwrap();
        assert_eq!(first.read_pixels().unwrap().bytes(), &BLUE);
        assert!(alloc::rc::Rc::ptr_eq(
            &first,
            &resolve_image(&image, Some(&cache), None).unwrap()
        ));
        original.add_static(
            "blue",
            Texture::from_static(&BLUE, 1, 1, ColorFormat::RGBA8888).expect("valid pixel storage"),
        );
        assert!(resolve_image(&image, Some(&cache), None).is_none());
        assert_eq!(
            resolve_image(&image, Some(&cache), Some(&replacement))
                .unwrap()
                .read_pixels()
                .unwrap()
                .bytes(),
            &BLUE
        );
        drop(original);
        assert_eq!(
            resolve_image(&image, Some(&cache), None)
                .unwrap()
                .read_pixels()
                .unwrap()
                .bytes(),
            &BLUE
        );
        assert_eq!(
            resolve_image(&image, None, Some(&replacement))
                .unwrap()
                .read_pixels()
                .unwrap()
                .bytes(),
            &RED
        );
        image.src = ImageSource::from("other");
        assert!(resolve_image(&image, Some(&cache), None).is_none());
        assert_eq!(
            resolve_image(&image, Some(&cache), Some(&replacement))
                .unwrap()
                .read_pixels()
                .unwrap()
                .bytes(),
            &RED
        );
        assert_eq!(cache.0.borrow().as_ref().unwrap().token(), "other");
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Request {
        Texture {
            ptr: *const u8,
            pos: Point,
            size: Point,
            transform: Transform,
            quad: Option<[Point; 4]>,
            clip: Rect,
            radius: Fixed,
            composite: CompositeMode,
            opa: u8,
        },
        Path {
            ptr: *const PathCmd,
            transform: Transform,
            clip: Rect,
            color: Color,
            rule: FillRule,
            opa: u8,
        },
    }

    #[derive(Default)]
    struct Trace {
        request: Option<Request>,
        calls: usize,
        fail: bool,
    }

    impl Renderer for Trace {
        type TargetState = ();

        fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
            self.request = Some(match *request.command {
                DrawCommand::Blit {
                    pos,
                    size,
                    transform,
                    quad,
                    texture,
                    radius,
                    composite,
                    opa,
                } => Request::Texture {
                    ptr: texture.read_pixels().unwrap().bytes().as_ptr(),
                    pos,
                    size,
                    transform,
                    quad,
                    clip: request.clip,
                    radius,
                    composite,
                    opa,
                },
                DrawCommand::FillPath {
                    path,
                    transform,
                    paint: Paint::Color(color),
                    fill_rule,
                    opa,
                } => Request::Path {
                    ptr: path.cmds.as_ptr(),
                    transform,
                    clip: request.clip,
                    color: (*color).into(),
                    rule: fill_rule,
                    opa,
                },
                _ => panic!("unexpected Image request"),
            });
            self.calls += 1;
            if self.fail {
                Err(RenderError::BackendFailure)
            } else {
                Ok(())
            }
        }
        fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
            Ok(())
        }
    }

    #[test]
    fn typed_paint_retains_borrowed_requests_early_returns_and_first_error_without_allocating() {
        static COMMANDS: &[PathCmd] = &[PathCmd::Close];
        let mut world = World::new();
        world.insert_resource(Theme::default());
        let entity = world.spawn_empty();
        let descriptor = view();
        assert_eq!(descriptor.name(), "Image");
        assert_eq!(descriptor.priority(), Some(70));
        assert_eq!(
            descriptor.component_filter(),
            Some(core::any::TypeId::of::<Image>())
        );
        assert!(descriptor.auto_attach().is_some());
        let render = descriptor.render().unwrap();
        let style = Style::default();
        let rect = Rect::new(5, 7, 40, 20);
        let clip = Rect::new(7, 9, 12, 13);
        let transform = Transform::translate(Fixed::from_int(3), Fixed::from_int(11));
        let quad = Some([
            Point::new(5, 7),
            Point::new(45, 7),
            Point::new(45, 27),
            Point::new(5, 27),
        ]);
        for _ in 0..2 {
            let mut trace = Trace::default();
            let mut paint = PaintCtx::new(
                &mut trace,
                &style,
                &rect,
                &clip,
                transform,
                quad,
                WidgetState::Enabled,
            );
            assert_eq!(
                tracked_allocations(|| {
                    render(&world, entity, &mut paint);
                }),
                0
            );
            assert_eq!(paint.error(), None);
            assert_eq!(trace.calls, 0);
        }
        world.insert(
            entity,
            Image::new(Path::from_static(COMMANDS)).with_viewbox(Fixed::ZERO),
        );
        let mut trace = Trace::default();
        {
            let mut paint = PaintCtx::new(
                &mut trace,
                &style,
                &rect,
                &clip,
                transform,
                quad,
                WidgetState::Enabled,
            );
            render(&world, entity, &mut paint);
            assert_eq!(paint.error(), None);
        }
        assert_eq!(trace.calls, 0);

        world.insert(entity, Image::new(Path::from_static(COMMANDS)));
        world.remove_resource::<Theme>();
        let rejected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut trace = Trace::default();
            let mut paint = PaintCtx::new(
                &mut trace,
                &style,
                &rect,
                &clip,
                transform,
                quad,
                WidgetState::Enabled,
            );
            render(&world, entity, &mut paint);
        }));
        assert!(rejected.is_err(), "a drawable vector requires Theme");
        world.insert_resource(Theme::default());

        world.insert_resource(image_manager());
        world.insert(
            entity,
            Image::new("blue")
                .with_radius(3)
                .with_composite(CompositeMode::Screen),
        );
        world
            .resource::<ResourceManager<Texture<'static>>>()
            .unwrap()
            .prefetch("blue");
        let expected = Request::Texture {
            ptr: BLUE.as_ptr(),
            pos: Point::new(5, 7),
            size: Point::new(40, 20),
            transform,
            quad,
            clip,
            radius: Fixed::from_int(3),
            composite: CompositeMode::Screen,
            opa: 255,
        };
        for cached in [false, true] {
            if cached {
                view().auto_attach().unwrap().invoke(&mut world, entity);
                let image = world.get::<Image>(entity).unwrap();
                resolve_image(
                    image,
                    world.get::<ImageResource>(entity),
                    world.resource::<ResourceManager<Texture<'static>>>(),
                );
                world.remove_resource::<ResourceManager<Texture<'static>>>();
            }
            for fail in [false, true] {
                for _ in 0..2 {
                    let mut trace = Trace {
                        fail,
                        ..Trace::default()
                    };
                    {
                        let mut paint = PaintCtx::new(
                            &mut trace,
                            &style,
                            &rect,
                            &clip,
                            transform,
                            quad,
                            WidgetState::Disabled,
                        );
                        assert_eq!(
                            tracked_allocations(|| render(&world, entity, &mut paint)),
                            0
                        );
                        assert_eq!(paint.error(), fail.then_some(RenderError::BackendFailure));
                        if fail {
                            render(&world, entity, &mut paint);
                        }
                    }
                    assert_eq!(trace.calls, 1);
                    assert_eq!(trace.request, Some(expected));
                }
            }
        }
        world.insert(entity, Image::new(Path::from_static(COMMANDS)));
        world.insert_resource(Theme::light());
        let color = world
            .resource::<Theme>()
            .unwrap()
            .resolve_in(ColorToken::OnSurface, WidgetState::Disabled);
        let expected = Request::Path {
            ptr: COMMANDS.as_ptr(),
            transform: vector_transform(&rect, Fixed::from_int(24), Fixed::from_int(20), transform)
                .unwrap(),
            clip,
            color,
            rule: FillRule::EvenOdd,
            opa: 255,
        };
        for _ in 0..2 {
            let mut trace = Trace::default();
            {
                let mut paint = PaintCtx::new(
                    &mut trace,
                    &style,
                    &rect,
                    &clip,
                    transform,
                    None,
                    WidgetState::Disabled,
                );
                assert_eq!(
                    tracked_allocations(|| render(&world, entity, &mut paint)),
                    0
                );
                assert_eq!(paint.error(), None);
            }
            assert_eq!(trace.calls, 1);
            assert_eq!(trace.request, Some(expected));
        }
    }

    #[test]
    fn source_writes_and_cache_membership_repaint_in_the_actual_app() {
        use crate::surface::FramebufferAccess;
        let mut app = crate::app::App::headless(8, 8);
        app.with_widget(view());
        app.add_resource(image_manager().with_static(
            "red",
            Texture::from_static(&RED, 1, 1, ColorFormat::RGBA8888).expect("valid pixel storage"),
        ));
        let root = app.spawn_root().id();
        app.world.insert(root, Image::new("blue"));
        app.render_dirty().unwrap();
        assert_eq!(
            &app.backend.framebuffer().read_pixels().unwrap().bytes()[..4],
            &BLUE
        );
        app.world.get_mut::<Image>(root).unwrap().src = ImageSource::from("red");
        app.render_dirty().unwrap();
        assert_eq!(
            &app.backend.framebuffer().read_pixels().unwrap().bytes()[..4],
            &RED
        );
        view().auto_attach().unwrap().invoke(&mut app.world, root);
        app.render_dirty().unwrap();
        assert_eq!(
            &app.backend.framebuffer().read_pixels().unwrap().bytes()[..4],
            &RED
        );
        app.world.get_mut::<Image>(root).unwrap().src = ImageSource::from("blue");
        app.render_dirty().unwrap();
        assert_eq!(
            &app.backend.framebuffer().read_pixels().unwrap().bytes()[..4],
            &BLUE
        );
        app.world.remove::<ImageResource>(root);
        app.render_dirty().unwrap();
        assert_eq!(
            &app.backend.framebuffer().read_pixels().unwrap().bytes()[..4],
            &BLUE
        );
    }
}
