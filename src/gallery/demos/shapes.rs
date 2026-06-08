#![allow(clippy::needless_update)]

extern crate alloc;

use super::attach_to_parent;
#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::draw::command::DrawCommand;
use crate::draw::renderer::Renderer;
use crate::ecs::{Entity, MonoClock, World};
#[cfg(feature = "std")]
use crate::plugins::StdInstantClockPlugin;
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::widget::dirty::Dirty;
use crate::widget::view::{View, ViewCtx};

#[derive(Default)]
pub struct Shapes {
    pub start_ms: u32,
}

fn shapes_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(state) = world.get::<Shapes>(entity) else {
        return;
    };
    let now_ms = world
        .resource::<MonoClock>()
        .map(|c| c.now_ms())
        .unwrap_or(0);
    let elapsed_ms = now_ms.wrapping_sub(state.start_ms) as i32;

    let cx = rect.x + rect.w / Fixed::from_int(2);
    let cy = rect.y + rect.h / Fixed::from_int(2);
    let r = (rect.w.min(rect.h)) / Fixed::from_int(2) - Fixed::from_int(2);
    let center = Point { x: cx, y: cy };

    renderer.draw(
        &DrawCommand::Arc {
            center,
            transform: ctx.transform,
            radius: r,
            start_angle: Fixed::from_int(0),
            end_angle: Fixed::from_int(360),
            color: Color::rgb(80, 180, 220),
            width: Fixed::from_int(2),
            opa: 255,
        },
        ctx.clip,
    );

    let angle_deg_raw = ((elapsed_ms * 360) / 30_000) % 360;
    let angle_deg = Fixed::from_int(angle_deg_raw);
    let end = Point {
        x: cx + Fixed::cos_deg(angle_deg) * r,
        y: cy + Fixed::sin_deg(angle_deg) * r,
    };
    renderer.draw(
        &DrawCommand::Line {
            p1: center,
            p2: end,
            transform: ctx.transform,
            color: Color::rgb(255, 180, 80),
            width: Fixed::from_int(2),
            opa: 255,
        },
        ctx.clip,
    );

    for i in 0..12 {
        let a = Fixed::from_int(i * 30);
        let inner = r - Fixed::from_int(5);
        let outer = r - Fixed::from_int(1);
        let p1 = Point {
            x: cx + Fixed::cos_deg(a) * inner,
            y: cy + Fixed::sin_deg(a) * inner,
        };
        let p2 = Point {
            x: cx + Fixed::cos_deg(a) * outer,
            y: cy + Fixed::sin_deg(a) * outer,
        };
        renderer.draw(
            &DrawCommand::Line {
                p1,
                p2,
                transform: ctx.transform,
                color: Color::rgb(180, 180, 200),
                width: Fixed::ONE,
                opa: 255,
            },
            ctx.clip,
        );
    }
}

pub fn shapes_view() -> View {
    View::new("Shapes", 60, shapes_render).with_filter::<Shapes>()
}

#[mirui_macros::system(order = ANIMATION)]
pub fn shapes_anim_system(world: &mut World) {
    let mut buf = alloc::vec::Vec::new();
    world.query::<Shapes>().collect_into(&mut buf);
    for e in buf {
        world.insert(e, Dirty);
    }
}

pub fn build_widgets(world: &mut World, parent: Entity, view_w: u16, view_h: u16) -> Entity {
    let now_ms = world
        .resource::<MonoClock>()
        .map(|c| c.now_ms())
        .unwrap_or(0);

    let root = WidgetBuilder::new(world)
        .bg_color(Color::rgb(20, 20, 30))
        .layout(LayoutStyle {
            width: Dimension::px(view_w as i32),
            height: Dimension::px(view_h as i32),
            ..Default::default()
        })
        .id();

    ui! {
        :(
            parent: parent
            world: world
        :)

        Shapes (
            start_ms: now_ms,
            width: view_w as i32,
            height: view_h as i32,
            position: Position::Absolute,
            left: 0,
            top: 0
        ) {}
    };

    attach_to_parent(world, parent, root);
    root
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity) -> Entity
where
    B: Surface,
    F: RendererFactory<B>,
{
    let info = app.backend.display_info();
    app.add_plugin(StdInstantClockPlugin);
    app.with_widget(shapes_view());
    app.add_system(shapes_anim_system::system());
    build_widgets(&mut app.world, parent, info.width, info.height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::Children;
    use crate::widget::IdMap;
    use crate::widget::builder::WidgetBuilder;
    use crate::widget::view::ViewRegistry;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let mut reg = ViewRegistry::with_builtins();
        reg.insert(shapes_view());
        world.insert_resource(reg);
        let parent = WidgetBuilder::new(&mut world).id();
        let root = build_widgets(&mut world, parent, 128, 128);
        assert_ne!(root, parent);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| c.0.contains(&root)),
        );
    }
}
