// View registry — the type-level definition of one widget kind.
//
// `Widget` (in `widget/mod.rs`) is a per-entity marker. `View` is the
// per-kind dispatch entry: render fn + auto-attach fn + priority.
// `App::default_views()` populates the registry with built-ins; user
// code adds custom kinds via `App::register_view`.

use alloc::vec::Vec;

use crate::draw::renderer::Renderer;
use crate::ecs::{Entity, World};
use crate::types::{Point, Rect, Transform};
use crate::widget::Style;

/// Boxed as a struct so adding fields later doesn't change the
/// `ViewRender` fn-pointer signature (which would break user views).
pub struct ViewCtx<'a> {
    pub style: &'a Style,
    pub transform: Transform,
    pub quad: Option<[Point; 4]>,
    pub clip: &'a Rect,
    /// Set true by a view that emits its own background fill so the
    /// generic Style stage skips its bg fill but still emits border.
    pub bg_handled: bool,
}

impl ViewCtx<'_> {
    /// Active [`crate::widget::Theme`]. Lazy lookup so render fns
    /// that don't need fallback colors pay nothing. `App::new`
    /// guarantees the resource is present.
    pub fn theme<'w>(&self, world: &'w World) -> &'w crate::widget::Theme {
        world
            .resource::<crate::widget::Theme>()
            .expect("App::new must insert Theme; missing means a test fixture skipped App")
    }
}

pub type ViewRender =
    fn(renderer: &mut dyn Renderer, world: &World, entity: Entity, rect: &Rect, ctx: &mut ViewCtx);

pub type ViewAttach = fn(world: &mut World, entity: Entity);

pub struct View {
    name: &'static str,
    /// Lower runs earlier. Slot reservation: 0..30 pre-bg,
    /// 30..50 explicit-bg widgets, 50 generic Style, 60..80 content
    /// widgets, 80..100 overlays.
    priority: u8,
    render: ViewRender,
    auto_attach: Option<ViewAttach>,
    systems: &'static [crate::ecs::System],
}

impl View {
    pub const fn new(name: &'static str, priority: u8, render: ViewRender) -> Self {
        Self {
            name,
            priority,
            render,
            auto_attach: None,
            systems: &[],
        }
    }

    pub const fn with_attach(mut self, attach: ViewAttach) -> Self {
        self.auto_attach = Some(attach);
        self
    }

    pub const fn with_systems(mut self, systems: &'static [crate::ecs::System]) -> Self {
        self.systems = systems;
        self
    }

    /// Marker widget: only contributes systems, no rendering.
    pub const fn systems_only(name: &'static str, systems: &'static [crate::ecs::System]) -> Self {
        fn noop_render(
            _renderer: &mut dyn Renderer,
            _world: &World,
            _entity: Entity,
            _rect: &Rect,
            _ctx: &mut ViewCtx,
        ) {
        }
        Self {
            name,
            priority: 100,
            render: noop_render,
            auto_attach: None,
            systems,
        }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }
    pub(crate) fn priority(&self) -> u8 {
        self.priority
    }
    pub(crate) fn render(&self) -> ViewRender {
        self.render
    }
    pub(crate) fn auto_attach(&self) -> Option<ViewAttach> {
        self.auto_attach
    }

    /// Hand each contributed `System` to `sink`. Called by `App` at
    /// view-registration time; `View` owns when and how its systems
    /// surface — callers don't read `self.systems` directly.
    pub(crate) fn install(&self, _world: &mut World, mut sink: impl FnMut(crate::ecs::System)) {
        for &s in self.systems {
            sink(s);
        }
    }
}

#[derive(Default)]
pub struct ViewRegistry {
    views: Vec<View>,
}

impl ViewRegistry {
    /// Pre-populated registry containing every built-in widget.
    pub fn with_builtins() -> Self {
        let mut reg = Self::default();
        reg.insert(super::style_view::view());
        reg.insert(crate::components::button::view());
        reg.insert(crate::components::checkbox::view());
        reg.insert(crate::components::progress_bar::view());
        reg.insert(crate::components::tabbar::view());
        reg.insert(crate::components::text_input::view());
        reg.insert(crate::components::image::view());
        reg.insert(crate::components::text::view());
        reg.insert(crate::components::slider::view());
        reg.insert(crate::components::switch::view());
        reg.insert(crate::components::tab_pages::view());
        reg
    }

    /// Add a view, keeping the internal vec sorted by priority.
    pub fn insert(&mut self, view: View) {
        self.views.push(view);
        self.views.sort_by_key(|v| v.priority());
    }

    pub fn iter(&self) -> impl Iterator<Item = &View> {
        self.views.iter()
    }

    pub fn len(&self) -> usize {
        self.views.len()
    }

    pub fn is_empty(&self) -> bool {
        self.views.is_empty()
    }
}

/// Built-in views in priority order.
pub fn builtin_views() -> [View; 11] {
    [
        super::style_view::view(),
        crate::components::button::view(),
        crate::components::checkbox::view(),
        crate::components::progress_bar::view(),
        crate::components::tabbar::view(),
        crate::components::text_input::view(),
        crate::components::image::view(),
        crate::components::text::view(),
        crate::components::slider::view(),
        crate::components::switch::view(),
        crate::components::tab_pages::view(),
    ]
}

/// Test-only shortcut; production code uses `App::with_default_widgets`.
/// Also seeds the default `Theme` so render fns calling
/// `ctx.theme(world)` don't trip the missing-resource expectation.
pub fn install_default_registry(world: &mut World) {
    world.insert_resource(ViewRegistry::with_builtins());
    world.insert_resource(crate::widget::theme::Theme::default());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::command::DrawCommand;
    use crate::types::Fixed;

    fn dummy_render(
        _renderer: &mut dyn Renderer,
        _world: &World,
        _entity: Entity,
        _rect: &Rect,
        _ctx: &mut ViewCtx,
    ) {
    }

    fn make_view(name: &'static str, priority: u8) -> View {
        View::new(name, priority, dummy_render)
    }

    #[test]
    fn insert_keeps_priority_order() {
        let mut reg = ViewRegistry::default();
        reg.insert(make_view("c", 80));
        reg.insert(make_view("a", 40));
        reg.insert(make_view("b", 50));
        let names: Vec<&str> = reg.iter().map(|v| v.name()).collect();
        assert_eq!(names, ["a", "b", "c"]);
    }

    #[test]
    fn insert_is_stable_within_same_priority() {
        let mut reg = ViewRegistry::default();
        reg.insert(make_view("first-50", 50));
        reg.insert(make_view("second-50", 50));
        reg.insert(make_view("third-50", 50));
        let names: Vec<&str> = reg.iter().map(|v| v.name()).collect();
        assert_eq!(names, ["first-50", "second-50", "third-50"]);
    }

    // A view fn must be able to mutate `ViewCtx` while concurrently
    // borrowing components from `&World`. Real renders read multiple
    // components alongside the ctx-mut path; the borrow pattern must
    // compose. Pinning that here so a regression breaks loudly.
    #[test]
    fn render_fn_can_mutate_ctx_while_reading_world() {
        struct StubRenderer;
        impl Renderer for StubRenderer {
            fn draw(&mut self, _cmd: &DrawCommand, _clip: &Rect) {}
            fn flush(&mut self) {}
        }

        fn flip_bg_when_styled(
            _renderer: &mut dyn Renderer,
            world: &World,
            entity: Entity,
            _rect: &Rect,
            ctx: &mut ViewCtx,
        ) {
            if world.get::<Style>(entity).is_some() {
                ctx.bg_handled = true;
            }
        }

        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Style::default());

        let style = world.get::<Style>(e).expect("style present");
        let rect = Rect::new(0, 0, 0, 0);
        let mut ctx = ViewCtx {
            style,
            transform: Transform::default(),
            quad: None,
            clip: &rect,
            bg_handled: false,
        };
        let mut renderer = StubRenderer;

        flip_bg_when_styled(&mut renderer, &world, e, &rect, &mut ctx);
        assert!(
            ctx.bg_handled,
            "view fn must mutate ctx while reading world"
        );

        // Pin the type aliases — failure here means signature drift
        // breaks user-code views.
        let _: ViewRender = flip_bg_when_styled;
        let _ = Fixed::ZERO;
    }

    #[test]
    fn theme_accessor_returns_world_resource() {
        use crate::widget::Theme;
        use crate::widget::theme::ColorToken;
        let mut world = World::new();
        world.insert_resource(Theme::light());
        let style = Style::default();
        let rect = Rect::new(0, 0, 0, 0);
        let ctx = ViewCtx {
            style: &style,
            transform: Transform::default(),
            quad: None,
            clip: &rect,
            bg_handled: false,
        };
        let theme = ctx.theme(&world);
        // Compare via a representative token rather than struct
        // equality — `Theme` doesn't derive `PartialEq` (it owns a
        // `BTreeMap` of extras).
        assert_eq!(
            theme.resolve(ColorToken::Surface),
            Theme::light().resolve(ColorToken::Surface),
        );
    }

    #[test]
    #[should_panic(expected = "App::new must insert Theme")]
    fn theme_accessor_panics_when_resource_missing() {
        let world = World::new();
        let style = Style::default();
        let rect = Rect::new(0, 0, 0, 0);
        let ctx = ViewCtx {
            style: &style,
            transform: Transform::default(),
            quad: None,
            clip: &rect,
            bg_handled: false,
        };
        let _ = ctx.theme(&world);
    }
}
