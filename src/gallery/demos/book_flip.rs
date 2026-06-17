extern crate alloc;

#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, World};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::types::Transform3D;
use crate::ui::dirty::Dirty;
use crate::ui::widgets::{TransformOrigin, WidgetTransform3D};

pub struct Page {
    pub angle_deg: Fixed,
    pub speed_deg: Fixed,
}

#[mirui_macros::system(order = ANIMATION)]
pub fn flip_system(world: &mut World) {
    let mut pages = alloc::vec::Vec::new();
    world.query::<Page>().collect_into(&mut pages);
    for e in pages {
        let angle = if let Some(p) = world.get_mut::<Page>(e) {
            p.angle_deg += p.speed_deg;
            // 0..120..0 keeps the right page from swinging past the spine and covering the left.
            if p.angle_deg > Fixed::from_int(120) || p.angle_deg < Fixed::ZERO {
                p.speed_deg = -p.speed_deg;
                p.angle_deg += p.speed_deg;
            }
            p.angle_deg
        } else {
            continue;
        };
        world.insert(
            e,
            WidgetTransform3D(Transform3D::rotate_y_perspective(
                Fixed::ZERO - angle,
                Fixed::from_int(500),
            )),
        );
        world.insert(e, Dirty);
    }
}

pub fn build_widgets(world: &mut World, parent: Entity) {
    //~focus-start
    ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 640,
            height: 360
        ) {
            View (
                position: Position::Absolute,
                left: 140,
                top: 60,
                width: 180,
                height: 240,
                bg_color: Color::rgb(220, 210, 180),
                border_radius: 4,
                border_color: Color::rgb(255, 255, 255),
                border_width: 3
            )
            View (
                position: Position::Absolute,
                left: 320,
                top: 60,
                width: 180,
                height: 240,
                bg_color: Color::rgb(200, 230, 200),
                border_radius: 4,
                border_color: Color::rgb(255, 255, 255),
                border_width: 3
            ) [
                TransformOrigin {
                    x: Fixed::ZERO,
                    y: Fixed::ONE / 2,
                },
                Page {
                    angle_deg: Fixed::ZERO,
                    speed_deg: Fixed::ONE / 2,
                },
            ]
        }
    };
    //~focus-end
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
    use crate::ui::Children;
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
