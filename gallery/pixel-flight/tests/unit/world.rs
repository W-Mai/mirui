use super::*;
#[test]
fn scene_and_simulation_sample_the_same_generated_planes() {
    for side in [64, SIDE] {
        let world = World::with_size(7, Fixed::from_ratio(33, 100), side, 32, 16);
        let field = world.field();
        assert!(field.ptr_eq(&world.field()));
        let cell_size = (SIDE / side) as i32;
        for row in 0..side {
            for col in 0..side {
                let index = row * side + col;
                let x = col as i32 * cell_size - SIDE as i32;
                let y = row as i32 * cell_size + SIDE as i32;
                assert_eq!(field.materials()[index], world.material(x, y));
                assert_eq!(field.levels()[index], world.height(x, y));
                assert_eq!(
                    field.height_at(Fixed::from_int(x), Fixed::from_int(-y)),
                    Some(Fixed::from_int(i32::from(world.height(x, y))))
                );
                let [r, g, b] = TOP[usize::from(world.material(x, y))];
                assert_eq!(field.color(field.materials()[index]), Color::rgb(r, g, b));
            }
        }
        for x in [-1025, -17, -16, -1, 0, 1, 15, 16, 17, 1023, 1024] {
            for y in [-1025, -17, -16, -1, 0, 1, 15, 16, 17, 1023, 1024] {
                for fraction in [-Fixed::HALF, Fixed::ZERO, Fixed::HALF] {
                    let x = Fixed::from_int(x) + fraction;
                    let y = Fixed::from_int(y) - fraction;
                    let address = field.address_at(x, -y).unwrap();
                    let (col, row) = (x.trunc_to_int(), y.trunc_to_int());
                    assert_eq!(field.materials()[address], world.material(col, row));
                    assert_eq!(
                        field.height_at(x, -y),
                        Some(Fixed::from_int(i32::from(world.height(col, row))))
                    );
                }
            }
        }
    }
}

#[test]
fn seeded_fields_are_bounded_wrapped_and_deterministic() {
    let a = World::new(7, Fixed::from_ratio(33, 100));
    let b = World::new(7, Fixed::from_ratio(33, 100));
    assert_eq!(
        a.ground.levels().len() + a.ground.materials().len() + a.clouds.len() + a.noise.len(),
        2_424_832
    );
    assert_eq!(a.ground.levels(), b.ground.levels());
    assert_eq!(a.ground.materials(), b.ground.materials());
    assert_eq!(a.clouds, b.clouds);
    assert!(
        a.ground
            .levels()
            .iter()
            .all(|h| usize::from(*h) < MAX_HEIGHT)
    );
    assert!(
        a.ground
            .materials()
            .iter()
            .all(|m| usize::from(*m) < TOP.len())
    );
    for (x, y) in [(-1, -1), (0, 0), (123, 456), (-99_999, 123_456)] {
        assert_eq!(a.height(x, y), a.height(x + SIDE as i32, y - SIDE as i32));
        assert_eq!(
            a.cloud(x, y),
            a.cloud(x - CLOUD_SIDE as i32, y + CLOUD_SIDE as i32)
        );
    }
    let land = Fixed::from_int((a.ground.materials().iter().filter(|m| **m >= 4).count()) as i32)
        / Fixed::from_int((SIDE * SIDE) as i32);
    assert!(
        (land - Fixed::from_ratio(33, 100)).abs() < Fixed::from_ratio(1, 50),
        "land fraction {land}"
    );
    assert!(a.ground.materials().iter().any(|m| (9..=11).contains(m)));
    assert!(a.clouds.iter().any(|c| *c != 0));
    assert!(a.material((a.spawn[0]).trunc_to_int(), (a.spawn[1]).trunc_to_int()) <= 2);
}
