use super::view::diamond_render;
use super::*;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::*;
use crate::ui::Children;
use crate::ui::IdMap;
use crate::ui::UiScope;
use crate::ui::view::ViewRegistry;
use core::any::TypeId;

#[test]
fn diamond_view_keeps_its_dispatch_contract() {
    let view = diamond_render::view();
    assert_eq!(view.name(), "Diamond");
    assert_eq!(view.priority(), Some(60));
    assert_eq!(view.component_filter(), Some(TypeId::of::<Diamond>()));
}

#[test]
fn build_widgets_smoke() {
    let mut world = World::new();
    crate::input::event::runtime::configure(
        &mut world,
        EventCapacity::new().commands(CommandCapacity::new().insert::<super::view::Diamond>(1)),
    )
    .unwrap();
    world.insert_resource(IdMap::new());
    let mut reg = ViewRegistry::with_builtins();
    reg.insert(diamond_render::view());
    world.insert_resource(reg);
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    build_widgets(&mut cx);
    assert!(
        world
            .get::<Children>(parent)
            .is_some_and(|c| !c.0.is_empty()),
    );
}

#[test]
fn diamond_geometry_stays_in_static_storage() {
    assert!(super::view::DIAMOND_PATH.is_borrowed());
}

#[test]
fn tap_cycles_diamond_color() {
    let mut world = World::new();
    crate::input::event::runtime::configure(
        &mut world,
        EventCapacity::new().commands(CommandCapacity::new().insert::<super::view::Diamond>(1)),
    )
    .unwrap();
    world.insert_resource(IdMap::new());
    let mut reg = ViewRegistry::with_builtins();
    reg.insert(diamond_render::view());
    world.insert_resource(reg);
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    build_widgets(&mut cx);
    let row = world.find_by_id("custom_view_tiles").unwrap();
    let d0 = world.get::<Children>(row).unwrap().0[0];

    assert_eq!(world.get::<Diamond>(d0).map(|d| d.color), Some(PALETTE[0]));
    {
        let decision = crate::__macro_support::dispatch_entity_gesture(
            &mut world,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: d0,
            },
            0,
        )
        .unwrap();
        crate::__macro_support::flush_entity_events(&mut world).unwrap();
        assert_eq!(decision, crate::input::event::BubbleControl::Prevent);
    };
    assert_eq!(world.get::<Diamond>(d0).map(|d| d.color), Some(PALETTE[1]));
}
