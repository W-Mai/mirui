//! Tap a chooser button at the top to swap the active `Theme`.
//! Every built-in widget repaints in the new palette next frame
//! because their color fields default to `ColorToken` references
//! into the active theme.
//!
//! The accent dot in the corner uses a custom `accent` token —
//! adding a token doesn't require forking mirui.

use mirui::app::App;
use mirui::components::button::Button;
use mirui::components::checkbox::Checkbox;
use mirui::components::progress_bar::ProgressBar;
use mirui::components::slider::Slider;
use mirui::components::switch::Switch;
use mirui::components::tabbar::TabBar;
use mirui::components::text_input::TextInput;
use mirui::ecs::{Entity, World};
use mirui::event::GestureHandler;
use mirui::event::gesture::GestureEvent;
use mirui::layout::*;
use mirui::surface::sdl::SdlSurface;
use mirui::types::{Color, Dimension, Fixed};
use mirui::widget::Theme;
use mirui::widget::builder::WidgetBuilder;
use mirui::widget::theme::{self, ColorToken};
use mirui_macros::ui;

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
    let mut app = App::new(backend)
        .with_theme(dark_with_accent())
        .with_default_widgets();

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

        chooser_row (
            direction: FlexDirection::Row,
            height: 44,
            padding: Padding::all(0)
        ) {
            dark_btn (
                grow: 1.0,
                height: 36,
                border_radius: 6,
                text: "Dark",
                text_color: ColorToken::OnPrimary
            ) [
                Button::new()
                    .with_normal_color(Color::rgb(40, 50, 70))
                    .with_pressed_color(Color::rgb(20, 25, 35)),
                ThemeChoice(dark_with_accent()),
                GestureHandler {
                    on_gesture: theme_swap_handler,
                },
            ] {}
            light_btn (
                grow: 1.0,
                height: 36,
                border_radius: 6,
                text: "Light",
                text_color: ColorToken::OnPrimary
            ) [
                Button::new()
                    .with_normal_color(Color::rgb(0, 100, 200))
                    .with_pressed_color(Color::rgb(0, 70, 150)),
                ThemeChoice(light_with_accent()),
                GestureHandler {
                    on_gesture: theme_swap_handler,
                },
            ] {}
            custom_btn (
                grow: 1.0,
                height: 36,
                border_radius: 6,
                text: "Custom",
                text_color: ColorToken::OnPrimary
            ) [
                Button::new()
                    .with_normal_color(Color::rgb(255, 105, 180))
                    .with_pressed_color(Color::rgb(200, 70, 140)),
                ThemeChoice(custom_theme()),
                GestureHandler {
                    on_gesture: theme_swap_handler,
                },
            ] {}
        }
    };

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        showcase (
            direction: FlexDirection::Column,
            grow: 1.0,
            padding: Padding::all(0)
        ) {
            slider_row (direction: FlexDirection::Row, height: 28, align: AlignItems::Center) {
                slider_label (text: "Slider", width: 90) {}
                slider (grow: 1.0, height: 20) [
                    Slider::new(Fixed::ZERO, Fixed::from_int(100)),
                ] {}
            }
            switch_row (direction: FlexDirection::Row, height: 36, align: AlignItems::Center) {
                switch_label (text: "Switch", width: 90) {}
                switch_widget (width: 56, height: 28) [
                    Switch::new(),
                ] {}
            }
            checkbox_row (direction: FlexDirection::Row, height: 36, align: AlignItems::Center) {
                checkbox_label (text: "Checkbox", width: 90) {}
                checkbox_widget (width: 24, height: 24, border_radius: 4) [
                    Checkbox::new(),
                ] {}
            }
            progress_row (direction: FlexDirection::Row, height: 28, align: AlignItems::Center) {
                progress_label (text: "Progress", width: 90) {}
                progress (grow: 1.0, height: 12, border_radius: 6) [
                    ProgressBar::new(),
                ] {}
            }
            input_row (direction: FlexDirection::Row, height: 36, align: AlignItems::Center) {
                input_label (text: "Input", width: 90) {}
                input (grow: 1.0, height: 28) [
                    TextInput::new(),
                ] {}
            }
            tabs_row (direction: FlexDirection::Row, height: 24, align: AlignItems::Center) {
                tabs_label (text: "Tabs", width: 90) {}
                tabs (grow: 1.0, height: 24) [
                    TabBar::new(3),
                ] {
                    t0 (grow: 1.0) {}
                    t1 (grow: 1.0) {}
                    t2 (grow: 1.0) {}
                }
            }
            accent_row (direction: FlexDirection::Row, height: 32, align: AlignItems::Center) {
                accent_label (text: "Custom 'accent'", width: 120) {}
                accent_block (width: 32, height: 24, border_radius: 4, bg_color: ACCENT) {}
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
