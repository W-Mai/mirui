#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::components::Text;
use crate::ecs::{Entity, World};
#[cfg(feature = "std")]
use crate::plugins::input_feedback::InputFeedbackPlugin;
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::types::Fixed;
use crate::widget::Style;

/// Hello world card layout: a header card and a body card stacked
/// vertically with rounded corners and contrasting backgrounds.
///
/// # Required plugins
/// - [`InputFeedbackPlugin`] (for the cursor / rotary feedback overlay)
pub fn build_widgets(world: &mut World, parent: Entity) {
    if let Some(style) = world.get_mut::<Style>(parent) {
        style.bg_color = Some(Color::rgb(20, 22, 30).into());
        style.layout = LayoutStyle {
            direction: FlexDirection::Column,
            padding: Padding::all(24),
            grow: Fixed::ONE,
            ..Default::default()
        };
    }
    ui! {
        :(
            parent: parent
            world: world
        :)

        Column (grow: 1.0) {
            View (
                bg_color: Color::rgb(38, 50, 70),
                text_color: Color::rgb(220, 220, 240),
                border_radius: 12,
                padding: Padding::all(16)
            ) {
                Text ("mirui hello") {}
            }
            View (height: 16) {}
            View (
                bg_color: Color::rgb(60, 80, 110),
                border_color: Color::rgb(120, 160, 220),
                border_width: 2,
                border_radius: 16,
                padding: Padding::all(20),
                grow: 1.0
            ) {
                Text (
                    "tap to interact",
                    text_color: Color::rgb(240, 240, 255)
                ) {}
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
    app.add_plugin(InputFeedbackPlugin::new());
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
            "demo did not add any children to parent",
        );
    }
}
