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
use crate::ui::layout::{FlexDirection, LayoutStyle};
use crate::ui::spawn_children;
use crate::ui::theme::ColorToken;
use crate::ui::widgets::{ProgressBar, Slider, Switch};

fn column_style() -> Style {
    Style {
        layout: LayoutStyle {
            direction: FlexDirection::Column,
            grow: Fixed::from_int(1),
            padding: crate::ui::layout::Padding {
                top: Dimension::px(12),
                left: Dimension::px(12),
                right: Dimension::px(12),
                bottom: Dimension::px(12),
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

fn row_style(height: i32) -> Style {
    Style {
        layout: LayoutStyle {
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
            Slider::build(Fixed::ZERO, Fixed::from_int(100))
                .style(row_style(20))
                .fill_color(ColorToken::Primary),
        );
        c.spawn(Switch::build().style(row_style(26)));
        c.spawn(ProgressBar::build().value(0.6).style(row_style(12)));
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
        assert_eq!(world.get::<Children>(column).map(|c| c.0.len()), Some(3));
        assert!(world.has::<Slider>(world.get::<Children>(column).unwrap().0[0]));
    }
}
