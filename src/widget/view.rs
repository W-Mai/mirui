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

pub type ViewRender =
    fn(renderer: &mut dyn Renderer, world: &World, entity: Entity, rect: &Rect, ctx: &mut ViewCtx);

pub type ViewAttach = fn(world: &mut World, entity: Entity);

pub struct View {
    pub name: &'static str,
    /// Lower runs earlier. Slot reservation: 0..30 pre-bg,
    /// 30..50 explicit-bg widgets, 50 generic Style, 60..80 content
    /// widgets, 80..100 overlays.
    pub priority: u8,
    pub render: ViewRender,
    pub auto_attach: Option<ViewAttach>,
}

#[derive(Default)]
pub struct ViewRegistry {
    views: Vec<View>,
}

impl ViewRegistry {
    pub fn register(&mut self, view: View) {
        self.views.push(view);
    }

    pub fn sort_by_priority(&mut self) {
        self.views.sort_by_key(|v| v.priority);
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

/// `App::with_factory` runs this; tests building a `World`
/// without `App` call it to opt into the rendering pipeline.
pub fn install_default_registry(world: &mut World) {
    let mut reg = ViewRegistry::default();
    reg.register(super::style_view::view());
    reg.register(crate::components::button::view());
    reg.sort_by_priority();
    world.insert_resource(reg);
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
        View {
            name,
            priority,
            render: dummy_render,
            auto_attach: None,
        }
    }

    #[test]
    fn sort_by_priority_orders_lower_first() {
        let mut reg = ViewRegistry::default();
        reg.register(make_view("c", 80));
        reg.register(make_view("a", 40));
        reg.register(make_view("b", 50));
        reg.sort_by_priority();
        let names: Vec<&str> = reg.iter().map(|v| v.name).collect();
        assert_eq!(names, ["a", "b", "c"]);
    }

    #[test]
    fn sort_is_stable_within_same_priority() {
        let mut reg = ViewRegistry::default();
        reg.register(make_view("first-50", 50));
        reg.register(make_view("second-50", 50));
        reg.register(make_view("third-50", 50));
        reg.sort_by_priority();
        let names: Vec<&str> = reg.iter().map(|v| v.name).collect();
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
}
