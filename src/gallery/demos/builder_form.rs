//! Typed widget builders with reactive properties and events, composed through App without the UI DSL.

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::app::{App, RendererFactory};
use crate::ecs::Entity;
use crate::input::event::contract::Event;
#[cfg(feature = "std")]
use crate::prelude::plugin::FpsSummaryPlugin;
use crate::surface::Surface;
use crate::types::{Dimension, Fixed};
use crate::ui::Style;
use crate::ui::layout::{AlignItems, FlexDirection, JustifyContent, LayoutStyle, Padding};
use crate::ui::property::prop;
use crate::ui::theme::ColorToken;
use crate::ui::widgets::{Button, ParagraphStyle, ProgressBar, Slider, Switch, Text, TextAlign};

#[crate::model]
struct Controls {
    #[observe]
    level: Fixed,
    #[observe]
    notifications: bool,
}

#[crate::model]
impl Controls {
    fn set_level(&mut self, level: Fixed) {
        self.level = level;
    }

    fn set_notifications(&mut self, enabled: bool) {
        self.notifications = enabled;
    }

    fn reset(&mut self) {
        self.level = Fixed::ZERO;
        self.notifications = false;
    }
}

#[crate::event]
fn level_changed(
    input: event!(crate::ui::widgets::slider::ValueChanged),
    controls: model!(Controls),
) {
    controls.set_level(input.new);
}

#[crate::event]
fn toggled(input: event!(crate::ui::widgets::switch::Toggled), controls: model!(Controls)) {
    controls.set_notifications(input.now);
}

#[crate::event]
fn reset(controls: model!(Controls)) {
    controls.reset();
}

fn column_style() -> Style {
    Style {
        layout: LayoutStyle {
            direction: FlexDirection::Column,
            grow: Fixed::from_int(1),
            justify: JustifyContent::Center,
            align: AlignItems::Center,
            padding: Padding::all(20),
            row_gap: Dimension::px(14),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn row_style(height: i32) -> Style {
    Style {
        layout: LayoutStyle {
            width: Dimension::percent(100),
            max_width: Dimension::px(420),
            height: Dimension::px(height),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    #[cfg(feature = "std")]
    app.add_plugin(StdInstantClockPlugin)
        .add_plugin(FpsSummaryPlugin::default());

    //~focus-start
    let controls = app.add_model(Controls {
        level: Fixed::from_int(60),
        notifications: false,
    });
    app.compose(parent, |tree| {
        tree.children(column_style(), |c| {
            c.spawn(
                Text::build("BUILDER API")
                    .style(Style {
                        font_size: Some(20),
                        ..row_style(32)
                    })
                    .paragraph(ParagraphStyle::label().with_align(TextAlign::Start)),
            );
            c.spawn(
                Text::build("Level and notifications")
                    .style(row_style(26))
                    .paragraph(ParagraphStyle::label().with_align(TextAlign::Start)),
            );
            let source = controls.clone();
            c.spawn(
                Slider::build(Fixed::ZERO, Fixed::from_int(100))
                    .style(row_style(28))
                    .bind_property::<prop::SliderValue>(move || source.level())
                    .on_event(
                        Slider::ValueChanged,
                        level_changed::callback(controls.clone()),
                    )
                    .fill_color(ColorToken::Primary),
            );
            c.children(
                Style {
                    layout: LayoutStyle {
                        direction: FlexDirection::Row,
                        align: AlignItems::Center,
                        column_gap: Dimension::px(12),
                        ..row_style(34).layout
                    },
                    ..Default::default()
                },
                |row| {
                    row.spawn(
                        Text::build("Notifications")
                            .style(Style {
                                layout: LayoutStyle {
                                    grow: Fixed::ONE,
                                    height: Dimension::px(28),
                                    ..Default::default()
                                },
                                ..Default::default()
                            })
                            .paragraph(ParagraphStyle::label().with_align(TextAlign::Start)),
                    );
                    let source = controls.clone();
                    row.spawn(
                        Switch::build()
                            .bind_property::<prop::SwitchOn>(move || source.notifications())
                            .on_event(Switch::Toggled, toggled::callback(controls.clone()))
                            .style(Style {
                                layout: LayoutStyle {
                                    width: Dimension::px(56),
                                    height: Dimension::px(28),
                                    ..Default::default()
                                },
                                ..Default::default()
                            }),
                    );
                },
            );
            let source = controls.clone();
            c.spawn(
                ProgressBar::build()
                    .bind_property::<prop::ProgressValue>(move || source.level().to_f32() / 100.0)
                    .style(row_style(14))
                    .fill_color(ColorToken::Success),
            );
            c.spawn((
                Button::build().style(row_style(32)).on_event(
                    crate::input::event::gesture::Tap::EVENT,
                    reset::callback(controls),
                ),
                Text::label("Reset"),
            ));
        })
    });
    //~focus-end
}

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(420, 360);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Children;

    #[test]
    fn build_widgets_smoke() {
        let mut app = App::headless(320, 320);
        app.with_default_widgets();
        let parent = app.spawn_root().id();
        setup_app(&mut app, parent);
        let world = &app.world;
        let column = world
            .get::<Children>(parent)
            .and_then(|c| c.0.first().copied());
        let column = column.expect("column parented");
        let children = &world.get::<Children>(column).unwrap().0;
        assert_eq!(children.len(), 6);
        assert!(world.has::<Text>(children[0]));
        assert!(world.has::<Slider>(children[2]));
        let control_row = children[3];
        assert!(world.has::<Switch>(world.get::<Children>(control_row).unwrap().0[1]));
        assert!(world.has::<ProgressBar>(children[4]));
        assert!(world.has::<Button>(children[5]));
    }

    #[test]
    fn input_updates_model_bindings_and_reset_restores_controls() {
        use crate::input::event::gesture::GestureEvent;
        use crate::ui::ComputedRect;

        let mut app = App::headless(420, 360);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        setup_app(&mut app, root);
        let column = app.world.get::<Children>(root).unwrap().0[0];
        let children = &app.world.get::<Children>(column).unwrap().0;
        let (slider, row, progress, reset) = (children[2], children[3], children[4], children[5]);
        let switch = app.world.get::<Children>(row).unwrap().0[1];
        app.world
            .insert(slider, ComputedRect(crate::types::Rect::new(0, 0, 100, 28)));
        let tap = |target| GestureEvent::Tap {
            x: Fixed::from_int(75),
            y: Fixed::ZERO,
            target,
        };

        crate::ui::delivery::dispatch_gesture(&mut app.world, &tap(slider), 1).unwrap();
        crate::ui::delivery::dispatch_gesture(&mut app.world, &tap(switch), 1).unwrap();
        crate::ui::delivery::flush(&mut app.world).unwrap();
        crate::core::reactive::flush_signal_dirty(&mut app.world);
        assert_eq!(
            app.world.get::<Slider>(slider).unwrap().value,
            Fixed::from_int(75)
        );
        assert_eq!(app.world.get::<ProgressBar>(progress).unwrap().value, 0.75);
        assert!(app.world.get::<Switch>(switch).unwrap().on);

        crate::ui::delivery::dispatch_gesture(&mut app.world, &tap(reset), 1).unwrap();
        crate::ui::delivery::flush(&mut app.world).unwrap();
        crate::core::reactive::flush_signal_dirty(&mut app.world);
        assert_eq!(app.world.get::<Slider>(slider).unwrap().value, Fixed::ZERO);
        assert_eq!(app.world.get::<ProgressBar>(progress).unwrap().value, 0.0);
        assert!(!app.world.get::<Switch>(switch).unwrap().on);
    }
}
