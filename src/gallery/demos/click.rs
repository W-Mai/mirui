#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, World};
use crate::event::GestureHandler;
use crate::event::gesture::GestureEvent;
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::widget::Style;
use crate::widget::dirty::Dirty;

pub struct Toggle {
    pub on: bool,
    pub base: Color,
    pub accent: Color,
}

fn toggle_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    if let GestureEvent::Tap { .. } = event {
        let new_color = {
            let Some(t) = world.get_mut::<Toggle>(entity) else {
                return false;
            };
            t.on = !t.on;
            if t.on { t.accent } else { t.base }
        };
        if let Some(style) = world.get_mut::<Style>(entity) {
            style.set_bg_color(new_color);
        }
        world.insert(entity, Dirty);
        true
    } else {
        false
    }
}

pub fn build_widgets(world: &mut World, parent: Entity) {
    let colors = [
        Color::rgb(88, 166, 255),
        Color::rgb(63, 185, 80),
        Color::rgb(248, 81, 73),
    ];
    let accent = Color::rgb(210, 168, 255);

    ui! {
        :(
            parent: parent
            world: world
        :)

        Row (
            justify: JustifyContent::SpaceEvenly,
            align: AlignItems::Center,
            padding: Padding::all(20),
            grow: 1.0
        ) {
            walk colors.iter() with color {
                View (
                    bg_color: *color,
                    width: 120,
                    height: 80
                ) [
                    Toggle {
                        on: false,
                        base: *color,
                        accent,
                    },
                    GestureHandler {
                        on_gesture: toggle_handler,
                    },
                ] {}
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
