extern crate alloc;

#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, World};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::widget;
use crate::widget::root_viewport;
use crate::widget::{Children, Parent};
use alloc::vec::Vec;

const BAR_W: i32 = 50;
const RIGHT_MARGIN: i32 = 10;

pub const DEFAULT_VIEW: (u16, u16) = (480, 320);

pub struct BarState {
    pub y: Fixed,
    pub speed: Fixed,
    pub snap: bool,
    pub x: Fixed,
    pub right_anchored: bool,
}

pub struct BarBounds {
    pub w: i32,
    pub h: i32,
}

//~focus-start
#[mirui_macros::system(order = ANIMATION)]
pub fn bar_move_system(world: &mut World) {
    if let Some(rect) = root_viewport(world) {
        world.insert_resource(BarBounds {
            w: rect.w.to_int(),
            h: rect.h.to_int(),
        });
    }
    let (bound_w, bound_h) = world
        .resource::<BarBounds>()
        .map(|b| (b.w, b.h))
        .unwrap_or((DEFAULT_VIEW.0 as i32, DEFAULT_VIEW.1 as i32));
    let mut buf = Vec::new();
    world.query::<BarState>().collect_into(&mut buf);
    for e in buf {
        let (new_x, new_y, changed) = {
            let Some(bar) = world.get_mut::<BarState>(e) else {
                continue;
            };
            if bar.right_anchored {
                bar.x = Fixed::from_int(bound_w - BAR_W - RIGHT_MARGIN);
            }
            let old_display = if bar.snap { bar.y.floor() } else { bar.y };
            bar.y += bar.speed;
            if bar.y > Fixed::from_int(bound_h - 18) {
                bar.y = Fixed::from_int(20);
            }
            let new_display = if bar.snap { bar.y.floor() } else { bar.y };
            (
                bar.x,
                new_display,
                new_display != old_display || bar.right_anchored,
            )
        };
        if changed {
            widget::set_position(world, e, new_x, new_y);
        }
    }
}
//~focus-end

pub fn build_widgets(world: &mut World, parent: Entity) {
    let bar1 = WidgetBuilder::new(world)
        .bg_color(Color::rgb(255, 100, 100))
        .layout(LayoutStyle {
            position: Position::Absolute,
            left: Dimension::px(10),
            top: Dimension::px(20),
            width: Dimension::px(BAR_W),
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
            right_anchored: false,
        },
    );

    let bar2 = WidgetBuilder::new(world)
        .bg_color(Color::rgb(100, 200, 255))
        .layout(LayoutStyle {
            position: Position::Absolute,
            left: Dimension::px(0),
            top: Dimension::px(20),
            width: Dimension::px(BAR_W),
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
            x: Fixed::ZERO,
            right_anchored: true,
        },
    );

    world.insert(bar1, Parent(parent));
    world.insert(bar2, Parent(parent));
    if let Some(children) = world.get_mut::<Children>(parent) {
        children.0.push(bar1);
        children.0.push(bar2);
    }
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_system(bar_move_system::system());
    build_widgets(&mut app.world, parent);
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
        build_widgets(&mut world, parent);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
    }
}
