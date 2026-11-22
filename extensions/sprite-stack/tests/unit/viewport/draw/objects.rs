use super::*;
use crate::{
    Camera, Pose, TerrainGrid,
    raster::{Frame, Workspace},
};

#[test]
fn many_tilted_repetitions_are_sampled_only_along_visible_rays() {
    use core::sync::atomic::{AtomicUsize, Ordering};
    static SAMPLES: AtomicUsize = AtomicUsize::new(0);
    let field = HeightField::from_planes(
        TerrainGrid::repeating([1, 1], Fixed::ONE).unwrap(),
        &[0][..],
        &[99][..],
    )
    .unwrap()
    .with_coordinate_map(|p| {
        SAMPLES.fetch_add(1, Ordering::Relaxed);
        p
    });
    let size = [32, 21];
    let color = u32::from_le_bytes([99, 99, 99, 255]);
    for perspective in [Fixed64::ZERO, Fixed64::from_ratio(1, 100)] {
        let mut camera = ViewProjection::from_camera(
            Camera {
                range: Fixed::from_int(1800),
                ..Camera::default()
            },
            size,
        )
        .unwrap();
        camera.projection.perspective = perspective;
        let object = ObjectProjection::new(
            &camera,
            Pose {
                pitch: Fixed::from_int(17),
                yaw: Fixed::from_int(24),
                roll: Fixed::from_int(-9),
                ..Pose::default()
            },
        );
        let mut frame = Frame::new(size[0], size[1]).unwrap();
        let mut workspace = Workspace::new(size[0], size[1]).unwrap();
        let mut reference = Frame::new(size[0], size[1]).unwrap();
        let mut expected_depth = alloc::vec::Vec::new();
        workspace.with_canvas(&mut reference, |canvas| {
            canvas.clear_depth();
            top(
                &object,
                canvas,
                [(-32).into(), 0.into(), (-32).into()],
                64.into(),
                64.into(),
                color,
            );
            expected_depth.extend_from_slice(canvas.depth);
        });
        SAMPLES.store(0, Ordering::Relaxed);
        workspace.with_canvas(&mut frame, |canvas| {
            canvas.clear_depth();
            draw_field(canvas, &object, &field);
            for (i, expected) in expected_depth.iter().enumerate() {
                if *expected != Fixed::MAX {
                    assert_eq!(canvas.get(i), color);
                    assert!(
                        (canvas.depth[i] - *expected).abs() <= Fixed::ONE,
                        "index {i}"
                    );
                }
            }
        });
        let count = SAMPLES.load(Ordering::Relaxed);
        assert!(count > 0 && count <= size[0] * size[1], "{count}");
        let mut tiled = Frame::new(size[0], size[1]).unwrap();
        let mut bands = Workspace::new(size[0], 4).unwrap();
        bands
            .render_rows(&mut tiled, |canvas| -> Result<(), RenderError> {
                canvas.clear_depth();
                draw_field(canvas, &object, &field);
                Ok(())
            })
            .unwrap();
        assert_eq!(
            tiled.texture.read_pixels().unwrap().bytes(),
            frame.texture.read_pixels().unwrap().bytes()
        );
    }
}

#[test]
fn tilted_field_range_is_centered_in_its_own_grid() {
    let field = HeightField::from_planes(
        TerrainGrid::finite([16, 128], Fixed::ONE).unwrap(),
        alloc::vec![0; 16 * 128],
        alloc::vec![99; 16 * 128],
    )
    .unwrap();
    let camera = ViewProjection::from_camera(
        Camera {
            center: [8.into(), 100.into(), 0.into()],
            pitch: 0.into(),
            range: 4.into(),
            ..Camera::default()
        },
        [8, 8],
    )
    .unwrap();
    let object = ObjectProjection::new(
        &camera,
        Pose {
            pitch: 90.into(),
            ..Pose::default()
        },
    );
    let mut frame = Frame::new(8, 8).unwrap();
    let mut work = Workspace::new(8, 8).unwrap();
    work.with_canvas(&mut frame, |tile| {
        tile.clear_depth();
        draw_field(tile, &object, &field);
        assert_eq!(
            tile.get(tile.color.index(4, 4)),
            u32::from_le_bytes([99, 99, 99, 255])
        );
    });
}

#[test]
fn field_rays_intersect_top_and_wall_in_both_directions_and_respect_depth() {
    let f = HeightField::from_planes(
        TerrainGrid::repeating([2, 1], Fixed::ONE).unwrap(),
        &[0, 3][..],
        &[10, 20][..],
    )
    .unwrap();
    let z = Fixed64::ZERO;
    let one = Fixed64::ONE;
    let half = one / 2;
    let bounds = [[-one * 3, one * 3], [z, one * 3], [-one, one]];
    let hit = |origin, dir, end| field_hit(&f, one, bounds, origin, dir, [z, end]);
    assert_eq!(
        hit([half, one, half], [one, z, z], one * 4),
        Some((Fixed::HALF, 1))
    );
    assert_eq!(
        hit([half, one, half], [-one, z, z], one * 4),
        Some((Fixed::HALF, 1))
    );
    assert_eq!(hit([half, one, half], [one, z, z], one / 4), None);
    assert_eq!(hit([one + half, one, half], [z, one, z], one / 4), None);
    assert_eq!(
        hit([one + half, -one, half], [z, one, z], one * 8),
        Some((4.into(), 1))
    );
    assert_eq!(
        hit([half, one * 5, half], [z, -one, z], one * 8),
        Some((5.into(), 0))
    );
}

#[test]
fn band_culling_preserves_rasterized_cells_at_fractional_and_edge_on_views() {
    let size = [49, 71];
    let grid = LayerGrid {
        size: [9, 5],
        pivot: [Fixed::from_ratio(7, 2), Fixed::from_ratio(5, 4)],
        layer_pivot: Fixed::ZERO,
    };
    let elevation = Fixed::from_ratio(9, 4);
    let mut culled = 0;
    for pitch in [-34, 0, 29, 90] {
        let camera = Camera {
            yaw: Fixed::from_int(33),
            pitch: Fixed::from_int(pitch),
            scale: Fixed::from_ratio(17, 4),
            ..Camera::default()
        };
        let view = ViewProjection::from_camera(
            Camera {
                center: [Fixed::HALF; 3],
                ..camera
            },
            size,
        )
        .unwrap();
        for yaw in [0, 90, 180, 270, 23] {
            let pose = Pose {
                yaw: Fixed::from_int(yaw),
                ..Pose::default()
            };
            let object = ObjectProjection::new(&view, pose);
            let slice = SliceProjection::new(&object);
            let mut frame = Frame::new(size[0], size[1]).unwrap();
            let mut work = Workspace::new(size[0], size[1]).unwrap();
            work.with_rows(&mut frame, 0, size[1], |tile| {
                tile.clear_depth();
                for y in (0..grid.size[1]).step_by(2) {
                    for x in (0..grid.size[0]).step_by(2) {
                        grid.cell(&slice, tile, [x, y], elevation, 2, u32::MAX);
                    }
                }
                for y in 0..size[1] {
                    if !grid.intersects_band(
                        &slice,
                        [elevation; 2],
                        [0, y as i32, size[0] as i32, y as i32 + 1],
                    ) {
                        culled += 1;
                        assert!((0..size[0]).all(|x| tile.get(y * size[0] + x) == 0));
                    }
                }
            });
        }
    }
    assert!(culled > 0);
}
