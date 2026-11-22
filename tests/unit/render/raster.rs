use alloc::vec;

use super::*;

fn pt(x: i32, y: i32) -> Point {
    Point {
        x: Fixed::from_int(x),
        y: Fixed::from_int(y),
    }
}

fn flatten_path(p: &Path) -> Vec<LineSeg> {
    let mut out = Vec::new();
    flatten_into(&p.cmds, None, &mut out);
    out
}

fn flatten_subpaths_path(p: &Path) -> (Vec<LineSeg>, Vec<SubPath>) {
    let mut segments = Vec::new();
    let mut out = Vec::new();
    flatten_subpaths_into(&p.cmds, None, &mut segments, &mut out);
    (segments, out)
}

fn offset_polygon_path(p: &Path, width: Fixed) -> Path {
    offset_polygon_path_with_cap(p, width, LineCap::Butt)
}

fn offset_polygon_path_with_cap(p: &Path, width: Fixed, cap: LineCap) -> Path {
    offset_polygon_path_with_style(p, width, cap, LineJoin::Miter)
}

fn offset_polygon_path_with_style(p: &Path, width: Fixed, cap: LineCap, join: LineJoin) -> Path {
    let mut out = Path::new();
    let mut flattened = Vec::new();
    let mut scratch = Vec::new();
    let mut semantic_joins = Vec::new();
    let mut normals = Vec::new();
    let mut rail = Vec::new();
    let mut left_rail = Vec::new();
    let mut arc = Vec::new();
    let mut dashed = Vec::new();
    let mut dash_scratch = Vec::new();
    let mut dashed_semantic_joins = Vec::new();
    offset_polygon_into(
        &p.cmds,
        None,
        width,
        cap,
        join,
        Fixed::from_int(4),
        None,
        &mut out,
        &mut flattened,
        &mut scratch,
        &mut semantic_joins,
        &mut normals,
        &mut rail,
        &mut left_rail,
        &mut arc,
        &mut dashed,
        &mut dash_scratch,
        &mut dashed_semantic_joins,
    );
    out
}

#[test]
fn flatten_empty_path() {
    assert!(flatten_path(&Path::new()).is_empty());
}

#[test]
fn flatten_single_line() {
    let mut p = Path::new();
    p.move_to(pt(0, 0)).line_to(pt(10, 0));
    let segs = flatten_path(&p);
    assert_eq!(segs.len(), 1);
    assert_eq!(segs[0].p1, pt(0, 0));
    assert_eq!(segs[0].p2, pt(10, 0));
}

#[test]
fn flatten_closed_triangle_emits_closing_edge() {
    let mut p = Path::new();
    p.move_to(pt(0, 0))
        .line_to(pt(10, 0))
        .line_to(pt(5, 10))
        .close();
    let segs = flatten_path(&p);
    // 3 explicit lines + 1 implicit close back to (0,0)
    assert_eq!(segs.len(), 3);
    assert_eq!(segs[2].p2, pt(0, 0));
}

#[test]
fn flatten_close_is_noop_when_current_equals_start() {
    let mut p = Path::new();
    p.move_to(pt(0, 0))
        .line_to(pt(10, 0))
        .line_to(pt(0, 0))
        .close();
    let segs = flatten_path(&p);
    // close() sees current == subpath_start, so it must not add a redundant edge
    assert_eq!(segs.len(), 2);
}

#[test]
fn flatten_quad_emits_n_segments_connected() {
    let mut p = Path::new();
    p.move_to(pt(0, 0)).quad_to(pt(10, 10), pt(20, 0));
    let segs = flatten_path(&p);
    assert_eq!(segs.len(), QUAD_STEPS as usize);
    // Chain: seg[i].p2 == seg[i+1].p1
    for i in 0..segs.len() - 1 {
        assert_eq!(segs[i].p2, segs[i + 1].p1);
    }
    assert_eq!(segs.last().unwrap().p2, pt(20, 0));
}

#[test]
fn flatten_cubic_emits_n_segments_connected() {
    let mut p = Path::new();
    p.move_to(pt(0, 0))
        .cubic_to(pt(0, 10), pt(10, 10), pt(10, 0));
    let segs = flatten_path(&p);
    assert_eq!(segs.len(), CUBIC_STEPS as usize);
    for i in 0..segs.len() - 1 {
        assert_eq!(segs[i].p2, segs[i + 1].p1);
    }
    assert_eq!(segs.last().unwrap().p2, pt(10, 0));
}

#[test]
fn flatten_multi_subpath() {
    // Two independent subpaths: a line then a separate triangle.
    let mut p = Path::new();
    p.move_to(pt(0, 0)).line_to(pt(10, 0));
    p.move_to(pt(50, 50)).line_to(pt(60, 50)).close();
    let segs = flatten_path(&p);
    // Subpath 1: 1 line. Subpath 2: 1 explicit + 1 close = 2.
    assert_eq!(segs.len(), 3);
    // Second subpath closes back to (50,50), not (0,0)
    assert_eq!(segs.last().unwrap().p2, pt(50, 50));
}

fn square_segs() -> Vec<LineSeg> {
    // Unit square [0, 10] × [0, 10] — 4 edges CCW.
    vec![
        LineSeg {
            p1: pt(0, 0),
            p2: pt(10, 0),
        },
        LineSeg {
            p1: pt(10, 0),
            p2: pt(10, 10),
        },
        LineSeg {
            p1: pt(10, 10),
            p2: pt(0, 10),
        },
        LineSeg {
            p1: pt(0, 10),
            p2: pt(0, 0),
        },
    ]
}

#[test]
fn point_in_square_interior_detected() {
    assert!(point_in_segments(pt(5, 5), &square_segs()));
}

#[test]
fn point_outside_square_rejected() {
    assert!(!point_in_segments(pt(-1, 5), &square_segs()));
    assert!(!point_in_segments(pt(11, 5), &square_segs()));
    assert!(!point_in_segments(pt(5, -1), &square_segs()));
    assert!(!point_in_segments(pt(5, 11), &square_segs()));
}

#[test]
fn point_on_horizontal_edge_half_open() {
    // Half-open [y_lo, y_hi) — bottom edge is inclusive, top edge is exclusive.
    assert!(point_in_segments(pt(5, 0), &square_segs()));
    assert!(!point_in_segments(pt(5, 10), &square_segs()));
}

fn big_cap() -> Fixed {
    // Large enough to not affect any test case, small enough that
    // dx_box/dy_box comparisons stay well within i32 Fixed range.
    Fixed::from_int(1000)
}

#[test]
fn dist_on_segment_is_zero() {
    let segs = square_segs();
    let d = min_dist_to_segments_capped(pt(5, 0), &segs, big_cap());
    assert_eq!(d, Fixed::ZERO);
}

#[test]
fn dist_center_to_square_is_half_width() {
    let segs = square_segs();
    let d = min_dist_to_segments_capped(pt(5, 5), &segs, big_cap()).to_f32();
    assert!((d - 5.0).abs() < 0.01, "d = {d}");
}

#[test]
fn dist_beyond_endpoint_uses_endpoint() {
    let segs = vec![LineSeg {
        p1: pt(0, 0),
        p2: pt(10, 0),
    }];
    let d = min_dist_to_segments_capped(pt(15, 0), &segs, big_cap()).to_f32();
    assert!((d - 5.0).abs() < 0.01);
}

#[test]
fn dist_to_degenerate_segment() {
    let segs = vec![LineSeg {
        p1: pt(3, 4),
        p2: pt(3, 4),
    }];
    let d = min_dist_to_segments_capped(pt(0, 0), &segs, big_cap()).to_f32();
    assert!((d - 5.0).abs() < 0.01);
}

#[test]
fn dist_capped_returns_cap_when_far() {
    let segs = vec![LineSeg {
        p1: pt(100, 100),
        p2: pt(110, 100),
    }];
    let cap = Fixed::from_int(5);
    let d = min_dist_to_segments_capped(pt(0, 0), &segs, cap);
    assert_eq!(d, cap);
}

#[test]
fn flatten_subpaths_marks_closed_and_open() {
    // subpath A ends with Close (closed), subpath B is a dangling LineTo (open).
    let mut p = Path::new();
    p.move_to(pt(0, 0))
        .line_to(pt(10, 0))
        .line_to(pt(0, 10))
        .close();
    p.move_to(pt(50, 50)).line_to(pt(60, 50));
    let (_, subs) = flatten_subpaths_path(&p);
    assert_eq!(subs.len(), 2);
    assert!(subs[0].closed);
    assert!(!subs[1].closed);
}

#[test]
fn shared_stroke_scratch_scales_dash_lengths_with_output_pixels() {
    let mut path = Path::new();
    path.move_to(pt(0, 0)).line_to(pt(40, 0));
    let mut scratch = StrokeScratch::new();
    let dash = [Fixed::from_int(5), Fixed::from_int(5)];
    let spec = |dash_scale| StrokeSpec {
        width: Fixed::from_int(2),
        cap: LineCap::Butt,
        join: LineJoin::Miter,
        miter_limit: Fixed::from_int(4),
        dash: &dash,
        dash_scale,
    };
    let contours = |path: &Path| {
        path.cmds
            .iter()
            .filter(|cmd| matches!(cmd, PathCmd::MoveTo(_)))
            .count()
    };
    assert_eq!(
        contours(scratch.outline(path.commands(), None, spec(Fixed::ONE))),
        4
    );
    assert_eq!(
        contours(scratch.outline(path.commands(), None, spec(Fixed::from_int(2)))),
        2
    );
}

#[test]
fn offset_polygon_rectangle_closed_produces_two_rings() {
    // 10×10 rectangle stroked with width=2. The outline path should have
    // exactly 2 subpaths: outer ring + inner ring.
    let path = Path::rect(
        Fixed::from_int(0),
        Fixed::from_int(0),
        Fixed::from_int(10),
        Fixed::from_int(10),
    );
    let outline = offset_polygon_path(&path, Fixed::from_int(2));
    let closes = outline
        .cmds
        .iter()
        .filter(|c| matches!(c, PathCmd::Close))
        .count();
    assert_eq!(closes, 2);
}

#[test]
fn offset_polygon_open_line_makes_one_ribbon() {
    // A single open LineTo should yield a single closed ribbon (butt caps).
    let mut path = Path::new();
    path.move_to(pt(0, 0)).line_to(pt(10, 0));
    let outline = offset_polygon_path(&path, Fixed::from_int(2));
    let closes = outline
        .cmds
        .iter()
        .filter(|c| matches!(c, PathCmd::Close))
        .count();
    assert_eq!(closes, 1);
}

#[test]
fn open_butt_stroke_connects_matching_rail_ends() {
    let mut path = Path::new();
    path.move_to(pt(0, 0)).line_to(pt(10, 0));
    let outline = offset_polygon_path(&path, Fixed::from_int(2));
    assert_eq!(
        &*outline.cmds,
        &[
            PathCmd::MoveTo(pt(0, 1)),
            PathCmd::LineTo(pt(10, 1)),
            PathCmd::LineTo(pt(10, -1)),
            PathCmd::LineTo(pt(0, -1)),
            PathCmd::Close,
        ]
    );
    let edges = flatten_path(&outline);
    for x in [1, 5, 9] {
        assert!(point_in_segments(pt(x, 0), &edges));
    }
    assert!(!point_in_segments(pt(5, 2), &edges));
}

#[test]
fn open_curve_stroke_has_no_long_cross_rail_edge() {
    let mut path = Path::new();
    path.move_to(pt(0, 0)).quad_to(pt(12, 16), pt(24, 0));
    let outline = offset_polygon_path(&path, Fixed::from_int(4));
    let edges = flatten_path(&outline);
    assert!(edges.iter().all(|edge| {
        let dx = (edge.p2.x - edge.p1.x).to_f32();
        let dy = (edge.p2.y - edge.p1.y).to_f32();
        dx * dx + dy * dy <= 64.0
    }));
}

#[test]
fn curve_flattening_does_not_apply_round_join_to_synthetic_vertices() {
    let mut path = Path::new();
    path.move_to(pt(0, 0))
        .cubic_to(pt(8, 12), pt(16, 12), pt(24, 0));
    let round =
        offset_polygon_path_with_style(&path, Fixed::from_int(4), LineCap::Butt, LineJoin::Round);
    let miter =
        offset_polygon_path_with_style(&path, Fixed::from_int(4), LineCap::Butt, LineJoin::Miter);
    assert_eq!(round.cmds, miter.cmds);
}

#[test]
fn round_join_is_retained_at_semantic_line_corner() {
    let mut path = Path::new();
    path.move_to(pt(0, 0))
        .line_to(pt(10, 0))
        .line_to(pt(10, 10));
    let round =
        offset_polygon_path_with_style(&path, Fixed::from_int(4), LineCap::Butt, LineJoin::Round);
    let miter =
        offset_polygon_path_with_style(&path, Fixed::from_int(4), LineCap::Butt, LineJoin::Miter);
    assert_ne!(round.cmds, miter.cmds);
}

#[test]
fn open_square_and_round_caps_extend_outward() {
    let mut path = Path::new();
    path.move_to(pt(0, 0)).line_to(pt(10, 0));
    for cap in [LineCap::Square, LineCap::Round] {
        let outline = offset_polygon_path_with_cap(&path, Fixed::from_int(4), cap);
        let xs = outline.cmds.iter().filter_map(|command| match command {
            PathCmd::MoveTo(point) | PathCmd::LineTo(point) => Some(point.x),
            _ => None,
        });
        assert!(xs.clone().min().unwrap() <= Fixed::from_int(-1), "{cap:?}");
        assert!(xs.max().unwrap() >= Fixed::from_int(11), "{cap:?}");
        assert!(flatten_path(&outline).iter().all(|edge| {
            let dx = (edge.p2.x - edge.p1.x).to_f32();
            let dy = (edge.p2.y - edge.p1.y).to_f32();
            dx * dx + dy * dy <= 100.0
        }));
    }
}

#[test]
fn closed_dash_does_not_repeat_the_closing_edge() {
    let mut path = Path::new();
    path.move_to(pt(0, 0))
        .line_to(pt(10, 0))
        .line_to(pt(10, 10))
        .line_to(pt(0, 10))
        .close();
    let (segments, subpaths) = flatten_subpaths_path(&path);
    let mut dashed_segments = Vec::new();
    let mut dashed = Vec::new();
    let mut dashed_semantic_joins = Vec::new();
    apply_dash_pattern(
        &segments[subpaths[0].start..subpaths[0].end],
        true,
        &[Fixed::from_int(100), Fixed::from_int(100)],
        &mut dashed_segments,
        &mut dashed,
        DashJoinMap {
            source_start: subpaths[0].start,
            source_semantic_joins: &[],
            output_semantic_joins: &mut dashed_semantic_joins,
        },
    );
    assert_eq!(dashed.len(), 1);
    assert_eq!(dashed_segments, segments);
    assert!(dashed[0].closed);
}

#[test]
fn open_stroke_reuses_both_rail_buffers() {
    let mut path = Path::new();
    path.move_to(pt(0, 0)).line_to(pt(10, 0));
    let mut outline = Path::new();
    let mut flattened = Vec::new();
    let mut subpaths = Vec::new();
    let mut semantic_joins = Vec::new();
    let mut normals = Vec::new();
    let mut right = Vec::new();
    let mut left = Vec::new();
    let mut arc = Vec::new();
    let mut dashed_segments = Vec::new();
    let mut dashed = Vec::new();
    let mut dashed_semantic_joins = Vec::new();
    let mut first = None;

    for _ in 0..2 {
        offset_polygon_into(
            &path.cmds,
            None,
            Fixed::from_int(2),
            LineCap::Butt,
            LineJoin::Miter,
            Fixed::from_int(4),
            None,
            &mut outline,
            &mut flattened,
            &mut subpaths,
            &mut semantic_joins,
            &mut normals,
            &mut right,
            &mut left,
            &mut arc,
            &mut dashed_segments,
            &mut dashed,
            &mut dashed_semantic_joins,
        );
        let buffers = [
            (left.as_ptr(), left.capacity()),
            (right.as_ptr(), right.capacity()),
        ];
        assert!(buffers.iter().all(|(_, capacity)| *capacity > 0));
        if let Some(previous) = first {
            assert_eq!(buffers, previous);
        }
        first = Some(buffers);
    }
}

#[test]
fn dashed_subpaths_reuse_flat_segment_and_range_buffers() {
    let mut path = Path::new();
    path.move_to(pt(0, 0)).quad_to(pt(10, 12), pt(20, 0));
    path.move_to(pt(0, 20)).line_to(pt(20, 20));
    let mut outline = Path::new();
    let mut flattened = Vec::new();
    let mut subpaths = Vec::new();
    let mut semantic_joins = Vec::new();
    let mut normals = Vec::new();
    let mut right = Vec::new();
    let mut left = Vec::new();
    let mut arc = Vec::new();
    let mut dashed_segments = Vec::new();
    let mut dashed_subpaths = Vec::new();
    let mut dashed_semantic_joins = Vec::new();
    let mut first = None;

    for _ in 0..2 {
        offset_polygon_into(
            &path.cmds,
            None,
            Fixed::from_int(2),
            LineCap::Butt,
            LineJoin::Miter,
            Fixed::from_int(4),
            Some(&[Fixed::from_int(3), Fixed::from_int(2)]),
            &mut outline,
            &mut flattened,
            &mut subpaths,
            &mut semantic_joins,
            &mut normals,
            &mut right,
            &mut left,
            &mut arc,
            &mut dashed_segments,
            &mut dashed_subpaths,
            &mut dashed_semantic_joins,
        );
        let buffers = [
            (flattened.as_ptr().cast::<()>(), flattened.capacity()),
            (subpaths.as_ptr().cast::<()>(), subpaths.capacity()),
            (
                dashed_segments.as_ptr().cast::<()>(),
                dashed_segments.capacity(),
            ),
            (
                dashed_subpaths.as_ptr().cast::<()>(),
                dashed_subpaths.capacity(),
            ),
        ];
        assert!(buffers.iter().all(|(_, capacity)| *capacity > 0));
        if let Some(previous) = first {
            assert_eq!(buffers, previous);
        }
        first = Some(buffers);
    }
}

#[test]
fn offset_polygon_zero_width_is_empty() {
    let mut path = Path::new();
    path.move_to(pt(0, 0)).line_to(pt(10, 0));
    let outline = offset_polygon_path(&path, Fixed::ZERO);
    assert!(outline.cmds.is_empty());
}

#[test]
fn flatten_rounded_rect_produces_expected_count() {
    let p = Path::rounded_rect(
        Fixed::from_int(0),
        Fixed::from_int(0),
        Fixed::from_int(20),
        Fixed::from_int(20),
        Fixed::from_int(4),
    );
    let segs = flatten_path(&p);
    // rounded_rect = 4 lines + 4 quad corners (N=8 each) + possibly 1 close.
    // Layout: move, line, quad, line, quad, line, quad, line, quad, close.
    // Close may add 0 or 1 edge depending on whether final point equals start.
    let expected_min = 4 + 4 * QUAD_STEPS as usize;
    assert!(
        segs.len() >= expected_min,
        "got {} segs, expected ≥{}",
        segs.len(),
        expected_min
    );
}

#[test]
fn projective_flatten_projects_curve_samples_in_path_space() {
    let mut path = Path::new();
    path.move_to(pt(0, 0)).quad_to(pt(8, 12), pt(16, 0));
    let projection = Transform3D::rotate_y_perspective(Fixed::from_int(25), Fixed::from_int(120));
    let mut segments = Vec::new();

    flatten_projective_into(&path.cmds, &projection, &mut segments).unwrap();

    let midpoint = quad_at(pt(0, 0), pt(8, 12), pt(16, 0), Fixed::from_ratio(1, 2));
    assert_eq!(segments[3].p2, projection.apply_point(midpoint).unwrap());
    assert_eq!(segments.len(), QUAD_STEPS as usize);
}

#[test]
fn projective_flatten_rejects_points_behind_the_camera() {
    let path = Path::rect(
        Fixed::ZERO,
        Fixed::ZERO,
        Fixed::from_int(8),
        Fixed::from_int(8),
    );
    let projection = Transform3D {
        m22: crate::types::Fixed64::ZERO,
        ..Transform3D::IDENTITY
    };
    let mut segments = Vec::new();

    assert_eq!(
        flatten_projective_into(&path.cmds, &projection, &mut segments),
        Err(())
    );
}

fn rect_segs(x0: i32, y0: i32, x1: i32, y1: i32, ccw: bool) -> Vec<LineSeg> {
    // Outer ring CW (default) or CCW. Order matters because winding
    // direction is derived from p1.y vs p2.y in the rasterizer.
    if ccw {
        alloc::vec![
            LineSeg {
                p1: pt(x0, y0),
                p2: pt(x0, y1)
            },
            LineSeg {
                p1: pt(x0, y1),
                p2: pt(x1, y1)
            },
            LineSeg {
                p1: pt(x1, y1),
                p2: pt(x1, y0)
            },
            LineSeg {
                p1: pt(x1, y0),
                p2: pt(x0, y0)
            },
        ]
    } else {
        alloc::vec![
            LineSeg {
                p1: pt(x0, y0),
                p2: pt(x1, y0)
            },
            LineSeg {
                p1: pt(x1, y0),
                p2: pt(x1, y1)
            },
            LineSeg {
                p1: pt(x1, y1),
                p2: pt(x0, y1)
            },
            LineSeg {
                p1: pt(x0, y1),
                p2: pt(x0, y0)
            },
        ]
    }
}

#[test]
fn coverage_runs_reconstruct_independent_prefix_sums_without_storage() {
    let mut seed = 0xC139715B_u32;
    for length in 0..=64 {
        for _ in 0..64 {
            let mut differences = [Fixed::ZERO; 64];
            let mut expected = [Fixed::ZERO; 64];
            let mut actual = [Fixed::ZERO; 64];
            let mut sum = Fixed::ZERO;
            for (difference, output) in differences[..length].iter_mut().zip(&mut expected) {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                if seed % 5 == 0 {
                    *difference = Fixed::from_ratio(((seed >> 8) % 7) as i32 - 3, 4);
                }
                sum += *difference;
                *output = sum.max(Fixed::ZERO);
            }
            let mut previous_end = 0;
            let allocations = crate::test_support::tracked_allocations(|| {
                coverage_spans(&differences[..length], |start, end, coverage| {
                    assert!(previous_end <= start && start < end && end <= length);
                    assert!(coverage > Fixed::ZERO);
                    previous_end = end;
                    actual[start..end].fill(coverage);
                });
            });
            assert_eq!(actual, expected);
            assert_eq!(allocations, 0);
        }
    }
}

#[test]
fn row_emission_borrows_scratch_and_preserves_disconnected_fractional_spans() {
    let mut segments = rect_segs(-2, 0, 0, 3, false);
    segments.extend(rect_segs(2, 0, 4, 3, false));
    for segment in &mut segments {
        segment.p1.x += Fixed::from_ratio(1, 2);
        segment.p2.x += Fixed::from_ratio(1, 2);
    }
    for rule in [FillRule::EvenOdd, FillRule::NonZero] {
        let mut acc = Vec::with_capacity(9);
        let mut crossings = Vec::with_capacity(8);
        let storage = acc.as_ptr();
        let mut rows = 0;
        let allocations = crate::test_support::tracked_allocations(|| {
            scanline_fill_rows(
                &segments,
                -3,
                -1,
                6,
                4,
                rule,
                &mut acc,
                &mut crossings,
                |left, y, differences| {
                    assert_eq!(left, -3);
                    assert_eq!(differences.len(), 9);
                    assert_eq!(differences.as_ptr(), storage);
                    assert_eq!(y, rows - 1);
                    rows += 1;
                    let mut coverage = Fixed::ZERO;
                    for (i, difference) in differences.iter().enumerate() {
                        coverage += *difference;
                        let x = left + i as i32;
                        let expected = if !(0..3).contains(&y) {
                            Fixed::ZERO
                        } else {
                            match x {
                                -2 | 0 | 2 | 4 => Fixed::from_ratio(1, 2),
                                -1 | 3 => Fixed::ONE,
                                _ => Fixed::ZERO,
                            }
                        };
                        assert_eq!(coverage, expected, "{x},{y}");
                    }
                },
            );
        });
        assert_eq!(rows, 5);
        assert_eq!(allocations, 0);
    }
}

#[test]
fn interval_differences_match_independent_pixel_intersections() {
    let mut seed = 0x7391A5D3_u32;
    for (left, right) in [(-5, -4), (-3, 11), (0, 1), (4, 13), (-17, 31), (9, 9)] {
        let width = (right - left) as usize;
        for _ in 0..1024 {
            let mut differences = [Fixed::ZERO; 64];
            let mut expected = [Fixed::ZERO; 64];
            for _ in 0..5 {
                let mut next = || {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    seed
                };
                let a = Fixed::from_ratio((next() % 2048) as i32 - 1024, 32);
                let b = Fixed::from_ratio((next() % 2048) as i32 - 1024, 32);
                let weight = Fixed::from_ratio((next() % 5) as i32, 16);
                accumulate_interval(&mut differences[..width], left, right, a, b, weight);
                for pixel in left..right {
                    let lo = a.min(b).max(Fixed::from_int(pixel));
                    let hi = a.max(b).min(Fixed::from_int(pixel + 1));
                    expected[(pixel - left) as usize] += (hi - lo).max(Fixed::ZERO) * weight;
                }
            }
            let mut coverage = Fixed::ZERO;
            for (difference, expected) in differences[..width].iter().zip(&expected[..width]) {
                coverage += *difference;
                assert_eq!(coverage, *expected);
            }
        }
    }
    for endpoint in [-513, -256, -1, 0, 1, 255, 256, 513] {
        let mut differences = [Fixed::ZERO; 3];
        let point = Fixed::from_ratio(endpoint, 256);
        accumulate_interval(&mut differences, -1, 2, point, point, Fixed::ONE);
        assert_eq!(differences, [Fixed::ZERO; 3]);
    }
}

fn count_filled(segs: &[LineSeg], rule: FillRule) -> i32 {
    let mut n = 0;
    let mut acc = Vec::new();
    let mut crossings = Vec::new();
    scanline_fill(
        segs,
        0,
        0,
        10,
        10,
        rule,
        &mut acc,
        &mut crossings,
        |_, _, cov| {
            if cov > Fixed::from_int(0) {
                n += 1;
            }
        },
    );
    n
}

#[test]
fn fill_rule_even_odd_carves_inner_ring_to_zero() {
    // Outer 0..10 + inner 2..8 (same direction). Even-odd makes the
    // inner ring punch a hole; non-zero would fill solid.
    let mut segs = rect_segs(0, 0, 10, 10, false);
    segs.extend(rect_segs(2, 2, 8, 8, false));
    let lit = count_filled(&segs, FillRule::EvenOdd);
    assert!(lit > 0, "outer ring still fills with hole");
    // Inside hole should be 0; only outer band lit.
    // 10x10 = 100 total, hole 6x6 = 36, ring = 64.
    assert!(lit < 70, "expected hole, got lit={}", lit);
}

#[test]
fn fill_rule_non_zero_fills_overlapping_same_direction_solid() {
    // Outer + inner same winding. Non-zero stays inside (count = 2),
    // entire 10x10 should be lit. Even-odd would punch a hole.
    let mut segs = rect_segs(0, 0, 10, 10, false);
    segs.extend(rect_segs(2, 2, 8, 8, false));
    let lit = count_filled(&segs, FillRule::NonZero);
    assert_eq!(lit, 100, "non-zero with same-winding should fill solid");
}

#[test]
fn fill_rule_non_zero_carves_hole_when_inner_reversed() {
    // Outer CW, inner CCW (opposite winding). Non-zero count cancels
    // inside the inner — TrueType "even-odd-like via path direction".
    let mut segs = rect_segs(0, 0, 10, 10, false);
    segs.extend(rect_segs(2, 2, 8, 8, true));
    let lit = count_filled(&segs, FillRule::NonZero);
    // 10x10 = 100, hole 6x6 = 36, ring = 64.
    assert!(
        (60..70).contains(&lit),
        "non-zero with opposite winding should carve a hole, got lit={}",
        lit
    );
}
