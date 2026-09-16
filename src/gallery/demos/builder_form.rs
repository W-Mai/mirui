//! Built entirely through the widget builder API — `X::build(..)` +
//! `world.spawn(..)` + `spawn_children` closures — with no `ui!` macro.
//! Every other demo uses the DSL; this one shows the hand-written path
//! that non-macro users (and the macro itself) lower to.

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, World};
#[cfg(feature = "std")]
use crate::prelude::plugin::FpsSummaryPlugin;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::types::{Dimension, Fixed};
use crate::ui::Style;
use crate::ui::layout::{AlignItems, FlexDirection, JustifyContent, LayoutStyle, Padding};
use crate::ui::spawn_children;
use crate::ui::theme::ColorToken;
use crate::ui::widgets::{ParagraphStyle, ProgressBar, Slider, Switch, Text, TextAlign};

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

pub fn build_widgets(world: &mut World, parent: Entity) {
    //~focus-start
    let column = spawn_children(world, column_style(), |c| {
        c.spawn(
            Text::build("BUILDER API")
                .style(Style {
                    font_size: Some(20),
                    ..row_style(32)
                })
                .paragraph(ParagraphStyle::label().with_align(TextAlign::Start)),
        );
        c.spawn(
            Text::build("The same widget tree without ui!")
                .style(row_style(26))
                .paragraph(ParagraphStyle::label().with_align(TextAlign::Start)),
        );
        c.spawn(
            Slider::build(Fixed::ZERO, Fixed::from_int(100))
                .style(row_style(28))
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
                row.spawn(Switch::build().style(Style {
                    layout: LayoutStyle {
                        width: Dimension::px(56),
                        height: Dimension::px(28),
                        ..Default::default()
                    },
                    ..Default::default()
                }));
            },
        );
        c.spawn(
            ProgressBar::build()
                .value(0.6)
                .style(row_style(14))
                .fill_color(ColorToken::Success),
        );
    });
    //~focus-end

    world.insert(column, crate::ui::Parent(parent));
    if let Some(children) = world.get_mut::<crate::ui::Children>(parent) {
        children.0.push(column);
    }
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_plugin(StdInstantClockPlugin)
        .add_plugin(FpsSummaryPlugin::default());
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
        let column = world
            .get::<Children>(parent)
            .and_then(|c| c.0.first().copied());
        let column = column.expect("column parented");
        let children = &world.get::<Children>(column).unwrap().0;
        assert_eq!(children.len(), 5);
        assert!(world.has::<Text>(children[0]));
        assert!(world.has::<Slider>(children[2]));
        let control_row = children[3];
        assert!(world.has::<Switch>(world.get::<Children>(control_row).unwrap().0[1]));
        assert!(world.has::<ProgressBar>(children[4]));
    }
}
