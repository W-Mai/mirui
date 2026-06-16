extern crate alloc;

#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::components::{Image, WidgetTransform};
use crate::ecs::{Entity, World};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::types::Transform;
use crate::widget::dirty::Dirty;
use crate::widget::{Children, Parent};

pub struct Spinner {
    pub angle: Fixed,
    pub speed: Fixed,
}

//~focus-start
#[mirui_macros::system(order = ANIMATION)]
pub fn spin_system(world: &mut World) {
    let mut entities = alloc::vec::Vec::new();
    world.query::<Spinner>().collect_into(&mut entities);
    for e in entities {
        let next = if let Some(s) = world.get_mut::<Spinner>(e) {
            s.angle += s.speed;
            s.angle
        } else {
            continue;
        };
        world.insert(e, WidgetTransform(Transform::rotate_deg(next)));
        world.insert(e, Dirty);
    }
}
//~focus-end

pub fn build_widgets(world: &mut World, parent: Entity) {
    let spinning_box = WidgetBuilder::new(world)
        .bg_color(Color::rgb(248, 81, 73))
        .layout(LayoutStyle {
            position: Position::Absolute,
            left: Dimension::px(100),
            top: Dimension::px(100),
            width: Dimension::px(80),
            height: Dimension::px(80),
            ..Default::default()
        })
        .rotate(0)
        .id();
    world.insert(
        spinning_box,
        Spinner {
            angle: Fixed::ZERO,
            speed: Fixed::from_int(2),
        },
    );

    let spinning_img = WidgetBuilder::new(world)
        .layout(LayoutStyle {
            position: Position::Absolute,
            left: Dimension::px(300),
            top: Dimension::px(120),
            width: Dimension::px(64),
            height: Dimension::px(64),
            ..Default::default()
        })
        .rotate(0)
        .id();
    world.insert(spinning_img, Image::new("thumbs_up"));
    world.insert(
        spinning_img,
        Spinner {
            angle: Fixed::ZERO,
            speed: Fixed::from_int(3),
        },
    );

    for &entity in &[spinning_box, spinning_img] {
        world.insert(entity, Parent(parent));
        if let Some(children) = world.get_mut::<Children>(parent) {
            children.0.push(entity);
        }
    }
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_system(spin_system::system());
    app.add_plugin(crate::plugins::ImageResourcesPlugin::default());
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
