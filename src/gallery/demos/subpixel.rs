extern crate alloc;

use super::attach_to_parent;
#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, World};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::widget;
use crate::widget::{Children, Parent};
use alloc::vec::Vec;

pub struct BarState {
    pub y: Fixed,
    pub speed: Fixed,
    pub snap: bool,
    pub x: Fixed,
}

pub struct BarBounds {
    pub h: i32,
}

#[mirui_macros::system(order = ANIMATION)]
pub fn bar_move_system(world: &mut World) {
    let bound_h = world.resource::<BarBounds>().map(|b| b.h).unwrap_or(128);
    let mut buf = Vec::new();
    world.query::<BarState>().collect_into(&mut buf);
    for e in buf {
        let (new_y, changed) = {
            let Some(bar) = world.get_mut::<BarState>(e) else {
                continue;
            };
            let old_display = if bar.snap { bar.y.floor() } else { bar.y };
            bar.y += bar.speed;
            if bar.y > Fixed::from_int(bound_h - 18) {
                bar.y = Fixed::from_int(20);
            }
            let new_display = if bar.snap { bar.y.floor() } else { bar.y };
            (new_display, new_display != old_display)
        };
        if changed {
            let bx = world.get::<BarState>(e).unwrap().x;
            widget::set_position(world, e, bx, new_y);
        }
    }
}

pub fn build_widgets(world: &mut World, parent: Entity, view_w: u16, view_h: u16) -> Entity {
    let bw = view_w as i32;
    let bh = view_h as i32;
    world.insert_resource(BarBounds { h: bh });

    let root = WidgetBuilder::new(world)
        .bg_color(Color::rgb(20, 20, 30))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(bw),
            height: Dimension::px(bh),
            ..Default::default()
        })
        .id();

    let bar1 = WidgetBuilder::new(world)
        .bg_color(Color::rgb(255, 100, 100))
        .layout(LayoutStyle {
            position: Position::Absolute,
            left: Dimension::px(10),
            top: Dimension::px(20),
            width: Dimension::px(50),
            height: Dimension::px(8),
            ..Default::default()
        })
        .id();
    world.insert(
        bar1,
        BarState {
            y: Fixed::from_int(20),
            speed: Fixed::from_raw(9),
            snap: true,
            x: Fixed::from_int(10),
        },
    );

    let bar2 = WidgetBuilder::new(world)
        .bg_color(Color::rgb(100, 200, 255))
        .layout(LayoutStyle {
            position: Position::Absolute,
            left: Dimension::px(bw - 60),
            top: Dimension::px(20),
            width: Dimension::px(50),
            height: Dimension::px(8),
            ..Default::default()
        })
        .id();
    world.insert(
        bar2,
        BarState {
            y: Fixed::from_int(20),
            speed: Fixed::from_raw(9),
            snap: false,
            x: Fixed::from_int(bw - 60),
        },
    );

    world.insert(bar1, Parent(root));
    world.insert(bar2, Parent(root));
    if let Some(children) = world.get_mut::<Children>(root) {
        children.0.push(bar1);
        children.0.push(bar2);
    }

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
    app.add_system(bar_move_system::system());
    build_widgets(&mut app.world, parent, info.width, info.height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::IdMap;
    use crate::widget::builder::WidgetBuilder;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
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
