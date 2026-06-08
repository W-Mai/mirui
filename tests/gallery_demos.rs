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

    smoke!(mirui::gallery::demos::absolute::build_widgets);
    smoke!(mirui::gallery::demos::animation::build_widgets);
    smoke!(mirui::gallery::demos::app_demo::build_widgets);
    smoke!(mirui::gallery::demos::book_flip::build_widgets);
    smoke!(mirui::gallery::demos::click::build_widgets);
    smoke!(mirui::gallery::demos::components::build_widgets);
    smoke!(mirui::gallery::demos::disabled::build_widgets);
    smoke!(mirui::gallery::demos::dsl::build_widgets);
    smoke!(mirui::gallery::demos::enchants::build_widgets);
    smoke!(mirui::gallery::demos::flip_card::build_widgets);
    smoke!(mirui::gallery::demos::gesture::build_widgets);
    smoke!(mirui::gallery::demos::hello::build_widgets);
    smoke!(mirui::gallery::demos::hover_tour::build_widgets);
    smoke!(mirui::gallery::demos::image_flip::build_widgets);
    smoke!(mirui::gallery::demos::input_feedback::build_widgets);
    smoke!(mirui::gallery::demos::interactive_states::build_widgets);
    smoke!(mirui::gallery::demos::lazy_list::build_widgets);
    smoke!(mirui::gallery::demos::nested_scroll::build_widgets);
    smoke!(mirui::gallery::demos::on_handlers::build_widgets);
    smoke!(mirui::gallery::demos::pinch_rotate::build_widgets);
    smoke!(mirui::gallery::demos::scroll::build_widgets);
    smoke!(mirui::gallery::demos::slider_switch::build_widgets);
    smoke!(mirui::gallery::demos::slider_value_changed::build_widgets);
    smoke!(mirui::gallery::demos::spatial_anim::build_widgets);
    smoke!(mirui::gallery::demos::tabbar::build_widgets);
    smoke!(mirui::gallery::demos::tabbar_selection::build_widgets);
    smoke!(mirui::gallery::demos::text::build_widgets);
    smoke!(mirui::gallery::demos::text_input::build_widgets);
    smoke!(mirui::gallery::demos::toggle::build_widgets);
    smoke!(mirui::gallery::demos::transform::build_widgets);
    smoke!(mirui::gallery::demos::walk::build_widgets);
}

#[test]
fn custom_view_demo_smoke() {
    use mirui::widget::view::ViewRegistry;
    let mut world = World::new();
    let mut reg = ViewRegistry::with_builtins();
    reg.insert(mirui::gallery::demos::custom_view::diamond_view());
    world.insert_resource(reg);
    let parent = WidgetBuilder::new(&mut world).id();
    let root = mirui::gallery::demos::custom_view::build_widgets(&mut world, parent);
    assert_ne!(root, parent);
}

#[test]
fn widgets_demo_smoke() {
    let (w, h) = mirui::gallery::demos::widgets::DEFAULT_VIEW;
    let mut world = World::new();
    let parent = WidgetBuilder::new(&mut world).id();
    let root = mirui::gallery::demos::widgets::build_widgets(&mut world, parent, w, h);
    assert_ne!(root, parent);
}
