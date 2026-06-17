extern crate alloc;

#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, World};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::types::Transform3D;
use crate::ui::dirty::Dirty;
use crate::ui::widgets::{Image, WidgetTransform3D};
use crate::ui::{Children, Parent};

pub struct Spinner {
    pub angle: Fixed,
    pub speed: Fixed,
    pub bounce_phase: Fixed,
}

//~focus-start
#[mirui_macros::system(order = ANIMATION)]
pub fn spin_system(world: &mut World) {
    let mut entities = alloc::vec::Vec::new();
    world.query::<Spinner>().collect_into(&mut entities);
    for e in entities {
        let (angle, bounce) = if let Some(s) = world.get_mut::<Spinner>(e) {
            s.angle += s.speed;
            if s.angle >= Fixed::from_int(360) {
                s.angle -= Fixed::from_int(360);
            }
            s.bounce_phase += s.speed;
            if s.bounce_phase >= Fixed::from_int(360) {
                s.bounce_phase -= Fixed::from_int(360);
            }
            (s.angle, s.bounce_phase)
        } else {
            continue;
        };

        let t_num = bounce.to_int() % 180;
        let t = Fixed::from_int(t_num) / Fixed::from_int(180);
        let two_t_minus_1 = t * Fixed::from_int(2) - Fixed::ONE;
        let h = Fixed::ONE - two_t_minus_1 * two_t_minus_1;

        let bounce_y = Fixed::ZERO - h * Fixed::from_int(100);
        let squash = Fixed::ONE - (Fixed::ONE - h) / Fixed::from_int(4);
        let stretch = Fixed::ONE + h / Fixed::from_int(8);

        let rot = Transform3D::rotate_y_perspective(angle, Fixed::from_int(400));
        let scale = Transform3D::scale(squash, stretch);
        let translate = Transform3D::translate(Fixed::ZERO, bounce_y);
        world.insert(
            e,
            WidgetTransform3D(translate.compose(&rot).compose(&scale)),
        );
        world.insert(e, Dirty);
    }
}
//~focus-end

pub fn build_widgets(world: &mut World, parent: Entity) {
    let side = 120;
    let img_widget = WidgetBuilder::new(world)
        .layout(LayoutStyle {
            position: Position::Absolute,
            left: Dimension::px((480 - side) / 2),
            top: Dimension::px(320 - side - 20),
            width: Dimension::px(side),
            height: Dimension::px(side),
            ..Default::default()
        })
        .id();
    world.insert(img_widget, Image::new("thumbs_up"));
    world.insert(
        img_widget,
        Spinner {
            angle: Fixed::ZERO,
            speed: Fixed::from_int(3),
            bounce_phase: Fixed::ZERO,
        },
    );
    world.insert(img_widget, Parent(parent));
    if let Some(children) = world.get_mut::<Children>(parent) {
        children.0.push(img_widget);
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
