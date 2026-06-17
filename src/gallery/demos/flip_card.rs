extern crate alloc;

#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, World};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::types::Transform3D;
use crate::ui::dirty::Dirty;
use crate::ui::root_viewport;
use crate::ui::widgets::WidgetTransform3D;
use crate::ui::{Children, Parent, Style};

pub const DEFAULT_VIEW: (u16, u16) = (480, 320);

pub struct FlipCard {
    pub angle_deg: Fixed,
    pub speed_deg: Fixed,
    pub front_color: Color,
    pub back_color: Color,
    pub root: Entity,
}

//~focus-start
#[mirui_macros::system(order = ANIMATION)]
pub fn flip_system(world: &mut World) {
    // 5/12 and 9/16 reproduce the original 200×180 card in a 480×320 window.
    let (vw, vh) = root_viewport(world)
        .map_or((DEFAULT_VIEW.0 as i32, DEFAULT_VIEW.1 as i32), |r| {
            (r.w.to_int(), r.h.to_int())
        });
    let card_w = vw * 5 / 12;
    let card_h = vh * 9 / 16;
    let card_left = (vw - card_w) / 2;
    let card_top = (vh - card_h) / 2;

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
            style.layout.left = Dimension::px(card_left);
            style.layout.top = Dimension::px(card_top);
            style.layout.width = Dimension::px(card_w);
            style.layout.height = Dimension::px(card_h);
        }

        world.insert(
            e,
            WidgetTransform3D(Transform3D::rotate_y_perspective(
                angle,
                Fixed::from_int(400),
            )),
        );
        world.insert(e, Dirty);
        world.insert(root, Dirty);
    }
}
//~focus-end

pub fn build_widgets(world: &mut World, parent: Entity) {
    let card = WidgetBuilder::new(world)
        .bg_color(Color::rgb(88, 166, 255))
        .layout(LayoutStyle {
            position: Position::Absolute,
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
            root: parent,
        },
    );

    world.insert(card, Parent(parent));
    if let Some(children) = world.get_mut::<Children>(parent) {
        children.0.push(card);
    }
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_system(flip_system::system());
    build_widgets(&mut app.world, parent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::IdMap;
    use crate::ui::builder::WidgetBuilder;

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
