use super::*;
use crate::input::event::GestureHandler;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::*;
use crate::ui::Children;
use crate::ui::IdMap;
use crate::ui::UiScope;
use crate::ui::view::ViewRegistry;
use core::any::TypeId;

#[test]
fn diamond_view_keeps_its_dispatch_contract() {
    let view = diamond_view();
    assert_eq!(view.name(), "Diamond");
    assert_eq!(view.priority(), 60);
    assert_eq!(view.component_filter(), Some(TypeId::of::<Diamond>()));
}

#[test]
fn build_widgets_smoke() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let mut reg = ViewRegistry::with_builtins();
    reg.insert(diamond_view());
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
    world.insert_resource(IdMap::new());
    let mut reg = ViewRegistry::with_builtins();
    reg.insert(diamond_view());
    world.insert_resource(reg);
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    build_widgets(&mut cx);
    let row = world.find_by_id("custom_view_tiles").unwrap();
    let d0 = world.get::<Children>(row).unwrap().0[0];

    assert_eq!(world.get::<Diamond>(d0).map(|d| d.color), Some(PALETTE[0]));
    GestureHandler::trigger(
        &mut world,
        d0,
        &GestureEvent::Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target: d0,
        },
    );
    assert_eq!(world.get::<Diamond>(d0).map(|d| d.color), Some(PALETTE[1]));
}
