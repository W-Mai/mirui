#![cfg(feature = "gallery")]

use mirui::ecs::World;
use mirui::widget::Children;
use mirui::widget::builder::WidgetBuilder;

#[test]
fn all_demos_build_widgets_smoke() {
    macro_rules! smoke {
        ($demo:path) => {{
            let mut world = World::new();
            let parent = WidgetBuilder::new(&mut world).id();
            let root = $demo(&mut world, parent);
            assert_ne!(root, parent);
            assert!(
                world
                    .get::<Children>(parent)
                    .is_some_and(|c| c.0.contains(&root)),
                "demo root not added under parent",
            );
        }};
    }

    smoke!(mirui::gallery::demos::hello::build_widgets);
    smoke!(mirui::gallery::demos::on_handlers::build_widgets);
    smoke!(mirui::gallery::demos::slider_value_changed::build_widgets);
    smoke!(mirui::gallery::demos::toggle::build_widgets);
    smoke!(mirui::gallery::demos::tabbar_selection::build_widgets);
}
