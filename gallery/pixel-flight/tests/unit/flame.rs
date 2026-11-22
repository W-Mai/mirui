use super::*;

#[test]
fn pixel_flame_clip_and_shading_follow_the_nozzle() {
    let flame = Flame {
        points: [[8, 8], [8, 11], [13, 16]].map(|point| point.map(Fixed::from_int)),
        radius: Fixed::from_int(3),
        tip_scale: Fixed::ONE,
        alpha: Fixed::ONE,
        visibility: [Fixed::ONE; 3],
        ..Flame::default()
    };
    let prepare =
        |flame| ScreenFlame::new(flame).map(|screen| PixelFlame::prepare(screen, Fixed::ZERO));
    let sample = |x, y| [x, y].map(|value| Fixed::from_int(value) + Fixed::HALF);
    let origin = prepare(flame).unwrap();
    let mut shifted = flame;
    for point in &mut shifted.points {
        point[0] += Fixed::from_int(63);
        point[1] += Fixed::from_int(57);
    }
    let shifted = prepare(shifted).unwrap();
    assert!(origin.shade(sample(8, 8)).unwrap().0.a > 0);
    for y in 3..=22 {
        for x in 3..=19 {
            assert_eq!(
                origin.shade(sample(x, y)),
                shifted.shade(sample(x + 63, y + 57))
            );
        }
    }
    let [left, top, right, bottom] = ScreenFlame::new(flame)
        .unwrap()
        .native_clip([90, 90, 120, 120]);
    assert!(left >= right || top >= bottom);
    assert!(
        prepare(Flame {
            visibility: [Fixed::ZERO; 3],
            ..flame
        })
        .is_none()
    );
}

#[test]
fn prepared_tiles_reuse_storage_and_fit_the_scene_budget() {
    use super::super::{
        cockpit::Cockpit,
        flight::{Flight, Settings},
        raster::tests::reference::Raster,
        world::World,
    };
    let world = World::new(7, Fixed::from_ratio(33, 100));
    let flight = Flight::new(&world, Settings::default());
    let raster = Raster::new([480, 300], false).unwrap();
    let cockpit = Cockpit::new(480, 300, raster.scene.palette);
    let mut flames = Flames::new().unwrap();
    let total = world.bytes()
        + core::mem::size_of::<World>()
        + flight.bytes()
        + raster.bytes()
        + 480 * 300 * 4
        + cockpit.bytes()
        + flames.bytes();
    eprintln!(
        "flight resident bytes: world={} raster={} cockpit={} flames={} total={total}",
        world.bytes(),
        raster.bytes(),
        cockpit.bytes(),
        flames.bytes()
    );
    assert!(total < 16 * 1024 * 1024);
    let flame = Flame {
        points: [
            [Fixed::ZERO, Fixed::ZERO],
            [Fixed::ZERO, Fixed::from_int(5)],
            [Fixed::from_int(2), Fixed::from_int(12)],
        ],
        radius: Fixed::ONE,
        tip_scale: Fixed::ONE,
        alpha: Fixed::ONE,
        visibility: [Fixed::ONE; 3],
        seed: Fixed::ZERO,
    };
    flames.tiles[0]
        .base
        .pixels_mut()
        .unwrap()
        .bytes_mut()
        .fill(255);
    let allocations = crate::test_support::tracked_allocations(|| {
        flames
            .prepare(&[flame], flight.time, Fixed::from_int(3), [480, 300])
            .unwrap()
    });
    assert_eq!(allocations, 0);
    let tile = &flames.tiles[0];
    assert!(tile.active);
    assert!(tile.size.iter().all(|n| usize::from(*n) <= TILE));
    let [width, height] = tile.size.map(usize::from);
    let base = tile.base.read_pixels().unwrap();
    let mut samples = base.bytes()[..width * height * 4].chunks_exact(4);
    assert!(samples.clone().any(|pixel| pixel[3] == 0));
    assert!(samples.clone().any(|pixel| pixel[3] != 0));
    assert!(samples.any(|pixel| pixel[3] != 0 && pixel[..3] != [255; 3]));
    assert!(flames.tiles[1..].iter().all(|tile| !tile.active));
}
