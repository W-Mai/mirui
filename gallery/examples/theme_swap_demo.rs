//! Tap a chooser button at the top to swap the active `Theme`. The
//! widget showcase below repaints with the new palette next frame.
//!
//! `Style.bg_color` and `Style.text_color` are entity-level overrides
//! and don't fall through to `Theme` on their own. This demo opts the
//! root container and row labels into theme tracking via local
//! `ThemedSurface` / `ThemedOnSurface` markers + `theme_style_system`.

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
use mirui::widget::builder::WidgetBuilder;
use mirui::widget::dirty::Dirty;
use mirui::widget::{Children, Style, Theme};
use mirui_macros::ui;

/// Per-button marker: tapping the entity carrying this swaps the
/// World's `Theme` resource to the contained palette.
struct ThemeChoice(Theme);

/// Marker for entities whose `bg_color` should track `theme.surface`
/// every frame. `Style.bg_color` is per-entity by design; demos that
/// want their root following the theme opt in here.
struct ThemedSurface;

/// Marker for entities whose `text_color` should track
/// `theme.on_surface`. Same rationale — `Style.text_color` doesn't
/// fall through to Theme on its own.
struct ThemedOnSurface;

fn theme_style_system(world: &mut World) {
    let Some(theme) = world.resource::<Theme>().copied() else {
        return;
    };

    let surfaces: Vec<Entity> = world.query::<ThemedSurface>().collect();
    for e in surfaces {
        if let Some(style) = world.get_mut::<Style>(e) {
            if style.bg_color != Some(theme.surface) {
                style.bg_color = Some(theme.surface);
                world.insert(e, Dirty);
            }
        }
    }

    let on_surfaces: Vec<Entity> = world.query::<ThemedOnSurface>().collect();
    for e in on_surfaces {
        if let Some(style) = world.get_mut::<Style>(e) {
            if style.text_color != Some(theme.on_surface) {
                style.text_color = Some(theme.on_surface);
                world.insert(e, Dirty);
            }
        }
    }
}

fn custom_theme() -> Theme {
    let mut t = Theme::dark();
    t.primary = Color::rgb(255, 105, 180);
    t.on_primary = Color::rgb(20, 20, 30);
    t.success = Color::rgb(255, 200, 60);
    t.surface = Color::rgb(38, 28, 50);
    t.surface_variant = Color::rgb(70, 50, 90);
    t.on_surface = Color::rgb(245, 235, 255);
    t.on_surface_variant = Color::rgb(180, 150, 200);
    t
}

fn theme_swap_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    if !matches!(event, GestureEvent::Tap { .. }) {
        return false;
    }
    let Some(theme) = world.get::<ThemeChoice>(entity).map(|c| c.0) else {
        return false;
    };
    world.insert_resource(theme);
    // Re-render every entity so the new palette reaches them.
    let roots: Vec<Entity> = world.query::<Children>().collect();
    for r in roots {
        mark_subtree_dirty(world, r);
    }
    true
}

fn mark_subtree_dirty(world: &mut World, entity: Entity) {
    world.insert(entity, Dirty);
    let children = world
        .get::<Children>(entity)
        .map(|c| c.0.clone())
        .unwrap_or_default();
    for c in children {
        mark_subtree_dirty(world, c);
    }
}

const W: u16 = 480;
const H: u16 = 320;

fn main() {
    let backend = SdlSurface::new("mirui — theme swap", W, H);
    let mut app = App::new(backend).with_default_widgets();
    app.add_system(theme_style_system);

    let root = WidgetBuilder::new(&mut app.world)
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(W as i32),
            height: Dimension::px(H as i32),
            padding: Padding::all(12),
            ..Default::default()
        })
        .id();
    app.world.insert(root, ThemedSurface);

    // Picker row — three Buttons, each marked with the Theme it sets.
    let chooser = ui! {
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
                text: "Dark"
            ) [
                Button::new()
                    .with_normal_color(Color::rgb(40, 50, 70))
                    .with_pressed_color(Color::rgb(20, 25, 35)),
                ThemeChoice(Theme::dark()),
                GestureHandler {
                    on_gesture: theme_swap_handler,
                },
            ] {}
            light_btn (
                grow: 1.0,
                height: 36,
                border_radius: 6,
                text: "Light"
            ) [
                Button::new()
                    .with_normal_color(Color::rgb(0, 100, 200))
                    .with_pressed_color(Color::rgb(0, 70, 150)),
                ThemeChoice(Theme::light()),
                GestureHandler {
                    on_gesture: theme_swap_handler,
                },
            ] {}
            custom_btn (
                grow: 1.0,
                height: 36,
                border_radius: 6,
                text: "Custom"
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
    let _ = chooser;

    // Showcase — every built-in widget reading its colors from Theme.
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
            slider_row (
                direction: FlexDirection::Row,
                height: 28,
                align: AlignItems::Center
            ) {
                slider_label (text: "Slider", width: 90) [
                    ThemedOnSurface,
                ] {}
                slider (grow: 1.0, height: 20) [
                    Slider::new(Fixed::ZERO, Fixed::from_int(100)),
                ] {}
            }
            switch_row (
                direction: FlexDirection::Row,
                height: 36,
                align: AlignItems::Center
            ) {
                switch_label (text: "Switch", width: 90) [
                    ThemedOnSurface,
                ] {}
                switch_widget (width: 56, height: 28) [
                    Switch::new(),
                ] {}
            }
            checkbox_row (
                direction: FlexDirection::Row,
                height: 36,
                align: AlignItems::Center
            ) {
                checkbox_label (text: "Checkbox", width: 90) [
                    ThemedOnSurface,
                ] {}
                checkbox_widget (width: 24, height: 24, border_radius: 4) [
                    Checkbox::new(),
                ] {}
            }
            progress_row (
                direction: FlexDirection::Row,
                height: 28,
                align: AlignItems::Center
            ) {
                progress_label (text: "Progress", width: 90) [
                    ThemedOnSurface,
                ] {}
                progress (grow: 1.0, height: 12, border_radius: 6) [
                    ProgressBar::new(),
                ] {}
            }
            input_row (
                direction: FlexDirection::Row,
                height: 36,
                align: AlignItems::Center
            ) {
                input_label (text: "Input", width: 90) [
                    ThemedOnSurface,
                ] {}
                input (grow: 1.0, height: 28) [
                    TextInput::new(),
                ] {}
            }
            tabs (height: 24) [
                TabBar::new(3),
            ] {
                t0 (grow: 1.0) {}
                t1 (grow: 1.0) {}
                t2 (grow: 1.0) {}
            }
        }
    };

    // Seed ProgressBar to a visible value so the theme swap shows the
    // fill colour, not just the empty track.
    let pbs: Vec<Entity> = app.world.query::<ProgressBar>().collect();
    for pb in pbs {
        if let Some(p) = app.world.get_mut::<ProgressBar>(pb) {
            p.value = 0.6;
        }
    }

    app.set_root(root);
    app.run();
}
