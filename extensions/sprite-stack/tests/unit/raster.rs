use super::*;

#[test]
fn tiled_samples_share_depth_and_reject_outside_writes() {
    let mut frame = Frame::new(4, 4).unwrap();
    let mut workspace = Workspace::new(4, 4).unwrap();
    let bytes = workspace.bytes();
    // A tall strip uses the same packed storage as a square tile.
    workspace.with_extent(&mut frame, [2, 8], [8, 8], [3, 0], [1, 1, 7, 7], |tile| {
        tile.clear_depth();
        assert_eq!(tile.sample_depth(1, 1), None);
        assert_eq!(tile.sample_depth(0, 0), Some(Fixed::MAX));
        let near = Fixed::ONE;
        let far = Fixed::from_int(2);
        assert!(tile.write_visible(3, 2, far, 0xff010203));
        assert!(tile.write_visible(3, 2, near, 0xff040506));
        assert!(!tile.write_visible(3, 2, far, 0xff010203));
        assert!(!tile.write_visible(3, 2, Fixed::ZERO, 0));
        tile.fill_depth(3, 1, 7, 0xff070809, far);
        assert_eq!(tile.sample_depth(3, 2), Some(near));
        assert_eq!(tile.get(tile.color.index(3, 2)), 0xff040506);
        assert!(!tile.write_visible(2, 2, near, 0xffffffff));
        tile.clear_depth();
        assert_eq!(tile.sample_depth(3, 2), Some(Fixed::MAX));
    });
    assert_eq!(workspace.bytes(), bytes);
}

#[test]
fn checker_spans_keep_global_parity_at_clipped_tile_edges() {
    let mut frame = Frame::new(3, 5).unwrap();
    let mut workspace = Workspace::new(3, 5).unwrap();
    let end = i32::MAX;
    workspace.with_region(
        &mut frame,
        [10, end as usize],
        [2, end - 5],
        [1, end - 4, 4, end],
        |tile| {
            let old = 0xff00ff00;
            let new = 0xff0000ff;
            assert!(tile.write_visible(2, end - 2, Fixed::ONE, old));
            for x in [usize::MAX, 1, 2, 3, 4] {
                tile.fill_span(x, end - 7, end, new, Fixed::from_int(2), true);
            }
            for x in 2..5 {
                for y in end - 5..end {
                    let expected = if (x, y) == (2, end - 2) {
                        old
                    } else if tile.color.contains(x, y) && (x ^ y) & 1 != 0 {
                        new
                    } else {
                        0
                    };
                    assert_eq!(tile.get(tile.color.index(x, y)), expected);
                }
            }
        },
    );
}

#[test]
fn projection_shares_orthographic_and_perspective_coordinates() {
    let mut p = Projection {
        perspective: Fixed64::ZERO,
        slope: Fixed::ZERO,
        scale: Fixed::from_int(2),
        center: [Fixed::from_int(4); 2],
    };
    let v = Fixed::ONE;
    assert_eq!(
        p.project_camera(v, v, v),
        Some([
            Fixed::from_int(6),
            Fixed::from_int(2),
            Fixed::from_int(2),
            v
        ])
    );
    p.perspective = Fixed64::ONE;
    assert_eq!(
        p.project_camera(v, v, v),
        Some([Fixed::from_int(5), Fixed::from_int(3), v, v])
    );
    assert!(p.project_camera(v, v, -v).is_none());
}
