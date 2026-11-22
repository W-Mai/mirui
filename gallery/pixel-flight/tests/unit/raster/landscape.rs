use super::*;
use crate::{flight::Settings, raster::PlacedAircraft};

#[test]
fn terrain_frame_preparation_keeps_every_active_shadow_caster() {
    let world = World::with_size(7, Fixed::from_ratio(33, 100), 64, 32, 16);
    let mut flight = Flight::new(&world, Settings::default());
    flight.set_wingmen(2);
    flight.jets[2].position = [
        Fixed::from_int(2000),
        Fixed::from_int(-2000),
        Fixed::from_int(77),
    ];
    let palette = Palette::new(Daylight::Sunset);
    let projection = flight.projection(128, 128);
    for elevation in [0, -82, 137] {
        let prepared = PreparedTerrain::new(
            world.at(Some(mirui_sprite_stack::Pose::at(0, elevation, 0))),
            &flight,
            &projection,
            &palette,
            &PlacedAircraft::from_flight(&flight),
        );
        for (index, jet) in flight.jets[..flight.count].iter().enumerate() {
            for axis in 0..2 {
                let base = projection.position[axis]
                    .rem_euclid(Fixed::from_int(world.period() as i32))
                    + Fixed::from_int(world.period() as i32) * Fixed::from_int(2);
                assert_eq!(
                    prepared.shadows[index].center[axis],
                    base + jet.position[axis] - projection.position[axis]
                        + palette.sun[axis] * jet.position[2]
                );
                for h in elevation..=elevation + world::MAX_HEIGHT as i32 {
                    let center = prepared.shadows[index].center[axis] - palette.sun[axis] * h;
                    assert!(
                        Fixed::from_int(prepared.shadow_bounds[axis]) <= center - prepared.reach
                    );
                    assert!(
                        Fixed::from_int(prepared.shadow_bounds[axis + 2])
                            >= center + prepared.reach
                    );
                }
            }
        }
    }
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            let _ = PreparedTerrain::new(
                world.at(Some(Default::default())),
                &flight,
                &projection,
                &palette,
                &PlacedAircraft::from_flight(&flight),
            );
        }),
        0
    );
    assert!(core::mem::size_of::<PreparedTerrain>() <= 120);
    let mut placed = PlacedAircraft::from_flight(&flight);
    placed[1] = None;
    let previous = placed[2].unwrap();
    let yaw = Fixed::from_ratio(1, 3);
    let roll = Fixed::from_ratio(1, 5);
    placed[2] = Some(PlacedAircraft::new(
        previous.position,
        [yaw, Fixed::ZERO, roll],
    ));
    let (sin, cos) = yaw.sin_cos();
    let bank = roll - flight.jets[2].spin();
    let span = bank.cos().abs().max(Fixed::from_ratio(3, 10)) * flight.settings.jet_scale;
    let prepared = PreparedTerrain::new(
        world.at(Some(Default::default())),
        &flight,
        &projection,
        &palette,
        &placed,
    );
    assert_eq!(prepared.shadow_count, 2);
    let shadow = prepared.shadows[1];
    assert_eq!((shadow.sin, shadow.cos, shadow.span), (sin, cos, span));
}
