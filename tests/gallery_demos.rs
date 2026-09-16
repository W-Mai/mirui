#![cfg(feature = "gallery")]

use mirui::ecs::World;
use mirui::ui::Children;
use mirui::ui::UiScope;
use mirui::ui::builder::WidgetBuilder;

#[test]
fn all_demos_build_widgets_smoke() {
    macro_rules! smoke_scoped {
        ($demo:path) => {{
            let mut world = World::new();
            let parent = WidgetBuilder::new(&mut world).id();
            let mut cx = UiScope::new(&mut world, parent);
            $demo(&mut cx);
            assert!(
                world
                    .get::<Children>(parent)
                    .is_some_and(|c| !c.0.is_empty()),
                "demo did not add any children to parent",
            );
        }};
    }

    smoke_scoped!(mirui::gallery::demos::animation::build_widgets);
    smoke_scoped!(mirui::gallery::demos::book_flip::build_widgets);
    smoke_scoped!(mirui::gallery::demos::image_flip::build_widgets);
    smoke_scoped!(mirui::gallery::demos::lazy_list::build_widgets);
    smoke_scoped!(mirui::gallery::demos::nested_scroll::build_widgets);
    smoke_scoped!(mirui::gallery::demos::offscreen::build_widgets);
    smoke_scoped!(mirui::gallery::demos::offscreen_modal::build_widgets);
    smoke_scoped!(mirui::gallery::demos::pinch_rotate::build_widgets);
    smoke_scoped!(mirui::gallery::demos::scroll::build_widgets);
    smoke_scoped!(mirui::gallery::demos::slider_value_changed::build_widgets);
    smoke_scoped!(mirui::gallery::demos::spatial_anim::build_widgets);
    smoke_scoped!(mirui::gallery::demos::tabbar::build_widgets);
    smoke_scoped!(mirui::gallery::demos::text_input::build_widgets);
    smoke_scoped!(mirui::gallery::demos::transform::build_widgets);
}

fn assert_demo_built(world: &World, parent: mirui::ecs::Entity) {
    assert!(
        world
            .get::<Children>(parent)
            .is_some_and(|c| !c.0.is_empty()),
        "demo did not add any children to parent",
    );
}

#[test]
fn custom_view_demo_smoke() {
    use mirui::ui::view::ViewRegistry;
    let mut world = World::new();
    let mut reg = ViewRegistry::with_builtins();
    reg.insert(mirui::gallery::demos::custom_view::diamond_view());
    world.insert_resource(reg);
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    mirui::gallery::demos::custom_view::build_widgets(&mut cx);
    assert_demo_built(&world, parent);
}

#[test]
fn widgets_demo_smoke() {
    let (w, h) = mirui::gallery::demos::widgets::DEFAULT_VIEW;
    let mut world = World::new();
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    mirui::gallery::demos::widgets::build_widgets(&mut cx, w, h);
    drop(cx);
    assert_demo_built(&world, parent);
}

#[test]
fn flip_card_demo_smoke() {
    let mut world = World::new();
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    mirui::gallery::demos::flip_card::build_widgets(&mut cx);
    drop(cx);
    assert_demo_built(&world, parent);
}

#[test]
fn cover_flow_demo_smoke() {
    let (w, h) = mirui::gallery::demos::cover_flow::DEFAULT_VIEW;
    let mut world = World::new();
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    mirui::gallery::demos::cover_flow::build_widgets(&mut cx, w, h);
    drop(cx);
    assert_demo_built(&world, parent);
}

#[test]
fn effect_panels_demo_smoke() {
    let mut world = World::new();
    world.insert_resource(mirui::ui::IdMap::new());
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    mirui::gallery::demos::effect_panels::build_widgets(&mut cx);
    drop(cx);
    assert_demo_built(&world, parent);
}

#[test]
fn effect_glass_demo_smoke() {
    let mut world = World::new();
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    mirui::gallery::demos::effect_glass::build_widgets(&mut cx);
    drop(cx);
    assert_demo_built(&world, parent);
}

#[test]
fn three_body_demo_smoke() {
    use mirui::types::Fixed;
    let mut world = World::new();
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    mirui::gallery::demos::three_body::build_widgets(&mut cx, 128, 128, 3, Fixed::from_int(30));
    drop(cx);
    assert_demo_built(&world, parent);
}

#[test]
fn particles_demo_smoke() {
    let mut world = World::new();
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    mirui::gallery::demos::particles::build_widgets(&mut cx);
    drop(cx);
    assert_demo_built(&world, parent);
}

#[test]
fn subpixel_demo_smoke() {
    let mut world = World::new();
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    mirui::gallery::demos::subpixel::build_widgets(&mut cx);
    drop(cx);
    assert_demo_built(&world, parent);
}

#[test]
fn shapes_demo_smoke() {
    use mirui::ui::view::ViewRegistry;
    let mut world = World::new();
    let mut reg = ViewRegistry::with_builtins();
    reg.insert(mirui::gallery::demos::shapes::shapes_view());
    world.insert_resource(reg);
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    mirui::gallery::demos::shapes::build_widgets(&mut cx);
    drop(cx);
    assert_demo_built(&world, parent);
}

#[test]
fn butterfly_demo_smoke() {
    use mirui::ui::view::ViewRegistry;
    let mut world = World::new();
    let mut reg = ViewRegistry::with_builtins();
    reg.insert(mirui::gallery::demos::butterfly::butterfly_view());
    world.insert_resource(reg);
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    mirui::gallery::demos::butterfly::build_widgets(&mut cx);
    drop(cx);
    assert_demo_built(&world, parent);
}
