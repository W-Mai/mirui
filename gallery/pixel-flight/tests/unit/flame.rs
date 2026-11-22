use super::*;

#[test]
fn puff_coefficients() {
    for (i, &weight) in EXTINCTION.iter().enumerate() {
        let t = (Fixed::from_int(i as i32) + Fixed::HALF) / Fixed::from_int(PUFFS as i32);
        assert_eq!(
            Fixed::from_ratio(i32::from(weight), 256),
            (Fixed::ONE - t).pow_fraction(Fixed::from_ratio(75, 100))
        );
    }
}
#[test]
fn density_row_admission_never_rejects_a_nonzero_weight() {
    for raw in 0..=256 {
        let radius = Fixed::from_ratio(raw, 256);
        let wide = Fixed64::from_fixed(radius);
        let square = (wide * wide).max(Fixed64::from_ratio(1, 65536));
        let puff = Puff {
            radius,
            inverse: Fixed64::ONE / square,
            cutoff: square * i64::from(PUFF_DISTANCE_LIMIT),
            ..Puff::default()
        };
        for offset in -300..=300 {
            let y = Fixed::from_ratio(offset, 256);
            if !puff.density_row_may_contribute([y, y]) {
                assert!(puff.scaled_distance([Fixed::ZERO, y]) >= Fixed::ONE);
            }
        }
    }
}
#[test]
fn noise_and_warp_intervals_preserve_fixed_rounding_bounds() {
    for x in [-1000, -7, 0, 3, 1000] {
        for y in [-1000, -11, 0, 5, 1000] {
            let value = fbm([Fixed::from_int(x), Fixed::from_int(y)]);
            assert!(value >= Fixed::ZERO && value < Fixed::ONE);
        }
    }
    for direction in [[0, 0], [0, 1], [1, 0], [3, 4], [-3, 4], [1, -1]] {
        for radius in [Fixed::from_ratio(1, 256), Fixed::HALF, Fixed::from_int(3)] {
            let [dx, dy] = direction.map(Fixed::from_int);
            let prepared = Prepared::new(
                Flame {
                    points: [[Fixed::ZERO; 2], [dx, dy], [dx * 2, dy * 2]],
                    radius,
                    tip_scale: Fixed::ONE,
                    length: Fixed::from_int(8),
                    ..Flame::default()
                },
                Fixed::ZERO,
            );
            let bounds = prepared.warp_y_interval();
            for a in -4..=4 {
                for b in -4..=4 {
                    for loose in [
                        Fixed::ZERO,
                        Fixed::from_ratio(1, 256),
                        Fixed::HALF,
                        Fixed::ONE,
                    ] {
                        let dy = (prepared.across[1] * Fixed::from_ratio(a, 8)
                            + prepared.direction[1]
                                * Fixed::from_ratio(b, 8)
                                * Fixed::from_ratio(6, 10))
                            * radius
                            * loose
                            * Fixed::from_ratio(11, 10);
                        assert!(dy >= bounds[0] && dy <= bounds[1]);
                    }
                }
            }
        }
    }
}
#[test]
fn saturated_puff_distances_have_no_density_or_halo() {
    let limit = Fixed::from_int(PUFF_DISTANCE_LIMIT);
    assert_eq!((Fixed::ONE - limit).max(Fixed::ZERO), Fixed::ZERO);
    assert_eq!((-limit * Fixed::from_ratio(42, 100)).exp(), Fixed::ZERO);
    for radius in [Fixed::from_ratio(1, 256), Fixed::HALF, Fixed::from_int(4)] {
        let radius = Fixed64::from_fixed(radius);
        let square = (radius * radius).max(Fixed64::from_ratio(1, 65536));
        let puff = Puff {
            inverse: Fixed64::ONE / square,
            cutoff: square * i64::from(PUFF_DISTANCE_LIMIT),
            ..Puff::default()
        };
        for point in [[32, 0], [-32, 0], [0, 32], [0, -32]] {
            assert_eq!(puff.scaled_distance(point.map(Fixed::from_int)), limit);
        }
    }
}
#[test]
fn flame_shading_is_finite_emissive_and_visibility_gated() {
    let mut flame = Flame {
        points: [
            [Fixed::ZERO, Fixed::ZERO],
            [Fixed::ZERO, Fixed::from_int(5)],
            [Fixed::from_int(2), Fixed::from_int(20)],
        ],
        radius: Fixed::from_int(2),
        tip_scale: Fixed::ONE,
        power: Fixed::ONE,
        alpha: Fixed::ONE,
        visibility: [Fixed::ONE; 3],
        seed: Fixed::ZERO,
        length: Fixed::from_int(10),
    };
    let visible = Prepared::new(flame, Fixed::from_int(3));
    let mut lit = false;
    let mut emission = false;
    for y in -4..24 {
        for x in -8..8 {
            let c = visible.shade::<true>([Fixed::from_int(x), Fixed::from_int(y)]);
            assert!(
                c.iter()
                    .all(|v| *v >= Fixed::ZERO && *v <= Fixed::from_int(2))
            );
            lit |= c[3] > Fixed::from_ratio(1, 10);
            emission |= c[0] > c[3];
        }
    }
    assert!(lit && emission);
    flame.visibility = [Fixed::ZERO; 3];
    let hidden = Prepared::new(flame, Fixed::from_int(3));
    assert_eq!(
        hidden.shade::<true>([Fixed::from_ratio(0, 1), Fixed::from_ratio(3, 1)]),
        [Fixed::from_ratio(0, 1); 4]
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
        power: Fixed::ZERO,
        alpha: Fixed::ONE,
        visibility: [Fixed::ONE; 3],
        seed: Fixed::ZERO,
        length: Fixed::from_int(8),
    };
    let allocations = crate::test_support::tracked_allocations(|| {
        flames
            .prepare(&[flame], flight.time, Fixed::from_int(3), [480, 300])
            .unwrap()
    });
    assert_eq!(allocations, 0);
    assert!(flames.tiles[0].active);
    assert!(flames.tiles[0].size.iter().all(|n| usize::from(*n) <= TILE));
    assert!(flames.tiles[1..].iter().all(|tile| !tile.active));
}
#[test]
fn support_culling_preserves_quantized_shader_output() {
    for tip_scale in [
        Fixed::from_ratio(1, 10),
        Fixed::from_ratio(5, 10),
        Fixed::ONE,
        Fixed::from_int(3),
    ] {
        for length in [Fixed::ONE, Fixed::from_int(8), Fixed::from_int(25)] {
            let flame = Flame {
                points: [
                    [Fixed::ZERO, Fixed::ZERO],
                    [Fixed::ZERO, Fixed::from_int(5)],
                    [Fixed::from_int(2), Fixed::from_int(20)],
                ],
                radius: Fixed::from_int(2),
                tip_scale,
                power: Fixed::ONE,
                alpha: Fixed::ONE,
                visibility: [Fixed::ONE; 3],
                seed: Fixed::from_int(2),
                length,
            };
            let prepared = Prepared::new(flame, Fixed::from_int(12));
            for y in -16..40 {
                for x in -20..20 {
                    let position = [
                        Fixed::from_int(x) * Fixed::from_ratio(7, 10),
                        Fixed::from_int(y) * Fixed::from_ratio(7, 10),
                    ];
                    let row = prepared.row_candidates(position[1], prepared.warp_y_interval());
                    let optimized = prepared.shade_row::<true>(position[0], &row);
                    let reference = prepared.shade::<false>(position);
                    if optimized != [Fixed::ZERO; 4] {
                        let [left, right] = prepared
                            .row_support(position[1])
                            .expect("nonzero sample has row support");
                        assert!(
                            position[0] >= left.floor() - Fixed::ONE
                                && position[0] < right.ceil() + Fixed::ONE
                        );
                    }
                    assert_eq!(
                        optimized.map(|v| ((v
                            .clamp(Fixed::from_ratio(0, 1), Fixed::from_ratio(1, 1))
                            * Fixed::from_ratio(255, 1))
                        .round())
                        .trunc_to_int() as u8),
                        reference.map(|v| ((v
                            .clamp(Fixed::from_ratio(0, 1), Fixed::from_ratio(1, 1))
                            * Fixed::from_ratio(255, 1))
                        .round())
                        .trunc_to_int() as u8)
                    );
                }
            }
        }
    }
}

impl Prepared {
    fn shade<const CULL: bool>(&self, p: [Fixed; 2]) -> [Fixed; 4] {
        self.shade_row::<CULL>(
            p[0],
            &PuffRow {
                y: p[1],
                density_mask: (1 << PUFFS) - 1,
                halo_mask: (1 << PUFFS) - 1,
            },
        )
    }
}
