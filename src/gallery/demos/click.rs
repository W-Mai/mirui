#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, World};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::ui::Style;
use crate::ui::dirty::Dirty;

pub struct Toggle {
    pub on: bool,
    pub base: Color,
    pub accent: Color,
}

pub fn build_widgets(world: &mut World, parent: Entity) {
    let colors = [
        Color::rgb(88, 166, 255),
        Color::rgb(63, 185, 80),
        Color::rgb(248, 81, 73),
    ];
    let accent = Color::rgb(210, 168, 255);

    //~focus-start
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
                ] on Tap {
                    let new_color = ctx
                        .world
                        .get_mut::<Toggle>(ctx.entity)
                        .map(|t| {
                            t.on = !t.on;
                            if t.on { t.accent } else { t.base }
                        });
                    if let Some(c) = new_color {
                        if let Some(style) = ctx.world.get_mut::<Style>(ctx.entity) {
                            style.set_bg_color(c);
                        }
                        ctx.world.insert(ctx.entity, Dirty);
                    }
                }
            }
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
    build_widgets(&mut app.world, parent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Children;
    use crate::ui::IdMap;
    use crate::ui::builder::WidgetBuilder;

    use crate::input::event::GestureHandler;
    use crate::input::event::gesture::GestureEvent;

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

    #[test]
    fn tap_toggles_on_flag() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        build_widgets(&mut world, parent);
        let row = world.get::<Children>(parent).unwrap().0[0];
        let card = world.get::<Children>(row).unwrap().0[0];

        assert_eq!(world.get::<Toggle>(card).map(|t| t.on), Some(false));
        GestureHandler::trigger(
            &mut world,
            card,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: card,
            },
        );
        assert_eq!(world.get::<Toggle>(card).map(|t| t.on), Some(true));
    }
}
