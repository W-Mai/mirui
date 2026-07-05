use crate::prelude::*;

#[compose]
pub fn build_widgets() {
    //~focus-start
    ui! {
        Row (
            justify: JustifyContent::SpaceEvenly,
            align: AlignItems::Center,
            padding: Padding::all(20),
            grow: 1.0
        ) {
            View (bg_color: Color::rgb(88, 166, 255), width: 120, height: 80)
            View (bg_color: Color::rgb(63, 185, 80), grow: 1.0, height: 80)
            View (bg_color: Color::rgb(248, 81, 73), width: 120, height: 80)
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
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
    }
}
