extern crate alloc;

#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, World};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::ui::widgets::assets::*;
use crate::ui::widgets::{Button, Checkbox, Image, ProgressBar, Text};

pub fn build_widgets(world: &mut World, parent: Entity) {
    //~focus-start
    ui! {
        :(
            parent: parent
            world: world
        :)

        Column (grow: 1.0) {
            Row (
                bg_color: Color::rgb(30, 102, 245),
                height: 40,
                border_radius: 8,
                align: AlignItems::Center,
                padding: Padding::all(8)
            ) {
                View (text: "mirui Components", grow: 1.0)
                View (bg_color: Color::rgb(255, 200, 50), width: 16, height: 16, border_radius: 8)
            }
            Image (
                width: IMG_THUMBS_UP.width as i32,
                height: IMG_THUMBS_UP.height as i32,
                src: "thumbs_up"
            )
            Row (height: 36) {
                Button (
                    border_radius: 6,
                    grow: 1.0,
                    normal_color: Color::rgb(63, 185, 80),
                    pressed_color: Color::rgb(40, 140, 55)
                ) [
                    Text(b"OK".to_vec()),
                ]
                Button (
                    border_radius: 6,
                    grow: 1.0,
                    normal_color: Color::rgb(248, 81, 73),
                    pressed_color: Color::rgb(200, 50, 45)
                ) [
                    Text(b"Cancel".to_vec()),
                ]
            }
            Column (
                height: 50,
                justify: JustifyContent::SpaceBetween
            ) {
                ProgressBar (border_radius: 4, height: 12, value: 0.7)
                ProgressBar (
                    border_radius: 4,
                    height: 12,
                    value: 0.4,
                    fill_color: Color::rgb(63, 185, 80)
                )
                ProgressBar (
                    border_radius: 4,
                    height: 12,
                    value: 0.9,
                    fill_color: Color::rgb(248, 81, 73)
                )
            }
            Row (height: 30, align: AlignItems::Center) {
                Checkbox (
                    border_radius: 4,
                    width: 24,
                    height: 24,
                    checked: true,
                    checked_color: Color::rgb(88, 166, 255),
                    unchecked_color: Color::rgb(80, 80, 100)
                )
                Checkbox (
                    border_radius: 4,
                    width: 24,
                    height: 24,
                    checked_color: Color::rgb(63, 185, 80),
                    unchecked_color: Color::rgb(80, 80, 100)
                )
                Checkbox (
                    border_radius: 4,
                    width: 24,
                    height: 24,
                    checked: true,
                    checked_color: Color::rgb(248, 81, 73),
                    unchecked_color: Color::rgb(80, 80, 100)
                )
            }
            View (
                bg_color: Color::rgb(40, 40, 55),
                height: 30,
                border_radius: 6,
                text: "Button | ProgressBar | Checkbox | Image"
            )
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
    app.add_plugin(crate::app::plugins::ImageResourcesPlugin::default());
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
