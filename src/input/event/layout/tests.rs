use super::*;
use crate::input::event::runtime;
use crate::prelude::*;
use crate::test_support::tracked_allocations;
use crate::ui::ComputedRect;

#[derive(Default)]
struct Count(usize);

#[crate::event]
fn record(change: event!(SizeChanged), count: res!(mut Count)) {
    assert_eq!(change.new.height, Fixed::from_int(20));
    count.0 += 1;
}

#[test]
fn prepared_size_capture_and_dispatch_allocate_nothing_and_restore_owner() {
    let mut app = App::headless(100, 100);
    app.with_event_capacity(EventCapacity::new())
        .add_resource(Count::default());
    let root = app.spawn_root().id();
    let node = app.compose(root, |cx| {
        ui! {
            View () on SizeChanged(record::callback())
        }
    });
    app.world
        .insert(node, ComputedRect(Rect::new(0, 0, 30, 20)));
    runtime::capture_sizes(&app.world, &[node], true)
        .unwrap()
        .unwrap()
        .dispatch(&mut app.world, crate::ui::lifecycle::prepare);
    let allocations = tracked_allocations(|| {
        for width in 31..95 {
            app.world.get_mut::<ComputedRect>(node).unwrap().0.w = Fixed::from_int(width);
            runtime::capture_sizes(&app.world, &[node], true)
                .unwrap()
                .unwrap()
                .dispatch(&mut app.world, crate::ui::lifecycle::prepare);
        }
    });
    assert_eq!(allocations, 0);
    assert_eq!(app.world.resource::<Count>().unwrap().0, 65);
    assert_eq!(crate::core::reactive::current_world_id(), None);
    let batch = runtime::capture_sizes(&app.world, &[node], true)
        .unwrap()
        .unwrap();
    assert!(!batch.changed);
    assert!(matches!(
        runtime::capture_sizes(&app.world, &[node], true),
        Err(ListenerError::Busy)
    ));
    drop(batch);
    assert!(runtime::capture_sizes(&app.world, &[node], true).is_ok());
}

#[test]
fn observation_identity_does_not_transfer_to_reused_entities() {
    let mut app = App::headless(100, 100);
    app.add_resource(Count::default());
    let root = app.spawn_root().id();
    let retired = app.compose(root, |cx| {
        ui! {
            View () on SizeChanged(record::callback())
        }
    });
    app.world.despawn(retired);
    let fresh = app.compose(root, |tree| tree.spawn(crate::ui::Style::default()));
    app.world
        .insert(fresh, ComputedRect(Rect::new(0, 0, 40, 20)));
    if let Some(batch) = runtime::capture_sizes(&app.world, &[fresh], true).unwrap() {
        batch.dispatch(&mut app.world, crate::ui::lifecycle::prepare);
    }
    assert_eq!(runtime::size_observer_count(&app.world), 0);
    assert_eq!(app.world.resource::<Count>().unwrap().0, 0);
}
