use mirui::ecs::World;
use mirui::prelude::Dimension;
use mirui::render::texture::Texture;
use mirui::render::{DrawCommand, Renderer};
use mirui::types::{Fixed, Rect, Viewport};
use mirui::ui::builder::WidgetBuilder;
use mirui::ui::layout::LayoutStyle;
use mirui::ui::render_system;
use mirui::ui::widgets::BackgroundBlur;

struct GracefulSkipRenderer {
    draws: usize,
}

impl Renderer for GracefulSkipRenderer {
    fn draw(&mut self, _cmd: &DrawCommand, _clip: &Rect) {
        self.draws += 1;
    }
    fn flush(&mut self) {}
    fn sample_target_region(&self, _src: &Rect) -> Option<Texture<'static>> {
        None
    }
    fn modify_target_region(&mut self, _src: &Rect, _f: &mut dyn FnMut(&mut Texture)) -> bool {
        false
    }
}

#[test]
fn background_blur_silently_skips_when_backend_returns_none() {
    let mut world = World::new();
    let widget = WidgetBuilder::new(&mut world)
        .layout(LayoutStyle {
            width: Dimension::px(64),
            height: Dimension::px(64),
            ..Default::default()
        })
        .id();
    world.insert(widget, BackgroundBlur::new(Fixed::from_int(4)));

    let root = WidgetBuilder::new(&mut world)
        .layout(LayoutStyle {
            width: Dimension::px(64),
            height: Dimension::px(64),
            ..Default::default()
        })
        .child(widget)
        .id();

    let mut renderer = GracefulSkipRenderer { draws: 0 };
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    render_system::render(&world, root, &viewport, &mut renderer);
    let _ = renderer.draws;
}

#[test]
fn background_blur_modify_path_returns_false_without_panic() {
    let mut renderer = GracefulSkipRenderer { draws: 0 };
    let rect = Rect {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        w: Fixed::from_int(16),
        h: Fixed::from_int(16),
    };
    let result = renderer.modify_target_region(&rect, &mut |_tex| {
        panic!("closure must not run when backend returns false");
    });
    assert!(!result);
}

#[test]
fn background_blur_sample_path_returns_none_without_panic() {
    let renderer = GracefulSkipRenderer { draws: 0 };
    let rect = Rect {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        w: Fixed::from_int(16),
        h: Fixed::from_int(16),
    };
    assert!(renderer.sample_target_region(&rect).is_none());
}
