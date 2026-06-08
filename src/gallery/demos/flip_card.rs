extern crate alloc;

use super::attach_to_parent;
#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::components::WidgetTransform3D;
use crate::ecs::{Entity, World};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::types::Transform3D;
use crate::widget::dirty::Dirty;
use crate::widget::{Children, Parent, Style};

pub struct FlipCard {
    pub angle_deg: Fixed,
    pub speed_deg: Fixed,
    pub front_color: Color,
    pub back_color: Color,
    pub root: Entity,
}

#[mirui_macros::system(order = ANIMATION)]
pub fn flip_system(world: &mut World) {
    let mut cards = alloc::vec::Vec::new();
    world.query::<FlipCard>().collect_into(&mut cards);
    for e in cards {
        let (angle, front, back, root) = if let Some(c) = world.get_mut::<FlipCard>(e) {
            c.angle_deg += c.speed_deg;
            if c.angle_deg >= Fixed::from_int(360) {
                c.angle_deg -= Fixed::from_int(360);
            }
            (c.angle_deg, c.front_color, c.back_color, c.root)
        } else {
            continue;
        };

        let halfway = Fixed::from_int(90);
        let three_quarters = Fixed::from_int(270);
        let color = if angle < halfway || angle >= three_quarters {
            front
        } else {
            back
        };
        if let Some(style) = world.get_mut::<Style>(e) {
            style.set_bg_color(color);
        }

        world.insert(
            e,
            WidgetTransform3D(Transform3D::rotate_y_perspective(
                angle,
                Fixed::from_int(400),
            )),
        );
        world.insert(root, Dirty);
    }
}

pub fn build_widgets(world: &mut World, parent: Entity) -> Entity {
    let root = WidgetBuilder::new(world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(480),
            height: Dimension::px(320),
            ..Default::default()
        })
        .id();

    let card = WidgetBuilder::new(world)
        .bg_color(Color::rgb(88, 166, 255))
        .layout(LayoutStyle {
            position: Position::Absolute,
            left: Dimension::px(140),
            top: Dimension::px(70),
            width: Dimension::px(200),
            height: Dimension::px(180),
            ..Default::default()
        })
        .id();
    world.insert(
        card,
        FlipCard {
            angle_deg: Fixed::ZERO,
            speed_deg: Fixed::ONE,
            front_color: Color::rgb(88, 166, 255),
            back_color: Color::rgb(248, 81, 73),
            root,
        },
    );

    world.insert(card, Parent(root));
    if let Some(children) = world.get_mut::<Children>(root) {
        children.0.push(card);
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
    app.add_system(flip_system::system());
    build_widgets(&mut app.world, parent)
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
        let root = build_widgets(&mut world, parent);
        assert_ne!(root, parent);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| c.0.contains(&root)),
        );
    }
}
