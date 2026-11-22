use super::*;
use crate::{
    TerrainCell, TerrainGrid,
    raster::{Frame, Workspace},
};
use mirui::types::Color;

#[test]
fn elevation_bounds_include_fractional_and_negative_samples() {
    let heights = [Fixed::from_ratio(-17, 2), Fixed::from_int(48)];
    for origin in [-256, -1, 0, 37, 256] {
        for scale in [-512, -37, -1, 0, 1, 37, 512] {
            let row = HeightProjection {
                origin: Fixed::from_int(origin),
                scale: Fixed::from_ratio(scale, 256),
            };
            let minimum = row.minimum_y(heights);
            for h in -17..=96 {
                assert!(row.at(Fixed::from_ratio(h, 2)) >= minimum);
            }
        }
    }
}

#[test]
fn additional_terrain_preserves_nearer_and_equal_depths_from_other_passes() {
    let grid = TerrainGrid::repeating([4, 1], Fixed::from_int(2)).unwrap();
    let make_field = |levels: [u8; 4], palette| {
        HeightField::generate(grid, palette, |[x, _]| TerrainCell {
            level: levels[x as usize],
            material: 0,
        })
        .unwrap()
    };
    const RED: &[Color] = &[Color::rgb(224, 32, 16)];
    const GREEN: &[Color] = &[Color::rgb(24, 192, 48)];
    let first = make_field([1, 2, 3, 0], RED);
    let second = make_field([3, 2, 0, 3], GREEN);
    let view = TerrainView {
        projection: Projection {
            perspective: Fixed64::ZERO,
            slope: Fixed::ONE,
            scale: Fixed::from_int(2),
            center: [Fixed::from_int(16), Fixed::from_int(8)],
        },
        origin: [Fixed::ZERO; 2],
        altitude: Fixed::from_int(4),
        yaw: (Fixed::ZERO, Fixed::ONE),
        heights: [Fixed::ZERO, Fixed::from_int(3)],
        depth: [Fixed::from_int(-32), Fixed::from_int(40)],
    };
    let render = |mut base: &HeightField, extra: Option<&HeightField>| {
        let mut image = Frame::new(32, 16).unwrap();
        let mut work = Workspace::new(32, 16).unwrap();
        work.with_canvas(&mut image, |tile| {
            tile.clear_depth();
            view.draw(tile, &mut base);
            if let Some(mut layer) = extra {
                view.draw_layer(tile, &mut layer);
            }
        });
        (image, work)
    };
    let (a, za) = render(&first, None);
    let (b, zb) = render(&second, None);
    let (composite, z) = render(&first, Some(&second));
    let (a, b, c) = (
        a.texture.read_pixels().unwrap(),
        b.texture.read_pixels().unwrap(),
        composite.texture.read_pixels().unwrap(),
    );
    let mut covered = [false; 3];
    for (i, out) in c.bytes().chunks_exact(4).enumerate() {
        let incoming = zb.depth[i];
        let existing = za.depth[i];
        if incoming != Fixed::MAX && existing != Fixed::MAX {
            let relation = match incoming.cmp(&existing) {
                core::cmp::Ordering::Less => 0,
                core::cmp::Ordering::Equal => 1,
                core::cmp::Ordering::Greater => 2,
            };
            covered[relation] = true;
        }
        let expected = if incoming < existing {
            b.bytes()
        } else {
            a.bytes()
        };
        assert_eq!(out, &expected[i * 4..i * 4 + 4]);
        assert_eq!(z.depth[i], incoming.min(existing));
    }
    assert_eq!(covered, [true; 3]);
}

#[test]
fn height_field_uses_the_same_surface_colors_and_depth_in_separate_tiles() {
    const PALETTE: &[Color] = &[Color::rgb(53, 35, 23), Color::rgb(9, 87, 41)];
    let grid = TerrainGrid::repeating([4, 2], Fixed::from_int(2))
        .unwrap()
        .with_elevation(-Fixed::ONE, Fixed::HALF)
        .unwrap();
    let field = HeightField::generate(grid, PALETTE, |[x, z]| TerrainCell {
        level: (x + z) as u8,
        material: (x % 2) as u8,
    })
    .unwrap();
    let mut surface = &field;
    for perspective in [Fixed64::ZERO, Fixed64::from_ratio(1, 128)] {
        let view = TerrainView {
            projection: Projection {
                perspective,
                slope: Fixed::ONE,
                scale: Fixed::from_int(2),
                center: [Fixed::from_int(16), Fixed::from_int(8)],
            },
            origin: [Fixed::HALF, -Fixed::HALF],
            altitude: Fixed::from_int(4),
            yaw: (
                Fixed::sin_deg(Fixed::from_int(30)),
                Fixed::cos_deg(Fixed::from_int(30)),
            ),
            heights: [-Fixed::ONE, Fixed::ONE],
            depth: [Fixed::from_int(-32), Fixed::from_int(48)],
        };
        let mut full = Frame::new(32, 16).unwrap();
        let mut full_work = Workspace::new(32, 16).unwrap();
        full_work.with_canvas(&mut full, |tile| {
            tile.clear_depth();
            view.draw(tile, &mut surface);
        });
        let mut tiled = Frame::new(32, 16).unwrap();
        let mut work = Workspace::new(32, 4).unwrap();
        for y in (0..16).step_by(4) {
            work.with_rows(&mut tiled, y, 4, |tile| {
                tile.clear_depth();
                view.draw(tile, &mut surface);
                assert_eq!(tile.depth, &full_work.depth[y * 32..(y + 4) * 32]);
            });
        }
        let full = full.texture.read_pixels().unwrap();
        for &color in PALETTE {
            assert!(
                full.bytes()
                    .chunks_exact(4)
                    .any(|c| c == [color.r, color.g, color.b, color.a])
            );
        }
        assert_eq!(full.bytes(), tiled.texture.read_pixels().unwrap().bytes());
    }
}
