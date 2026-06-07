//! Tap a chooser button at the top to swap the active `Theme`.
//! Every built-in widget repaints in the new palette next frame
//! because their color fields default to `ColorToken` references
//! into the active theme.
//!
//! The accent dot in the corner uses a custom `accent` token —
//! adding a token doesn't require forking mirui.

use mirui::components::{Button, Checkbox, ProgressBar, Slider, Switch, TabBar, Text, TextInput};
use mirui::event::GestureHandler;
use mirui::event::gesture::GestureEvent;
use mirui::prelude::*;
use mirui::surface::sdl::SdlSurface;
use mirui::widget::Theme;
use mirui::widget::theme::{self, ColorToken};

/// Per-button marker: tapping the entity carrying this swaps the
/// World's `Theme` resource to the contained palette.
struct ThemeChoice(Theme);

/// User-defined token. The custom-themed dot in the corner reads
/// from it; the three preset themes below all bind it to a value.
const ACCENT: ColorToken = ColorToken::custom("accent");

fn dark_with_accent() -> Theme {
    Theme::dark().with(ACCENT, Color::rgb(255, 200, 60))
}

fn light_with_accent() -> Theme {
    Theme::light().with(ACCENT, Color::rgb(220, 60, 90))
}

fn custom_theme() -> Theme {
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

fn theme_swap_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    if !matches!(event, GestureEvent::Tap { .. }) {
        return false;
    }
    let Some(theme) = world.get::<ThemeChoice>(entity).map(|c| c.0.clone()) else {
        return false;
    };
    theme::set_theme(world, theme);
    true
}

const W: u16 = 480;
const H: u16 = 320;

fn main() {
    let backend = SdlSurface::new("mirui — theme swap", W, H);
    let mut app = App::new(backend);
    app.with_theme(dark_with_accent())
        .with_default_widgets()
        .with_default_systems();

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(ColorToken::Surface)
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(W as i32),
            height: Dimension::px(H as i32),
            padding: Padding::all(12),
            ..Default::default()
        })
        .id();

    let _chooser = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        Row (height: 44) {
            Button (
                grow: 1.0,
                height: 36,
                border_radius: 6,
                text_color: ColorToken::OnPrimary,
                normal_color: Color::rgb(40, 50, 70),
                pressed_color: Color::rgb(20, 25, 35)
            ) [
                ThemeChoice(dark_with_accent()),
                GestureHandler {
                    on_gesture: theme_swap_handler,
                },
            ] {
                Text ("Dark") {}
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
                GestureHandler {
                    on_gesture: theme_swap_handler,
                },
            ] {
                Text ("Light") {}
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
                GestureHandler {
                    on_gesture: theme_swap_handler,
                },
            ] {
                Text ("Custom") {}
            }
        }
    };

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        Column (grow: 1.0) {
            Row (height: 28, align: AlignItems::Center) {
                View (width: 90) {
                    Text ("Slider") {}
                }
                Slider (
                    min: Fixed::ZERO,
                    max: Fixed::from_int(100),
                    grow: 1.0,
                    height: 20
                ) {}
            }
            Row (height: 36, align: AlignItems::Center) {
                View (width: 90) {
                    Text ("Switch") {}
                }
                Switch (width: 56, height: 28) {}
            }
            Row (height: 36, align: AlignItems::Center) {
                View (width: 90) {
                    Text ("Checkbox") {}
                }
                Checkbox (width: 24, height: 24, border_radius: 4) {}
            }
            Row (height: 28, align: AlignItems::Center) {
                View (width: 90) {
                    Text ("Progress") {}
                }
                ProgressBar (grow: 1.0, height: 12, border_radius: 6) {}
            }
            Row (height: 36, align: AlignItems::Center) {
                View (width: 90) {
                    Text ("Input") {}
                }
                TextInput (grow: 1.0, height: 28) {}
            }
            Row (height: 24, align: AlignItems::Center) {
                View (width: 90) {
                    Text ("Tabs") {}
                }
                TabBar (count: 3, grow: 1.0, height: 24) {
                    View (grow: 1.0) {}
                    View (grow: 1.0) {}
                    View (grow: 1.0) {}
                }
            }
            Row (height: 32, align: AlignItems::Center) {
                View (width: 120) {
                    Text ("Custom 'accent'") {}
                }
                View (width: 32, height: 24, border_radius: 4, bg_color: ACCENT) {}
            }
        }
    };

    // Seed ProgressBar so the theme swap shows the fill colour.
    let pbs: Vec<Entity> = app.world.query::<ProgressBar>().collect();
    for pb in pbs {
        if let Some(p) = app.world.get_mut::<ProgressBar>(pb) {
            p.value = 0.6;
        }
    }

    app.set_root(root);
    app.run();
}
