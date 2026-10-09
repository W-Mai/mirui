#![cfg(feature = "gallery")]

use mirui::ecs::World;
use mirui::prelude::App;
use mirui::ui::{Children, Parent};

#[test]
fn all_demos_build_widgets_smoke() {
    macro_rules! smoke_composition {
        ($demo:path) => {{
            let mut app = App::headless(480, 320);
            app.with_default_widgets().with_default_systems();
            let root = app.spawn_root().id();
            app.compose(root, $demo());
            assert_demo_built(&app.world, root);
        }};
    }

    smoke_composition!(mirui::gallery::demos::animation::build_widgets);
    smoke_composition!(mirui::gallery::demos::book_flip::build_widgets);
    smoke_composition!(mirui::gallery::demos::image_flip::build_widgets);
    smoke_composition!(mirui::gallery::demos::lazy_list::build_widgets);
    smoke_composition!(mirui::gallery::demos::nested_scroll::build_widgets);
    smoke_composition!(mirui::gallery::demos::offscreen::build_widgets);
    smoke_composition!(mirui::gallery::demos::offscreen_modal::build_widgets);
    smoke_composition!(mirui::gallery::demos::scroll::build_widgets);
    smoke_composition!(mirui::gallery::demos::slider_value_changed::build_widgets);
    smoke_composition!(mirui::gallery::demos::spatial_anim::build_widgets);
    smoke_composition!(mirui::gallery::demos::tabbar::build_widgets);
    smoke_composition!(mirui::gallery::demos::text_input::build_widgets);
    smoke_composition!(mirui::gallery::demos::transform::build_widgets);
    smoke_composition!(mirui::gallery::demos::widgets_compact::build_widgets);
}

#[test]
fn pinch_rotate_demo_smoke() {
    let (width, height) = mirui::gallery::demos::pinch_rotate::DEFAULT_VIEW;
    let mut app = App::headless(width, height);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    mirui::gallery::demos::pinch_rotate::setup_app(&mut app, root);
    assert_demo_built(&app.world, root);
}

fn assert_demo_built(world: &World, parent: mirui::ecs::Entity) {
    assert!(
        world
            .get::<Children>(parent)
            .is_some_and(|c| !c.0.is_empty()),
        "demo did not add any children to parent",
    );
    for &child in &world.get::<Children>(parent).unwrap().0 {
        assert_eq!(
            world.get::<Parent>(child).map(|value| value.0),
            Some(parent)
        );
    }
}

#[test]
fn custom_view_demo_smoke() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    app.with_widget(mirui::gallery::demos::custom_view::diamond_view());
    let root = app.spawn_root().id();
    app.compose(root, mirui::gallery::demos::custom_view::build_widgets());
    assert_demo_built(&app.world, root);
}

#[test]
fn widgets_demo_smoke() {
    let (w, h) = mirui::gallery::demos::widgets::DEFAULT_VIEW;
    let mut app = App::headless(w, h);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    app.compose(root, mirui::gallery::demos::widgets::build_widgets());
    assert_demo_built(&app.world, root);
}

#[test]
fn flip_card_demo_smoke() {
    let (w, h) = mirui::gallery::demos::flip_card::DEFAULT_VIEW;
    let mut app = App::headless(w, h);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    app.compose(root, mirui::gallery::demos::flip_card::build_widgets());
    assert_demo_built(&app.world, root);
}

#[test]
fn cover_flow_demo_smoke() {
    let (w, h) = mirui::gallery::demos::cover_flow::DEFAULT_VIEW;
    let mut app = App::headless(w, h);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    app.compose(root, mirui::gallery::demos::cover_flow::build_widgets(w, h));
    assert_demo_built(&app.world, root);
}

#[test]
fn effect_panels_demo_smoke() {
    let (w, h) = mirui::gallery::demos::effect_panels::DEFAULT_VIEW;
    let mut app = App::headless(w, h);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    app.compose(root, mirui::gallery::demos::effect_panels::build_widgets());
    assert_demo_built(&app.world, root);
}

#[test]
fn effect_glass_demo_smoke() {
    let (w, h) = mirui::gallery::demos::effect_glass::DEFAULT_VIEW;
    let mut app = App::headless(w, h);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    app.compose(root, mirui::gallery::demos::effect_glass::build_widgets());
    assert_demo_built(&app.world, root);
}

#[test]
fn three_body_demo_smoke() {
    use mirui::types::Fixed;
    let mut app = App::headless(128, 128);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    app.compose(
        root,
        mirui::gallery::demos::three_body::build_widgets(128, 128, 3, Fixed::from_int(30)),
    );
    assert_demo_built(&app.world, root);
}

#[test]
fn particles_demo_smoke() {
    let (w, h) = mirui::gallery::demos::particles::DEFAULT_VIEW;
    let mut app = App::headless(w, h);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    app.compose(root, mirui::gallery::demos::particles::build_widgets());
    assert_demo_built(&app.world, root);
}

#[test]
fn subpixel_demo_smoke() {
    let (w, h) = mirui::gallery::demos::subpixel::DEFAULT_VIEW;
    let mut app = App::headless(w, h);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    app.compose(root, mirui::gallery::demos::subpixel::build_widgets());
    assert_demo_built(&app.world, root);
}

#[test]
fn shapes_demo_smoke() {
    let mut app = App::headless(480, 480);
    app.with_default_widgets().with_default_systems();
    app.with_widget(mirui::gallery::demos::shapes::shapes_view());
    let root = app.spawn_root().id();
    app.compose(root, mirui::gallery::demos::shapes::build_widgets());
    assert_demo_built(&app.world, root);
}

#[test]
fn butterfly_demo_smoke() {
    let mut app = App::headless(480, 480);
    app.with_default_widgets().with_default_systems();
    app.with_widget(mirui::gallery::demos::butterfly::butterfly_view());
    let root = app.spawn_root().id();
    app.compose(root, mirui::gallery::demos::butterfly::build_widgets());
    assert_demo_built(&app.world, root);
}
