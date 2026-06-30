use core::cell::RefCell;

use mirui::ecs::World;
use mirui::prelude::Dimension;
use mirui::render::texture::{ColorFormat, Texture};
use mirui::render::{DrawCommand, Renderer};
use mirui::types::{Color, Fixed, Rect, Viewport};
use mirui::ui::builder::WidgetBuilder;
use mirui::ui::layout::LayoutStyle;
use mirui::ui::offscreen::OffscreenRender;
use mirui::ui::{OffscreenBufferPool, render_system};

#[derive(Default)]
struct Counts {
    blit: usize,
    fill: usize,
}

struct MockOuter {
    supports_offscreen: bool,
    counts: RefCell<Counts>,
}

impl Renderer for MockOuter {
    fn draw(&mut self, cmd: &DrawCommand, _clip: &Rect) {
        let mut c = self.counts.borrow_mut();
        match cmd {
            DrawCommand::Blit { .. } => c.blit += 1,
            DrawCommand::Fill { .. } => c.fill += 1,
            _ => {}
        }
    }
    fn flush(&mut self) {}
    fn supports_offscreen(&self) -> bool {
        self.supports_offscreen
    }
    fn offscreen_format(&self) -> Option<ColorFormat> {
        if self.supports_offscreen {
            Some(ColorFormat::RGBA8888)
        } else {
            None
        }
    }
    fn sample_target_region(&self, _src: &Rect) -> Option<Texture<'static>> {
        None
    }
    fn read_target_region(&self, _src: &Rect, _dst: &mut Texture) {}
    fn modify_target_region(&mut self, _src: &Rect, _f: &mut dyn FnMut(&mut Texture)) -> bool {
        false
    }
}

fn make_world(with_pool: bool) -> (World, mirui::ecs::Entity) {
    let mut app = mirui::app::App::headless(64, 64);
    app.with_default_widgets();
    if with_pool {
        app.with_offscreen_pool_budget(64 * 1024);
    }
    let mut world = app.world;

    let panel = WidgetBuilder::new(&mut world)
        .bg_color(Color::rgb(255, 0, 0))
        .layout(LayoutStyle {
            width: Dimension::px(32),
            height: Dimension::px(32),
            ..Default::default()
        })
        .id();
    world.insert(panel, OffscreenRender::default());
    (world, panel)
}

#[test]
fn gate_flip_routes_offscreen_to_blit_on_non_sw_outer() {
    let (world, panel) = make_world(true);
    let mut outer = MockOuter {
        supports_offscreen: true,
        counts: RefCell::new(Counts::default()),
    };
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    render_system::render(&world, panel, &viewport, &mut outer);

    let c = outer.counts.borrow();
    assert_eq!(c.blit, 1, "outer should receive exactly one blit-back");
    assert_eq!(c.fill, 0, "subtree fills should NOT reach outer");
}

#[test]
fn gate_off_falls_through_to_inline_render() {
    let (world, panel) = make_world(false);
    let mut outer = MockOuter {
        supports_offscreen: false,
        counts: RefCell::new(Counts::default()),
    };
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    render_system::render(&world, panel, &viewport, &mut outer);

    let c = outer.counts.borrow();
    assert_eq!(c.blit, 0, "inline path emits no blit-back");
    assert!(c.fill >= 1, "subtree fills reach outer inline");
}

#[test]
fn pool_resource_must_exist_when_gate_is_on() {
    let (mut world, panel) = make_world(false);
    world.insert_resource(OffscreenBufferPool::with_budget(64 * 1024));
    let mut outer = MockOuter {
        supports_offscreen: true,
        counts: RefCell::new(Counts::default()),
    };
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    render_system::render(&world, panel, &viewport, &mut outer);

    let c = outer.counts.borrow();
    assert_eq!(c.blit, 1);
}
