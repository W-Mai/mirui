use super::*;

#[test]
fn large_viewports_reserve_the_full_rotated_diagonal() {
    assert_eq!(
        super::super::sheet_side(6000, 8000, Fixed::from_int(4000)),
        10004
    );
    assert_eq!(super::super::sheet_side(60000, 65000, Fixed::ZERO), 143184);
}

#[test]
fn roll_bounds_cover_every_output_corner_in_all_quadrants() {
    for (width, height) in [
        (8, 8),
        (32, 32),
        (128, 128),
        (128, 64),
        (64, 128),
        (127, 113),
    ] {
        for degrees in (-360..360).step_by(5) {
            let angle = Fixed::from_int(degrees) * Fixed::PI / 180;
            let pivot = (Fixed::from_int(height as i32) * Fixed::from_ratio(34, 100)).round();
            let mapping = RollMapping::new(
                [width, height],
                pivot,
                Some((Fixed::sin_rad(angle), Fixed::cos_rad(angle))),
            );
            for y in (0..height).step_by(TILE_SIDE) {
                for x in (0..width).step_by(TILE_SIDE) {
                    let right = (x + TILE_SIDE).min(width) - 1;
                    let bottom = (y + TILE_SIDE).min(height) - 1;
                    let corners = [[x, y], [right, y], [x, bottom], [right, bottom]]
                        .map(|[x, y]| mapping.source_at(x as i32, y as i32));
                    let low = [
                        corners.iter().map(|p| p[0]).min().unwrap() - 2,
                        corners.iter().map(|p| p[1]).min().unwrap() - 2,
                    ];
                    for point in corners {
                        assert!(
                            point[0] >= mapping.bounds[0]
                                && point[1] >= mapping.bounds[1]
                                && point[0] < mapping.bounds[2]
                                && point[1] < mapping.bounds[3],
                            "{width}x{height} at {degrees}: {point:?} outside {:?}",
                            mapping.bounds
                        );
                        assert!(
                            point[0] > low[0]
                                && point[1] > low[1]
                                && point[0] < low[0] + SOURCE_SIDE as i32 - 1
                                && point[1] < low[1] + SOURCE_SIDE as i32 - 1
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn rows_stop_at_the_first_failed_scene_tile() {
    let mut output = Frame::new(9, 11).unwrap();
    let mut workspace = Workspace::new(9, 4).unwrap();
    let mut calls = 0;
    let result = workspace.render_rows(&mut output, |tile| {
        calls += 1;
        if calls == 2 {
            return Err(RenderError::InvalidGeometry);
        }
        tile.color.bytes.fill(42);
        Ok(())
    });
    assert!(matches!(result, Err(RenderError::InvalidGeometry)));
    assert_eq!(calls, 2);
    let pixels = output.texture.read_pixels().unwrap();
    assert!(pixels.bytes()[..4 * 9 * 4].iter().all(|v| *v == 42));
    assert!(pixels.bytes()[4 * 9 * 4..].iter().all(|v| *v == 0));
}

#[test]
fn opaque_overlay_skips_scene_tiles_and_effect_queries_share_a_depth_tile() {
    struct Solid;
    impl FrameOverlay for Solid {
        fn covers(&self, _: [i32; 4], _: &mut [u8]) -> bool {
            true
        }
        fn paint(&self, canvas: &mut Canvas<'_>) {
            let [left, top, right, bottom] = canvas.clip;
            for y in top..bottom {
                for x in left..right {
                    canvas.point(x, y, 0xff00_00ff);
                }
            }
        }
    }
    struct Sample(Option<[i32; 2]>);
    impl DepthQuery for Sample {
        fn next_point(&self) -> Option<[i32; 2]> {
            self.0
        }
        fn resolve(&mut self, tile: &SceneCanvas<'_, '_>) {
            if let Some([x, y]) = self.0
                && tile.sample_depth(x, y).is_some()
            {
                self.0 = None;
            }
        }
    }
    let mapping = RollMapping::new([65, 42], Fixed::ZERO, None);
    let mut workspace = CameraWorkspace::new([65, 42], Fixed::ZERO, true).unwrap();
    for format in [
        ColorFormat::RGBA8888,
        ColorFormat::RGB565,
        ColorFormat::RGB565Swapped,
    ] {
        let mut output = Frame::with_format(65, 42, format).unwrap();
        workspace
            .render(
                &mut output,
                &mapping,
                Some(&Solid),
                |_| panic!("covered scene drawn"),
                |_| Ok(()),
            )
            .unwrap();
        let packed = format
            .pack(&mirui::types::Color::rgb(255, 0, 0))
            .to_le_bytes();
        let bpp = format.bytes_per_pixel();
        let pixels = output.texture.read_pixels().unwrap();
        assert!(
            pixels
                .bytes()
                .chunks_exact(bpp)
                .all(|p| p == &packed[..bpp])
        );
    }
    let mut queries = [Sample(Some([4, 5])), Sample(Some([40, 39]))];
    let mut fills = 0;
    workspace
        .finish_depth_queries(&mapping, &mut queries, |tile, _| {
            tile.clear_depth();
            fills += 1;
            Ok(())
        })
        .unwrap();
    assert_eq!(fills, 1);
    assert!(queries.iter().all(|sample| sample.next_point().is_none()));
}
