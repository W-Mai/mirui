#![allow(clippy::needless_update)]

use super::attach_to_parent;
#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::draw::command::DrawCommand;
use crate::draw::renderer::Renderer;
use crate::ecs::{Entity, World};
use crate::event::GestureHandler;
use crate::event::gesture::GestureEvent;
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::widget::dirty::Dirty;
use crate::widget::view::{View, ViewCtx};

pub struct Diamond {
    pub color: Color,
    pub line_width: Fixed,
}

impl Default for Diamond {
    fn default() -> Self {
        Self {
            color: Color::rgb(255, 255, 255),
            line_width: Fixed::from_int(1),
        }
    }
}

fn diamond_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(d) = world.get::<Diamond>(entity) else {
        return;
    };
    let half_w = rect.w / Fixed::from_int(2);
    let half_h = rect.h / Fixed::from_int(2);
    let cx = rect.x + half_w;
    let cy = rect.y + half_h;
    let top = Point { x: cx, y: rect.y };
    let right = Point {
        x: rect.x + rect.w,
        y: cy,
    };
    let bottom = Point {
        x: cx,
        y: rect.y + rect.h,
    };
    let left = Point { x: rect.x, y: cy };

    let segs = [(top, right), (right, bottom), (bottom, left), (left, top)];
    for (p1, p2) in segs {
        renderer.draw(
            &DrawCommand::Line {
                p1,
                p2,
                transform: ctx.transform,
                color: d.color,
                width: d.line_width,
                opa: 255,
            },
            ctx.clip,
        );
    }
}

pub fn diamond_view() -> View {
    View::new("Diamond", 60, diamond_render)
}

pub const PALETTE: [Color; 3] = [
    Color::rgb(244, 167, 89),
    Color::rgb(140, 211, 255),
    Color::rgb(190, 240, 140),
];

fn cycle_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    if !matches!(event, GestureEvent::Tap { .. }) {
        return false;
    }
    if let Some(d) = world.get_mut::<Diamond>(entity) {
        let i = PALETTE.iter().position(|c| *c == d.color).unwrap_or(0);
        d.color = PALETTE[(i + 1) % PALETTE.len()];
    }
    world.insert(entity, Dirty);
    true
}

pub fn build_widgets(world: &mut World, parent: Entity) -> Entity {
    let root = WidgetBuilder::new(world)
        .bg_color(Color::rgb(28, 28, 36))
        .layout(LayoutStyle {
            direction: FlexDirection::Row,
            justify: JustifyContent::SpaceEvenly,
            align: AlignItems::Center,
            width: Dimension::px(480),
            height: Dimension::px(200),
            ..Default::default()
        })
        .id();

    ui! {
        :(
            parent: parent
            world: world
        :)

        Row (
            direction: FlexDirection::Row,
            justify: JustifyContent::SpaceEvenly,
            align: AlignItems::Center,
            width: 480,
            height: 200
        ) {
            Diamond (
                color: PALETTE[0],
                line_width: Fixed::from_int(2),
                width: 100,
                height: 100
            ) [
                GestureHandler {
                    on_gesture: cycle_handler,
                },
            ] {}
            Diamond (
                color: PALETTE[1],
                line_width: Fixed::from_int(3),
                width: 100,
                height: 100
            ) [
                GestureHandler {
                    on_gesture: cycle_handler,
                },
            ] {}
            Diamond (
                color: PALETTE[2],
                line_width: Fixed::from_int(4),
                width: 100,
                height: 100
            ) [
                GestureHandler {
                    on_gesture: cycle_handler,
                },
            ] {}
        }
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
    app.with_widget(diamond_view());
    build_widgets(&mut app.world, parent)
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
        reg.insert(diamond_view());
        world.insert_resource(reg);
        let parent = WidgetBuilder::new(&mut world).id();
        let root = build_widgets(&mut world, parent);
        assert_ne!(root, parent);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| c.0.contains(&root)),
        );
    }
}
