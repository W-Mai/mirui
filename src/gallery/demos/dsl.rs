use crate::prelude::*;

#[compose]
fn header() -> Entity {
    //~focus-start
    ui! {
        View (
            bg_color: Color::rgb(88, 166, 255),
            height: 40,
            text: "Hello xrune!",
            border_radius: 8
        )
    }
    //~focus-end
}

#[compose]
fn button_row() -> Entity {
    //~focus-start
    ui! {
        Row (grow: 1.0) {
            View (bg_color: Color::rgb(63, 185, 80), grow: 1.0, text: "OK", border_radius: 6)
            View (bg_color: Color::rgb(248, 81, 73), grow: 1.0, text: "Cancel", border_radius: 6)
            View (bg_color: Color::rgb(210, 168, 255), grow: 1.0, text: "Maybe", border_radius: 6)
        }
    }
    //~focus-end
}

#[compose]
fn footer() -> Entity {
    //~focus-start
    ui! {
        View (
            bg_color: Color::rgb(50, 50, 70),
            height: 30,
            text: "Built with mirui + xrune"
        )
    }
    //~focus-end
}

#[compose]
pub fn build_widgets() {
    ui!(header());
    ui!(button_row());
    ui!(footer());
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.compose(parent, build_widgets);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Children;
    use crate::ui::IdMap;
    use crate::ui::UiScope;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
        drop(cx);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
    }
}
