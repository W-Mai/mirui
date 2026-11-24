use super::queued_surface::QueuedSurface;
use super::*;
use crate::input::event::contract::Event;
use crate::input::event::gesture::Tap;
use crate::types::{Dimension, Fixed};
use crate::ui::builder::WidgetBuilder;
use crate::ui::layout::LayoutStyle;
use crate::ui::{Children, HitTarget, Parent};

#[derive(Default)]
struct TapCount(u8);

#[crate::event]
fn count_tap(count: res!(mut TapCount)) -> bool {
    count.0 += 1;
    true
}

#[test]
fn fractional_scale_input_uses_the_layout_logical_size() {
    let surface = QueuedSurface::new(128, 277, Fixed::from_f32(3.25));
    let mut app = App::with_factory(surface, SwRendererFactory::new());
    app.with_default_widgets().with_default_systems();
    app.world.insert_resource(TapCount::default());
    let root = app.spawn_root().id();
    let target = WidgetBuilder::new(&mut app.world)
        .layout(LayoutStyle {
            width: Dimension::px(32),
            height: Dimension::px(32),
            ..LayoutStyle::default()
        })
        .id();
    app.world.insert(target, HitTarget);
    crate::input::event::binding::install_entity_event(
        &mut app.world,
        target,
        Tap::EVENT,
        count_tap::callback::<Tap, crate::input::event::callback::NodeContext>(),
    );
    app.world.insert(target, Parent(root));
    app.world.get_mut::<Children>(root).unwrap().0.push(target);

    app.render().unwrap();
    app.backend.events.extend([
        InputEvent::PointerDown {
            kind: crate::input::event::input::PointerKind::Mouse,
            id: 0,
            x: Fixed::from_int(8),
            y: Fixed::from_int(8),
        },
        InputEvent::PointerUp {
            kind: crate::input::event::input::PointerKind::Mouse,
            id: 0,
            x: Fixed::from_int(8),
            y: Fixed::from_int(8),
        },
    ]);
    let original = app
        .world
        .resource::<GestureSystem>()
        .unwrap()
        .events
        .buffer
        .as_ptr();
    app.tick();

    assert_eq!(app.world.resource::<TapCount>().unwrap().0, 1);
    assert_ne!(
        app.world
            .resource::<GestureSystem>()
            .unwrap()
            .events
            .buffer
            .as_ptr(),
        original
    );
    for _ in 0..3 {
        app.backend.events.extend([
            InputEvent::PointerDown {
                kind: crate::input::event::input::PointerKind::Mouse,
                id: 0,
                x: Fixed::from_int(8),
                y: Fixed::from_int(8),
            },
            InputEvent::PointerUp {
                kind: crate::input::event::input::PointerKind::Mouse,
                id: 0,
                x: Fixed::from_int(8),
                y: Fixed::from_int(8),
            },
        ]);
    }
    app.tick();
    assert_eq!(app.world.resource::<TapCount>().unwrap().0, 4);
    assert_eq!(
        app.world
            .resource::<GestureSystem>()
            .unwrap()
            .events
            .buffer
            .as_ptr(),
        original
    );
}
