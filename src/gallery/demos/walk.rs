#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::components::Text;
use crate::ecs::{Entity, World};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::widget::Style;

pub fn build_widgets(world: &mut World, parent: Entity) {
    if let Some(style) = world.get_mut::<Style>(parent) {
        style.bg_color = Some(Color::rgb(30, 30, 46).into());
        style.layout = LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(480),
            height: Dimension::px(320),
            grow: Fixed::ONE,
            ..Default::default()
        };
    }

    let colors = [
        Color::rgb(88, 166, 255),
        Color::rgb(63, 185, 80),
        Color::rgb(248, 81, 73),
        Color::rgb(210, 168, 255),
        Color::rgb(255, 200, 50),
    ];

    let show_footer = true;

    ui! {
        :(
            parent: parent
            world: world
        :)

        Column (grow: 1.0) {
            walk colors.iter() with color {
                View (bg_color: *color, grow: 1.0, border_radius: 4) {}
            }
            if show_footer {
                View (bg_color: Color::rgb(50, 50, 70), height: 30) {
                    Text ("conditional!") {}
                }
            }
        }
    };
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    build_widgets(&mut app.world, parent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::Children;
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
