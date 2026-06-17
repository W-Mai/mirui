extern crate alloc;

use crate::prelude::*;
use crate::ui::widgets::Text;

pub fn build_widgets(world: &mut World, parent: Entity) {
    //~focus-start
    ui! {
        :(
            parent: parent
            world: world
        :)

        Column (
            grow: 1.0,
            padding: Padding::all(16),
            justify: JustifyContent::Center
        ) {
            View (height: 24) {
                Text (
                    "default token",
                    font: FontToken::Default,
                    text_color: Color::rgb(220, 220, 240)
                )
            }
            View (height: 24) {
                Text (
                    "heading token",
                    font: FontToken::Heading,
                    text_color: Color::rgb(255, 200, 100)
                )
            }
            View (height: 24) {
                Text (
                    "mono token",
                    font: FontToken::Mono,
                    text_color: Color::rgb(120, 220, 160)
                )
            }
            View (height: 24) {
                Text (
                    "custom brand",
                    font: FontToken::Custom("brand"),
                    text_color: Color::rgb(255, 130, 200)
                )
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

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        world.insert_resource(crate::render::font::default_font_manager());
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
