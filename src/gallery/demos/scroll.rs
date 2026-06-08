use super::attach_to_parent;
#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, World};
use crate::event::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;

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

    let colors = [
        ("Item 0", Color::rgb(88, 166, 255)),
        ("Item 1", Color::rgb(63, 185, 80)),
        ("Item 2", Color::rgb(248, 81, 73)),
        ("Item 3", Color::rgb(210, 168, 255)),
        ("Item 4", Color::rgb(255, 200, 50)),
        ("Item 5", Color::rgb(150, 100, 200)),
        ("Item 6", Color::rgb(100, 200, 150)),
        ("Item 7", Color::rgb(200, 100, 100)),
    ];

    ui! {
        :(
            parent: parent
            world: world
        :)

        View (direction: FlexDirection::Column, bg_color: Color::rgb(40, 40, 60), grow: 1.0) [
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
            ScrollConfig {
                direction: ScrollAxis::Vertical,
                elastic: true,
                content_height: Fixed::from_int(480),
                content_width: Fixed::ZERO,
            },
        ] {
            walk colors.iter() with item {
                Row (bg_color: item.1, height: 60, border_radius: 4, text: item.0) {}
            }
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
    build_widgets(&mut app.world, parent)
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
        let root = build_widgets(&mut world, parent);
        assert_ne!(root, parent);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| c.0.contains(&root)),
        );
    }
}
