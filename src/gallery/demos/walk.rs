use crate::prelude::*;
use crate::ui::UiScope;
use crate::ui::widgets::Text;

#[ui_scope]
pub fn build_widgets() {
    let colors = [
        Color::rgb(88, 166, 255),
        Color::rgb(63, 185, 80),
        Color::rgb(248, 81, 73),
        Color::rgb(210, 168, 255),
        Color::rgb(255, 200, 50),
    ];

    let show_footer = true;

    //~focus-start
    ui! {
        Column (grow: 1.0) {
            walk colors.iter() with color {
                View (bg_color: *color, grow: 1.0, border_radius: 4)
            }
            if show_footer {
                View (bg_color: Color::rgb(50, 50, 70), height: 30) {
                    Text ("conditional!")
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
    let mut cx = UiScope::new(&mut app.world, parent);
    build_widgets(&mut cx);
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
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
    }
}
