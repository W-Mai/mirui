extern crate alloc;

use crate::prelude::*;
use crate::ui::widgets::{Button, Checkbox, ProgressBar, Slider, Switch, TabBar, Text, TextInput};
use crate::ui::{Theme, theme};
use alloc::vec::Vec;

pub struct ThemeChoice(pub Theme);

pub const ACCENT: ColorToken = ColorToken::custom("accent");

pub fn dark_with_accent() -> Theme {
    Theme::dark().with(ACCENT, Color::rgb(255, 200, 60))
}

pub fn light_with_accent() -> Theme {
    Theme::light().with(ACCENT, Color::rgb(220, 60, 90))
}

pub fn custom_theme() -> Theme {
    Theme::dark().with_many([
        (ColorToken::Primary, Color::rgb(255, 105, 180)),
        (ColorToken::OnPrimary, Color::rgb(20, 20, 30)),
        (ColorToken::Success, Color::rgb(255, 200, 60)),
        (ColorToken::Surface, Color::rgb(38, 28, 50)),
        (ColorToken::SurfaceVariant, Color::rgb(70, 50, 90)),
        (ColorToken::OnSurface, Color::rgb(245, 235, 255)),
        (ColorToken::OnSurfaceVariant, Color::rgb(180, 150, 200)),
        (ACCENT, Color::rgb(140, 200, 220)),
    ])
}

pub fn build_widgets(world: &mut World, parent: Entity) {
    //~focus-start
    ui! {
        :(
            parent: parent
            world: world
        :)

        Row (height: 44, padding: Padding::all(12)) {
            Button (
                grow: 1.0,
                height: 36,
                border_radius: 6,
                text_color: ColorToken::OnPrimary,
                normal_color: Color::rgb(40, 50, 70),
                pressed_color: Color::rgb(20, 25, 35)
            ) [
                ThemeChoice(dark_with_accent()),
            ] on Tap {
                if let Some(theme) = ctx.world.get::<ThemeChoice>(ctx.entity).map(|c| c.0.clone()) {
                    theme::set_theme(ctx.world, theme);
                }
            }
            {
                Text ("Dark")
            }
            Button (
                grow: 1.0,
                height: 36,
                border_radius: 6,
                text_color: ColorToken::OnPrimary,
                normal_color: Color::rgb(0, 100, 200),
                pressed_color: Color::rgb(0, 70, 150)
            ) [
                ThemeChoice(light_with_accent()),
            ] on Tap {
                if let Some(theme) = ctx.world.get::<ThemeChoice>(ctx.entity).map(|c| c.0.clone()) {
                    theme::set_theme(ctx.world, theme);
                }
            }
            {
                Text ("Light")
            }
            Button (
                grow: 1.0,
                height: 36,
                border_radius: 6,
                text_color: ColorToken::OnPrimary,
                normal_color: Color::rgb(255, 105, 180),
                pressed_color: Color::rgb(200, 70, 140)
            ) [
                ThemeChoice(custom_theme()),
            ] on Tap {
                if let Some(theme) = ctx.world.get::<ThemeChoice>(ctx.entity).map(|c| c.0.clone()) {
                    theme::set_theme(ctx.world, theme);
                }
            }
            {
                Text ("Custom")
            }
        }
    };
    //~focus-end

    //~focus-start
    ui! {
        :(
            parent: parent
            world: world
        :)

        Column (grow: 1.0) {
            Row (height: 28, align: AlignItems::Center) {
                View (width: 90) {
                    Text ("Slider")
                }
                Slider (
                    min: Fixed::ZERO,
                    max: Fixed::from_int(100),
                    grow: 1.0,
                    height: 20
                )
            }
            Row (height: 36, align: AlignItems::Center) {
                View (width: 90) {
                    Text ("Switch")
                }
                Switch (width: 56, height: 28)
            }
            Row (height: 36, align: AlignItems::Center) {
                View (width: 90) {
                    Text ("Checkbox")
                }
                Checkbox (width: 24, height: 24, border_radius: 4)
            }
            Row (height: 28, align: AlignItems::Center) {
                View (width: 90) {
                    Text ("Progress")
                }
                ProgressBar (grow: 1.0, height: 12, border_radius: 6)
            }
            Row (height: 36, align: AlignItems::Center) {
                View (width: 90) {
                    Text ("Input")
                }
                TextInput (grow: 1.0, height: 28)
            }
            Row (height: 24, align: AlignItems::Center) {
                View (width: 90) {
                    Text ("Tabs")
                }
                TabBar (count: 3, grow: 1.0, height: 24) {
                    View (grow: 1.0)
                    View (grow: 1.0)
                    View (grow: 1.0)
                }
            }
            Row (height: 32, align: AlignItems::Center) {
                View (width: 120) {
                    Text ("Custom 'accent'")
                }
                View (width: 32, height: 24, border_radius: 4, bg_color: ACCENT)
            }
        }
    };
    //~focus-end

    let pbs: Vec<Entity> = world.query::<ProgressBar>().collect();
    for pb in pbs {
        if let Some(p) = world.get_mut::<ProgressBar>(pb) {
            p.value = 0.6;
        }
    }
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.with_theme(dark_with_accent());
    build_widgets(&mut app.world, parent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::IdMap;

    use crate::input::event::GestureHandler;
    use crate::input::event::gesture::GestureEvent;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        world.insert_resource(dark_with_accent());
        let parent = WidgetBuilder::new(&mut world).id();
        build_widgets(&mut world, parent);
        assert!(
            world
                .get::<crate::ui::Children>(parent)
                .is_some_and(|c| !c.0.is_empty())
        );
    }

    #[test]
    fn tap_button_swaps_global_theme() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        world.insert_resource(dark_with_accent());
        let parent = WidgetBuilder::new(&mut world).id();
        build_widgets(&mut world, parent);
        let row = world.get::<crate::ui::Children>(parent).unwrap().0[0];
        let custom_btn = world.get::<crate::ui::Children>(row).unwrap().0[2];

        GestureHandler::trigger(
            &mut world,
            custom_btn,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: custom_btn,
            },
        );
        let theme = world.resource::<Theme>().unwrap();
        assert_eq!(theme.resolve(ACCENT), Color::rgb(140, 200, 220));
    }
}
