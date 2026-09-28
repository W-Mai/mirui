use super::animation::{fps_readout_system, modal_slide_system};
use super::composition::build_widgets;
use super::state::{FpsReadout, MODAL_H, MODAL_W, ModalAnim, UPDATE_EVERY};
use crate::ecs::FrameTimings;
use crate::prelude::*;
use crate::types::Viewport;
use crate::ui::render_system::update_layout;
use crate::ui::widgets::{Text, WidgetTransform};
use crate::ui::{Children, ComputedRect, IdMap, UiScope};

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
            .is_some_and(|c| !c.0.is_empty())
    );
}

#[test]
fn modal_is_centered_and_contained_across_supported_viewports() {
    for (width, height) in [(320, 320), (320, 568), (1024, 640)] {
        let mut app = App::headless(width, height);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        app.compose(root, build_widgets);
        app.set_root(root);
        update_layout(
            &mut app.world,
            root,
            &Viewport::new(width, height, Fixed::ONE),
        );

        let panel = app.world.find_by_id("offscreen_modal_panel").unwrap();
        let rect = app.world.get::<ComputedRect>(panel).unwrap().0;
        assert!(rect.x >= Fixed::ZERO);
        assert!(rect.y >= Fixed::ZERO);
        assert!(rect.x + rect.w <= Fixed::from_int(width as i32));
        assert!(rect.y + rect.h <= Fixed::from_int(height as i32));
    }
}

#[test]
fn slide_origin_uses_the_live_layout_position() {
    let mut world = World::new();
    let panel = WidgetBuilder::new(&mut world).id();
    world.insert(panel, ModalAnim { t: Fixed::ZERO });
    world.insert(
        panel,
        ComputedRect(Rect {
            x: Fixed::from_int(412),
            y: Fixed::from_int(20),
            w: Fixed::from_int(MODAL_W),
            h: Fixed::from_int(MODAL_H),
        }),
    );

    modal_slide_system(&mut world);

    let bounce = (Fixed::ONE / 90) * Fixed::from_int(2);
    let expected = Fixed::from_int(-(412 + MODAL_W)) * (Fixed::ONE - bounce);
    assert_eq!(world.get::<WidgetTransform>(panel).unwrap().0.tx, expected);
}

#[test]
fn readout_updates_content_without_replacing_paragraph_style() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    world.insert_resource(FrameTimings {
        render_nanos: 77_000,
        ..FrameTimings::default()
    });
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    build_widgets(&mut cx);
    drop(cx);

    let readout = world.find_by_id("offscreen_modal_readout").unwrap();
    let paragraph = world.get::<Text>(readout).unwrap().paragraph().clone();
    let state = world.get_mut::<FpsReadout>(readout).unwrap();
    state.counter = UPDATE_EVERY - 1;
    state.accum_render_ns = 77_000 * (UPDATE_EVERY - 1) as u64;
    fps_readout_system(&mut world);

    let text = world.get::<Text>(readout).unwrap();
    assert!(text.resolve(&world).contains("render avg 77us"));
    assert_eq!(text.paragraph(), &paragraph);
}
