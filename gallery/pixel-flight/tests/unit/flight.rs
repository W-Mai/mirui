use super::*;
#[test]
fn camera_retargets_without_teleporting_and_all_views_settle() {
    let world = World::new(7, Fixed::from_ratio(33, 100));
    let mut flight = Flight::new(&world, Settings::default());
    for target in [View::Chase, View::Cockpit, View::Overhead, View::Cockpit] {
        let before = flight.projection(427, 240);
        flight.view = target;
        assert_eq!(
            before.camera.center,
            flight.projection(427, 240).camera.center
        );
        for _ in 0..400 {
            flight.step(&world, 16, 427, 240);
            let p = flight.projection(427, 240);
            assert!(
                p.camera
                    .center
                    .into_iter()
                    .chain(p.position)
                    .all(|v| v.abs() < Fixed::from_int(1_000_000))
            );
        }
        assert_eq!(
            flight.cockpit[2].to_fixed(),
            Fixed::from_int((target == View::Cockpit) as i32)
        );
        assert_eq!(
            flight.chase[2].to_fixed(),
            Fixed::from_int((target == View::Chase) as i32)
        );
    }
    for i in 0..300 {
        flight.view = if i % 2 == 0 {
            View::Chase
        } else {
            View::Cockpit
        };
        flight.step(&world, 16, 427, 240);
        assert!(flight.chase[2] + flight.cockpit[2] <= Fixed64::from_ratio(100001, 100000));
    }
}
#[test]
fn controls_cancel_and_history_storage_remain_bounded() {
    // Model moves must not carry the three fixed aircraft history buffers.
    assert!(core::mem::size_of::<Flight>() < 8 * 1024);
    let world = World::new(7, Fixed::from_ratio(33, 100));
    let mut flight = Flight::new(&world, Settings::default());
    flight.set_wingmen(2);
    flight.controls.key(0, true);
    flight.controls.key(1, true);
    flight.controls.key(0, false);
    assert_eq!(flight.controls.keys, 2);
    flight.controls.press();
    flight.release_pointer(true);
    assert!(flight.jets[0].stunt < Fixed::from_ratio(0, 1));
    flight.controls.press();
    flight.release_pointer(false);
    assert_eq!(flight.jets[0].stunt, Fixed::from_ratio(0, 1));
    flight.controls.release_all();
    assert_eq!(flight.controls.keys, 0);
    let allocations = crate::test_support::tracked_allocations(|| {
        for _ in 0..2000 {
            flight.step(&world, 16, 427, 240);
        }
    });
    assert_eq!(allocations, 0);
    assert_eq!(flight.time, Fixed::from_int(32));
    for jet in flight.jets.iter() {
        assert!(jet.trail_len <= TRAIL_CAPACITY);
        assert!(jet.trail_head < TRAIL_CAPACITY);
        assert!(
            jet.position
                .iter()
                .all(|v| v.abs() < Fixed::from_int(1_000_000))
        );
    }
    assert!(flight.smoke_head < SMOKE_CAPACITY);
}
