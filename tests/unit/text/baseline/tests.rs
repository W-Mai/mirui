use super::*;
use crate::text::layout::{TextLayoutCache, TextLayoutRequest};
use textflow::bidi::BaseDirection;
use textflow::layout::{Alignment, Overflow, TextSpacing, WrapMode};
use textflow::shaping::{
    FlowPoint, FontAccessError, FontId, FontMetrics, GlyphId, GlyphSource, SimpleTypeface,
};

fn point(x: i32, y: i32) -> Point {
    Point::new(x, y)
}

fn path_id(path: Path) -> PathId {
    let mut store = PathStore::new(1).unwrap();
    store.insert(path).unwrap()
}

fn placement_key(store: &PathStore, path: TextPath, layout: TextLayoutHandle) -> PlacementKey {
    PlacementKey::new(
        Entity {
            id: 1,
            generation: 0,
        },
        layout,
        path,
        store.revision(path.path()).unwrap(),
        DEFAULT_TOLERANCE,
    )
}

fn test_layout(layouts: &mut TextLayoutCache, text: &str) -> TextLayoutHandle {
    let face = SimpleTypeface::new(&Mono);
    layouts
        .layout(
            TextLayoutRequest {
                text,
                max_width: 100 << 8,
                width: Some(100 << 8),
                max_lines: 1,
                line_height: 256,
                baseline: 192,
                direction: BaseDirection::LeftToRight,
                wrap: WrapMode::NoWrap,
                alignment: Alignment::Start,
                overflow: Overflow::Clip,
                spacing: TextSpacing::default(),
                features: &[],
                line_widths: None,
            },
            &[&face],
        )
        .unwrap()
}

fn placement_fixture() -> (TextLayoutCache, TextLayoutHandle, PathStore, TextPath) {
    let mut layouts = TextLayoutCache::default();
    let handle = test_layout(&mut layouts, "ab");
    let mut store = PathStore::new(1).unwrap();
    let path = store
        .insert(Path::from_owned(alloc::vec![
            PathCmd::MoveTo(point(0, 0)),
            PathCmd::QuadTo {
                ctrl: point(50, 40),
                end: point(100, 0)
            },
        ]))
        .unwrap();
    (layouts, handle, store, TextPath::new(path))
}

struct Mono;

impl GlyphSource for Mono {
    fn id(&self) -> FontId {
        FontId::new(1)
    }

    fn metrics(&self) -> Result<FontMetrics, FontAccessError> {
        Ok(FontMetrics {
            units_per_em: 256,
            ascender: 192,
            descender: -64,
            line_gap: 0,
        })
    }

    fn glyph_for(&self, character: char) -> Result<Option<GlyphId>, FontAccessError> {
        Ok(Some(GlyphId::new(character as u16)))
    }

    fn glyph_advance(&self, _glyph: GlyphId) -> Result<FlowPoint, FontAccessError> {
        Ok(FlowPoint { x: 256, y: 0 })
    }
}

#[test]
fn idle_path_runtime_reserves_no_heap_storage() {
    let runtime = PathTextRuntime::default();

    assert_eq!(runtime.baselines.entries.capacity(), 0);
    assert_eq!(runtime.baselines.segments.capacity(), 0);
    assert_eq!(runtime.placements.glyphs.entries.capacity(), 0);
    assert_eq!(runtime.placements.glyphs.values.capacity(), 0);
    assert_eq!(runtime.placements.carets.entries.capacity(), 0);
    assert_eq!(runtime.placements.carets.values.capacity(), 0);
    assert_eq!(core::mem::size_of::<textflow::placement::GlyphFrame>(), 16);
    assert_eq!(core::mem::size_of::<textflow::placement::CaretFrame>(), 16);
}

#[test]
fn placement_reads_are_shared_epoch_scoped_and_do_not_pin() {
    let (layouts, handle, store, path) = placement_fixture();
    let layout = layouts.get(handle).unwrap();
    let key = placement_key(&store, path, handle);
    let mut runtime = PathTextRuntime::default();
    runtime.begin_layout();
    runtime.begin_placements();
    assert!(runtime.glyph_frames(key, |_| ()).is_none());
    assert!(runtime.caret_frames(key, |_| ()).is_none());
    assert!(runtime.placements.glyphs.entries.is_empty());
    assert!(runtime.placements.carets.entries.is_empty());
    runtime.prepare_glyph_frames(&store, key, &layout).unwrap();
    runtime.prepare_caret_frames(&store, key, &layout).unwrap();
    {
        let used = runtime.placements.glyphs.entries[0].last_used;
        assert_eq!(
            runtime.glyph_frames(key, |glyphs| {
                runtime.caret_frames(key, |carets| (glyphs.len(), carets.len()))
            }),
            Some(Some((2, 3)))
        );
        assert_eq!(runtime.placements.glyphs.entries[0].last_used, used);
    }
    runtime.begin_layout();
    assert_eq!(runtime.glyph_frames(key, <[_]>::len), Some(2));
    runtime.begin_placements();
    {
        let used = runtime.placements.glyphs.entries[0].last_used;
        assert!(runtime.glyph_frames(key, |_| ()).is_none());
        assert!(runtime.caret_frames(key, |_| ()).is_none());
        assert_eq!(runtime.placements.glyphs.entries[0].last_used, used);
    }
    runtime.pin_placement(key);
    assert_eq!(runtime.glyph_frames(key, <[_]>::len), Some(2));
    assert_eq!(runtime.caret_frames(key, <[_]>::len), Some(3));
    runtime.trim_memory();
    assert!(runtime.glyph_frames(key, |_| ()).is_none());
    assert!(runtime.caret_frames(key, |_| ()).is_none());
    assert_eq!(runtime.placements.glyphs.values.capacity(), 0);
    assert_eq!(runtime.placements.carets.values.capacity(), 0);
}

#[test]
fn placement_working_set_is_pinned_before_new_keys_compact_the_arenas() {
    let (layouts, handle, store, path) = placement_fixture();
    let layout = layouts.get(handle).unwrap();
    let old = placement_key(&store, path, handle);
    let retained = PlacementKey {
        owner: Entity {
            id: 2,
            generation: 0,
        },
        ..old
    };
    let incoming = PlacementKey {
        owner: Entity {
            id: 3,
            generation: 0,
        },
        ..old
    };
    let rejected = PlacementKey {
        owner: Entity {
            id: 4,
            generation: 0,
        },
        ..old
    };
    let mut runtime = PathTextRuntime {
        placements: PathPlacementCache::new(4096, 4096, 2),
        ..PathTextRuntime::default()
    };
    runtime.begin_placements();
    for key in [old, retained] {
        runtime.prepare_glyph_frames(&store, key, &layout).unwrap();
        runtime.prepare_caret_frames(&store, key, &layout).unwrap();
    }
    let glyph = runtime.glyph_frames(retained, |frames| frames[0]).unwrap();
    let caret = runtime.caret_frames(retained, |frames| frames[0]).unwrap();
    runtime.begin_placements();
    runtime.pin_placement(retained);
    runtime
        .prepare_glyph_frames(&store, incoming, &layout)
        .unwrap();
    runtime
        .prepare_caret_frames(&store, incoming, &layout)
        .unwrap();
    assert!(runtime.glyph_frames(old, |_| ()).is_none());
    assert!(runtime.caret_frames(old, |_| ()).is_none());
    assert_eq!(
        runtime.glyph_frames(retained, |frames| frames[0]),
        Some(glyph)
    );
    assert_eq!(
        runtime.caret_frames(retained, |frames| frames[0]),
        Some(caret)
    );
    assert_eq!(runtime.placements.glyphs.entries[0].key, retained);
    assert_eq!(runtime.placements.glyphs.entries[0].values.start, 0);
    assert_eq!(runtime.placements.carets.entries[0].key, retained);
    assert_eq!(runtime.placements.carets.entries[0].values.start, 0);
    assert!(matches!(
        runtime.prepare_glyph_frames(&store, rejected, &layout),
        Err(PathBaselineError::CacheBudget { .. })
    ));
    assert!(matches!(
        runtime.prepare_caret_frames(&store, rejected, &layout),
        Err(PathBaselineError::CacheBudget { .. })
    ));
    for key in [retained, incoming] {
        assert_eq!(runtime.glyph_frames(key, <[_]>::len), Some(2));
        assert_eq!(runtime.caret_frames(key, <[_]>::len), Some(3));
    }
}

#[test]
fn placement_keys_cover_owner_generation_layout_path_revision_and_tolerance() {
    let (mut layouts, handle, mut store, path) = placement_fixture();
    let different_layout = test_layout(&mut layouts, "abc");
    let layout = layouts.get(handle).unwrap();
    let key = placement_key(&store, path, handle);
    let mut runtime = PathTextRuntime::default();
    runtime.begin_placements();
    runtime.prepare_glyph_frames(&store, key, &layout).unwrap();
    runtime.prepare_caret_frames(&store, key, &layout).unwrap();
    for different in [
        PlacementKey {
            owner: Entity {
                id: 2,
                generation: 0,
            },
            ..key
        },
        PlacementKey {
            owner: Entity {
                id: 1,
                generation: 1,
            },
            ..key
        },
        PlacementKey {
            layout: different_layout,
            ..key
        },
        PlacementKey {
            path: path.with_offset(Fixed::ONE),
            ..key
        },
        PlacementKey {
            tolerance: Fixed::HALF,
            ..key
        },
    ] {
        assert!(runtime.glyph_frames(different, |_| ()).is_none());
        assert!(runtime.caret_frames(different, |_| ()).is_none());
    }
    store
        .edit(path.path(), |path| {
            path.set_command(
                1,
                PathCmd::QuadTo {
                    ctrl: point(50, 80),
                    end: point(100, 0),
                },
            )
            .unwrap();
        })
        .unwrap();
    let revised = placement_key(&store, path, handle);
    assert_ne!(revised, key);
    assert!(runtime.glyph_frames(revised, |_| ()).is_none());
    assert_eq!(
        runtime.prepare_glyph_frames(&store, key, &layout),
        Err(PathBaselineError::Unavailable)
    );
    assert_eq!(
        runtime.prepare_caret_frames(&store, key, &layout),
        Err(PathBaselineError::Unavailable)
    );
    runtime.begin_layout();
    runtime.begin_placements();
    runtime
        .prepare_glyph_frames(&store, revised, &layout)
        .unwrap();
    runtime
        .prepare_caret_frames(&store, revised, &layout)
        .unwrap();
    assert!(runtime.glyph_frames(key, |_| ()).is_none());
    assert!(runtime.caret_frames(key, |_| ()).is_none());
    assert_eq!(runtime.glyph_frames(revised, <[_]>::len), Some(2));
    assert_eq!(runtime.caret_frames(revised, <[_]>::len), Some(3));
}

#[test]
fn caret_budget_failure_does_not_discard_prepared_glyphs() {
    let (layouts, handle, store, path) = placement_fixture();
    let layout = layouts.get(handle).unwrap();
    let key = placement_key(&store, path, handle);
    let mut runtime = PathTextRuntime {
        placements: PathPlacementCache::new(4096, 1, 4),
        ..PathTextRuntime::default()
    };
    runtime.begin_placements();
    runtime.prepare_glyph_frames(&store, key, &layout).unwrap();
    assert!(matches!(
        runtime.prepare_caret_frames(&store, key, &layout),
        Err(PathBaselineError::CacheBudget { budget: 1, .. })
    ));
    assert_eq!(runtime.glyph_frames(key, <[_]>::len), Some(2));
    assert!(runtime.caret_frames(key, |_| ()).is_none());
    assert!(runtime.placements.carets.entries.is_empty());
    assert!(runtime.placements.carets.values.is_empty());
}

#[test]
fn placement_budget_eviction_counts_surviving_ranges() {
    let (_, handle, store, path) = placement_fixture();
    let old = placement_key(&store, path, handle);
    let incoming = PlacementKey {
        owner: Entity {
            id: 2,
            generation: 0,
        },
        ..old
    };
    let mut arena = FrameArena::<u32>::new(frame_capacity_bytes::<u32>(1, 1000), 4);
    arena
        .resolve(old, 1000, |values| {
            values.fill(1);
            Ok(())
        })
        .unwrap();
    arena.begin_frame();
    arena
        .resolve(incoming, 1000, |values| {
            values.fill(2);
            Ok(())
        })
        .unwrap();
    assert!(arena.get(old).is_none());
    assert_eq!(arena.get(incoming).unwrap(), &[2; 1000]);
    assert_eq!(arena.entries.len(), 1);
    assert_eq!(arena.entries[0].values, 0..1000);
}

#[test]
fn placement_admission_still_accounts_for_retained_capacity() {
    let (_, handle, store, path) = placement_fixture();
    let first = placement_key(&store, path, handle);
    let second = PlacementKey {
        owner: Entity {
            id: 2,
            generation: 0,
        },
        ..first
    };
    let incoming = PlacementKey {
        owner: Entity {
            id: 3,
            generation: 0,
        },
        ..first
    };
    let budget = frame_capacity_bytes::<u32>(2, 100);
    let mut arena = FrameArena::<u32>::new(budget, 4);
    for key in [first, second] {
        arena.resolve(key, 50, |_| Ok(())).unwrap();
    }
    arena.begin_frame();
    assert!(frame_capacity_bytes::<u32>(1, 101) <= budget);
    let allocations = crate::test_support::tracked_allocations(|| {
        assert!(matches!(
            arena.resolve(incoming, 101, |_| Ok(())),
            Err(PathBaselineError::CacheBudget { .. })
        ));
    });
    assert_eq!(allocations, 0);
    assert_eq!(arena.entries.capacity(), 2);
    assert_eq!(arena.values.capacity(), 100);
    assert!(arena.get(incoming).is_none());
}

#[test]
fn warmed_placement_preparation_and_reads_do_not_allocate() {
    let (layouts, handle, store, path) = placement_fixture();
    let layout = layouts.get(handle).unwrap();
    let key = placement_key(&store, path, handle);
    let mut runtime = PathTextRuntime::default();
    runtime.begin_layout();
    runtime.begin_placements();
    runtime.prepare_glyph_frames(&store, key, &layout).unwrap();
    runtime.prepare_caret_frames(&store, key, &layout).unwrap();
    let allocations = crate::test_support::tracked_allocations(|| {
        for _ in 0..16 {
            runtime.begin_layout();
            runtime.begin_placements();
            runtime.pin_placement(key);
            runtime.prepare_glyph_frames(&store, key, &layout).unwrap();
            runtime.prepare_caret_frames(&store, key, &layout).unwrap();
            assert_eq!(runtime.glyph_frames(key, <[_]>::len), Some(2));
            assert_eq!(runtime.caret_frames(key, <[_]>::len), Some(3));
        }
    });
    assert_eq!(allocations, 0);
}

#[test]
fn line_path_samples_commands_without_measured_storage() {
    let path = Path::from_owned(alloc::vec![
        PathCmd::MoveTo(point(0, 0)),
        PathCmd::LineTo(point(10, 0)),
        PathCmd::LineTo(point(10, 10)),
    ]);
    let measure = PathMeasure::new(&path, 0, Fixed::from_ratio(1, 4)).unwrap();
    assert!(measure.is_line_only());
    let line = measure.line().unwrap();
    assert_eq!(line.length(), Fixed::from_int(20));

    let mut cursor = line.cursor();
    assert_eq!(
        cursor.sample_forward(Fixed::from_int(5)).unwrap(),
        BaselineSample {
            position: point(5, 0),
            unit_tangent: point(1, 0),
        }
    );
    assert_eq!(
        cursor.sample_forward(Fixed::from_int(10)).unwrap(),
        BaselineSample {
            position: point(10, 0),
            unit_tangent: point(0, 1),
        }
    );
}

#[test]
fn reverse_window_scans_segments_once_in_reverse_order() {
    use textflow::placement::{BaselineCursor as _, TextBaseline as _};

    let path = Path::from_owned(alloc::vec![
        PathCmd::MoveTo(point(0, 0)),
        PathCmd::LineTo(point(10, 0)),
        PathCmd::LineTo(point(10, 10)),
    ]);
    let baseline = PathBaseline::Line(
        PathMeasure::new(&path, 0, DEFAULT_TOLERANCE)
            .unwrap()
            .line()
            .unwrap(),
    );
    let id = path_id(path.clone());
    let window = BaselineWindow::new(
        baseline,
        TextPath::new(id).with_direction(PathDirection::Reverse),
    )
    .unwrap();
    let mut cursor = window.cursor();

    assert_eq!(
        cursor.sample_forward(0).unwrap(),
        textflow::placement::BaselineSample {
            position: FlowPoint {
                x: 10 << 8,
                y: 10 << 8,
            },
            unit_tangent: FlowPoint { x: 0, y: -256 },
        }
    );
    assert_eq!(
        cursor.sample_forward(10 << 8).unwrap(),
        textflow::placement::BaselineSample {
            position: FlowPoint { x: 10 << 8, y: 0 },
            unit_tangent: FlowPoint { x: -256, y: 0 },
        }
    );
}

#[test]
fn offset_advances_forward_and_reverse_window_origins() {
    use textflow::placement::{BaselineCursor as _, TextBaseline as _};

    let path = Path::from_owned(alloc::vec![
        PathCmd::MoveTo(point(0, 0)),
        PathCmd::LineTo(point(20, 0)),
    ]);
    let baseline = PathBaseline::Line(
        PathMeasure::new(&path, 0, DEFAULT_TOLERANCE)
            .unwrap()
            .line()
            .unwrap(),
    );
    let id = path_id(path.clone());
    let forward =
        BaselineWindow::new(baseline, TextPath::new(id).with_offset(Fixed::from_int(4))).unwrap();
    let reverse = BaselineWindow::new(
        baseline,
        TextPath::new(id)
            .with_offset(Fixed::from_int(4))
            .with_direction(PathDirection::Reverse),
    )
    .unwrap();

    assert_eq!(forward.length(), 16 << 8);
    assert_eq!(reverse.length(), 16 << 8);
    assert_eq!(
        forward.cursor().sample_forward(0).unwrap().position,
        FlowPoint { x: 4 << 8, y: 0 }
    );
    assert_eq!(
        reverse.cursor().sample_forward(0).unwrap().position,
        FlowPoint { x: 16 << 8, y: 0 }
    );
}

#[test]
fn reverse_window_supports_measured_curves() {
    use textflow::placement::{BaselineCursor as _, TextBaseline as _};

    let path = Path::from_owned(alloc::vec![
        PathCmd::MoveTo(point(0, 0)),
        PathCmd::QuadTo {
            ctrl: point(5, 10),
            end: point(10, 0),
        },
    ]);
    let measure = PathMeasure::new(&path, 0, DEFAULT_TOLERANCE).unwrap();
    let requirements = measure.requirements().unwrap();
    let mut segments = alloc::vec![EMPTY_SEGMENT; requirements.segments];
    let measured = measure.measure_into(&mut segments).unwrap();
    let id = path_id(path.clone());
    let window = BaselineWindow::new(
        PathBaseline::Measured(measured),
        TextPath::new(id).with_direction(PathDirection::Reverse),
    )
    .unwrap();
    let mut cursor = window.cursor();

    let start = cursor.sample_forward(0).unwrap();
    let end = cursor
        .sample_forward(to_textflow(requirements.length))
        .unwrap();
    assert_eq!(start.position, FlowPoint { x: 10 << 8, y: 0 });
    assert_eq!(end.position, FlowPoint { x: 0, y: 0 });
    assert!(start.unit_tangent.x < 0);
    assert!(end.unit_tangent.x < 0);
}

#[test]
fn closed_seam_wraps_without_restarting_each_sample() {
    use textflow::placement::{BaselineCursor as _, TextBaseline as _};

    let path = Path::from_owned(alloc::vec![
        PathCmd::MoveTo(point(0, 0)),
        PathCmd::LineTo(point(10, 0)),
        PathCmd::LineTo(point(10, 10)),
        PathCmd::LineTo(point(0, 10)),
        PathCmd::Close,
    ]);
    let baseline = PathBaseline::Line(
        PathMeasure::new(&path, 0, DEFAULT_TOLERANCE)
            .unwrap()
            .line()
            .unwrap(),
    );
    let id = path_id(path.clone());
    let window =
        BaselineWindow::new(baseline, TextPath::new(id).with_seam(Fixed::from_int(30))).unwrap();
    let mut cursor = window.cursor();

    assert_eq!(
        cursor.sample_forward(0).unwrap().position,
        FlowPoint { x: 0, y: 10 << 8 }
    );
    assert_eq!(
        cursor.sample_forward(20 << 8).unwrap().position,
        FlowPoint { x: 10 << 8, y: 0 }
    );
}

#[test]
fn offset_is_applied_after_the_closed_seam() {
    use textflow::placement::{BaselineCursor as _, TextBaseline as _};

    let path = Path::from_owned(alloc::vec![
        PathCmd::MoveTo(point(0, 0)),
        PathCmd::LineTo(point(10, 0)),
        PathCmd::LineTo(point(10, 10)),
        PathCmd::LineTo(point(0, 10)),
        PathCmd::Close,
    ]);
    let baseline = PathBaseline::Line(
        PathMeasure::new(&path, 0, DEFAULT_TOLERANCE)
            .unwrap()
            .line()
            .unwrap(),
    );
    let id = path_id(path.clone());
    let window = BaselineWindow::new(
        baseline,
        TextPath::new(id)
            .with_seam(Fixed::from_int(38))
            .with_offset(Fixed::from_int(4)),
    )
    .unwrap();

    assert_eq!(
        window.cursor().sample_forward(0).unwrap().position,
        FlowPoint { x: 2 << 8, y: 0 }
    );
}

#[test]
fn reverse_closed_seam_wraps_in_one_monotonic_domain() {
    use textflow::placement::{BaselineCursor as _, TextBaseline as _};

    let path = Path::from_owned(alloc::vec![
        PathCmd::MoveTo(point(0, 0)),
        PathCmd::LineTo(point(10, 0)),
        PathCmd::LineTo(point(10, 10)),
        PathCmd::LineTo(point(0, 10)),
        PathCmd::Close,
    ]);
    let baseline = PathBaseline::Line(
        PathMeasure::new(&path, 0, DEFAULT_TOLERANCE)
            .unwrap()
            .line()
            .unwrap(),
    );
    let id = path_id(path.clone());
    let window = BaselineWindow::new(
        baseline,
        TextPath::new(id)
            .with_direction(PathDirection::Reverse)
            .with_seam(Fixed::from_int(10)),
    )
    .unwrap();
    let mut cursor = window.cursor();

    assert_eq!(
        cursor.sample_forward(0).unwrap().position,
        FlowPoint { x: 10 << 8, y: 0 }
    );
    assert_eq!(
        cursor.sample_forward(20 << 8).unwrap().position,
        FlowPoint { x: 0, y: 10 << 8 }
    );
}

#[test]
fn seam_requires_closed_geometry() {
    let path = Path::from_owned(alloc::vec![
        PathCmd::MoveTo(point(0, 0)),
        PathCmd::LineTo(point(10, 0)),
    ]);
    let id = path_id(path.clone());
    let baseline = PathBaseline::Line(
        PathMeasure::new(&path, 0, DEFAULT_TOLERANCE)
            .unwrap()
            .line()
            .unwrap(),
    );

    assert!(matches!(
        BaselineWindow::new(baseline, TextPath::new(id).with_seam(Fixed::from_int(5)),),
        Err(PathBaselineError::OpenSeam)
    ));
}

#[test]
fn selected_subpath_does_not_measure_siblings() {
    let path = Path::from_owned(alloc::vec![
        PathCmd::MoveTo(point(0, 0)),
        PathCmd::LineTo(point(100, 0)),
        PathCmd::MoveTo(point(7, 9)),
        PathCmd::LineTo(point(7, 19)),
    ]);
    let measure = PathMeasure::new(&path, 1, Fixed::ONE).unwrap();
    let line = measure.line().unwrap();
    assert_eq!(line.length(), Fixed::from_int(10));
    assert_eq!(
        line.cursor().sample_forward(Fixed::ZERO).unwrap().position,
        point(7, 9)
    );
}

#[test]
fn close_returns_to_the_selected_subpath_start() {
    let path = Path::from_owned(alloc::vec![
        PathCmd::MoveTo(point(0, 0)),
        PathCmd::LineTo(point(3, 0)),
        PathCmd::LineTo(point(3, 4)),
        PathCmd::Close,
    ]);
    let line = PathMeasure::new(&path, 0, Fixed::ONE)
        .unwrap()
        .line()
        .unwrap();
    assert_eq!(line.length(), Fixed::from_int(12));
    assert_eq!(
        line.cursor()
            .sample_forward(Fixed::from_int(12))
            .unwrap()
            .position,
        point(0, 0)
    );
}

#[test]
fn curve_measurement_is_atomic_when_capacity_is_short() {
    let path = Path::from_owned(alloc::vec![
        PathCmd::MoveTo(point(0, 0)),
        PathCmd::QuadTo {
            ctrl: point(50, 100),
            end: point(100, 0),
        },
    ]);
    let measure = PathMeasure::new(&path, 0, Fixed::from_ratio(1, 8)).unwrap();
    let requirements = measure.requirements().unwrap();
    assert!(requirements.segments > 1);
    let sentinel = MeasuredSegment {
        end: point(-1, -1),
        end_distance: Fixed::MAX,
    };
    let mut output = alloc::vec![sentinel; requirements.segments - 1];
    match measure.measure_into(&mut output) {
        Err(PathBaselineError::InsufficientCapacity { required, provided }) => {
            assert_eq!(required, requirements.segments);
            assert_eq!(provided, requirements.segments - 1);
        }
        _ => panic!("short output must fail before writing"),
    }
    assert!(output.iter().all(|segment| *segment == sentinel));
}

#[test]
fn tighter_tolerance_produces_at_least_as_many_segments() {
    let path = Path::from_owned(alloc::vec![
        PathCmd::MoveTo(point(0, 0)),
        PathCmd::CubicTo {
            ctrl1: point(0, 100),
            ctrl2: point(100, 100),
            end: point(100, 0),
        },
    ]);
    let loose = PathMeasure::new(&path, 0, Fixed::from_int(4))
        .unwrap()
        .requirements()
        .unwrap();
    let tight = PathMeasure::new(&path, 0, Fixed::from_ratio(1, 8))
        .unwrap()
        .requirements()
        .unwrap();
    assert!(tight.segments >= loose.segments);
    assert!(tight.length >= loose.length);
}

#[test]
fn measured_curve_interpolates_tangents_across_flattened_segments() {
    let path = Path::from_owned(alloc::vec![
        PathCmd::MoveTo(point(0, 0)),
        PathCmd::CubicTo {
            ctrl1: point(0, 100),
            ctrl2: point(100, 100),
            end: point(100, 0),
        },
    ]);
    let measure = PathMeasure::new(&path, 0, Fixed::from_int(2)).unwrap();
    let requirements = measure.requirements().unwrap();
    let mut output = alloc::vec![EMPTY_SEGMENT; requirements.segments];
    let baseline = measure.measure_into(&mut output).unwrap();
    let boundary = baseline.segments[requirements.segments / 2 - 1].end_distance;
    let epsilon = Fixed::from_ratio(1, 256);
    let mut cursor = baseline.cursor();
    let before = cursor.sample_forward(boundary - epsilon).unwrap();
    let at = cursor.sample_forward(boundary).unwrap();
    let after = cursor.sample_forward(boundary + epsilon).unwrap();

    for adjacent in [(before, at), (at, after)] {
        assert!((adjacent.0.unit_tangent.x - adjacent.1.unit_tangent.x).abs() <= epsilon);
        assert!((adjacent.0.unit_tangent.y - adjacent.1.unit_tangent.y).abs() <= epsilon);
    }
}

#[test]
fn measured_cursor_uses_outgoing_tangent_at_segment_boundary() {
    let path = Path::from_owned(alloc::vec![
        PathCmd::MoveTo(point(0, 0)),
        PathCmd::LineTo(point(10, 0)),
        PathCmd::LineTo(point(10, 10)),
    ]);
    let measure = PathMeasure::new(&path, 0, Fixed::ONE).unwrap();
    let requirements = measure.requirements().unwrap();
    let mut output = alloc::vec![
        MeasuredSegment {
            end: Point::ZERO,
            end_distance: Fixed::ZERO,
        };
        requirements.segments
    ];
    let baseline = measure.measure_into(&mut output).unwrap();
    assert_eq!(baseline.length(), Fixed::from_int(20));
    assert_eq!(
        baseline
            .cursor()
            .sample_forward(Fixed::from_int(10))
            .unwrap()
            .unit_tangent,
        point(0, 1)
    );
}

#[test]
fn degenerate_and_out_of_order_queries_fail_explicitly() {
    let empty = Path::from_owned(alloc::vec![
        PathCmd::MoveTo(point(1, 1)),
        PathCmd::LineTo(point(1, 1)),
    ]);
    assert!(matches!(
        PathMeasure::new(&empty, 0, Fixed::ONE)
            .unwrap()
            .requirements(),
        Err(PathBaselineError::Empty)
    ));

    let line = Path::from_owned(alloc::vec![
        PathCmd::MoveTo(point(0, 0)),
        PathCmd::LineTo(point(10, 0)),
    ]);
    let baseline = PathMeasure::new(&line, 0, Fixed::ONE)
        .unwrap()
        .line()
        .unwrap();
    let mut cursor = baseline.cursor();
    cursor.sample_forward(Fixed::from_int(8)).unwrap();
    assert!(matches!(
        cursor.sample_forward(Fixed::from_int(7)),
        Err(PathBaselineError::DistanceOrder { .. })
    ));
}

#[test]
fn zero_segments_and_cusps_use_the_next_nonzero_tangent() {
    let path = Path::from_owned(alloc::vec![
        PathCmd::MoveTo(point(0, 0)),
        PathCmd::LineTo(point(0, 0)),
        PathCmd::LineTo(point(10, 0)),
        PathCmd::LineTo(point(0, 0)),
        PathCmd::LineTo(point(0, 0)),
    ]);
    let baseline = PathMeasure::new(&path, 0, Fixed::ONE)
        .unwrap()
        .line()
        .unwrap();
    let mut cursor = baseline.cursor();

    assert_eq!(
        cursor.sample_forward(Fixed::ZERO).unwrap().unit_tangent,
        point(1, 0)
    );
    assert_eq!(
        cursor
            .sample_forward(Fixed::from_int(10))
            .unwrap()
            .unit_tangent,
        point(-1, 0)
    );
    assert_eq!(
        cursor
            .sample_forward(Fixed::from_int(20))
            .unwrap()
            .unit_tangent,
        point(-1, 0)
    );
}

#[test]
fn extreme_coordinate_span_reports_length_overflow() {
    let path = Path::from_owned(alloc::vec![
        PathCmd::MoveTo(Point {
            x: Fixed::MIN,
            y: Fixed::ZERO,
        }),
        PathCmd::LineTo(Point {
            x: Fixed::MAX,
            y: Fixed::ZERO,
        }),
    ]);
    assert_eq!(
        PathMeasure::new(&path, 0, Fixed::ONE)
            .unwrap()
            .requirements(),
        Err(PathBaselineError::LengthOverflow)
    );
}

#[test]
fn measured_segment_storage_is_compact() {
    assert_eq!(core::mem::size_of::<MeasuredSegment>(), 12);
}

#[test]
fn line_paths_never_enter_the_measured_arena() {
    let path = Path::from_owned(alloc::vec![
        PathCmd::MoveTo(point(0, 0)),
        PathCmd::LineTo(point(100, 0)),
    ]);
    let mut cache = PathBaselineCache::new(1_024, 4);
    let key = MeasurementKey {
        path: path_id(path.clone()),
        revision: None,
        subpath: 0,
        tolerance: DEFAULT_TOLERANCE,
    };
    assert!(matches!(
        cache.resolve(key, &path).unwrap(),
        PathBaseline::Line(_)
    ));
    assert!(cache.entries.is_empty());
    assert!(cache.segments.is_empty());
}

#[test]
fn measured_cache_reuses_matching_revision_and_tolerance() {
    let path = Path::from_owned(alloc::vec![
        PathCmd::MoveTo(point(0, 0)),
        PathCmd::QuadTo {
            ctrl: point(50, 100),
            end: point(100, 0),
        },
    ]);
    let mut cache = PathBaselineCache::new(8 * 1_024, 4);
    let key = MeasurementKey {
        path: path_id(path.clone()),
        revision: Some(PathRevision::default()),
        subpath: 0,
        tolerance: DEFAULT_TOLERANCE,
    };
    let first_length = cache.resolve(key, &path).unwrap().length();
    let segment_count = cache.segments.len();
    let second_length = cache.resolve(key, &path).unwrap().length();
    assert_eq!(second_length, first_length);
    assert_eq!(cache.entries.len(), 1);
    assert_eq!(cache.segments.len(), segment_count);
}

#[test]
fn animated_path_revisions_replace_stale_measurement_and_placement_entries() {
    let face = SimpleTypeface::new(&Mono);
    let mut layouts = TextLayoutCache::default();
    let handle = layouts
        .layout(
            TextLayoutRequest {
                text: "animated",
                max_width: 100 << 8,
                width: Some(100 << 8),
                max_lines: 1,
                line_height: 256,
                baseline: 192,
                direction: BaseDirection::LeftToRight,
                wrap: WrapMode::NoWrap,
                alignment: Alignment::Start,
                overflow: Overflow::Clip,
                spacing: TextSpacing::default(),
                features: &[],
                line_widths: None,
            },
            &[&face],
        )
        .unwrap();
    let layout = layouts.get(handle).unwrap();
    let mut store = PathStore::new(1).unwrap();
    let path = store
        .insert(Path::from_owned(alloc::vec![
            PathCmd::MoveTo(point(0, 0)),
            PathCmd::QuadTo {
                ctrl: point(50, 40),
                end: point(100, 0),
            },
        ]))
        .unwrap();
    let text_path = TextPath::new(path).with_range(Fixed::ZERO..Fixed::from_int(100));
    let mut runtime = PathTextRuntime::default();

    for frame in 0..64 {
        runtime.begin_layout();
        runtime.begin_placements();
        store
            .edit(path, |path| {
                path.set_command(
                    1,
                    PathCmd::QuadTo {
                        ctrl: point(50, 40 + frame),
                        end: point(100, 0),
                    },
                )
                .unwrap();
            })
            .unwrap();
        let key = placement_key(&store, text_path, handle);
        runtime.prepare_glyph_frames(&store, key, &layout).unwrap();
        assert!(runtime.glyph_frames(key, |_| ()).is_some());
    }

    assert_eq!(runtime.baselines.entries.len(), 1);
    assert_eq!(runtime.placements.glyphs.entries.len(), 1);
}

#[test]
fn measured_cache_compacts_after_lru_eviction() {
    let first = Path::from_owned(alloc::vec![
        PathCmd::MoveTo(point(0, 0)),
        PathCmd::QuadTo {
            ctrl: point(20, 40),
            end: point(40, 0),
        },
    ]);
    let second = Path::from_owned(alloc::vec![
        PathCmd::MoveTo(point(0, 0)),
        PathCmd::CubicTo {
            ctrl1: point(0, 40),
            ctrl2: point(40, 40),
            end: point(40, 0),
        },
    ]);
    let mut cache = PathBaselineCache::new(8 * 1_024, 1);
    let first_id = path_id(first.clone());
    let mut store = PathStore::new(2).unwrap();
    store.insert(Path::new()).unwrap();
    let second_id = store.insert(second.clone()).unwrap();
    let key = |path| MeasurementKey {
        path,
        revision: None,
        subpath: 0,
        tolerance: DEFAULT_TOLERANCE,
    };
    cache.resolve(key(first_id), &first).unwrap();
    cache.begin_frame();
    cache.resolve(key(second_id), &second).unwrap();

    assert_eq!(cache.entries.len(), 1);
    assert_eq!(cache.entries[0].key, key(second_id));
    assert_eq!(cache.entries[0].segments.start, 0);
    assert_eq!(cache.entries[0].segments.end, cache.segments.len());
}

#[test]
fn measured_budget_eviction_counts_surviving_ranges() {
    let (_, _, source_store, source_path) = placement_fixture();
    let path = source_store.get(source_path.path()).unwrap();
    let count = PathMeasure::new(path, 0, DEFAULT_TOLERANCE)
        .unwrap()
        .requirements()
        .unwrap()
        .segments;
    let mut store = PathStore::new(2).unwrap();
    let first_id = store.insert(path.clone()).unwrap();
    let second_id = store.insert(path.clone()).unwrap();
    let first = MeasurementKey {
        path: first_id,
        revision: None,
        subpath: 0,
        tolerance: DEFAULT_TOLERANCE,
    };
    let second = MeasurementKey {
        path: second_id,
        ..first
    };
    let mut cache = PathBaselineCache::new(capacity_bytes(1, count), 4);
    let expected = cache.resolve(first, path).unwrap().length();
    cache.begin_frame();
    assert_eq!(cache.resolve(second, path).unwrap().length(), expected);
    assert_eq!(cache.entries.len(), 1);
    assert_eq!(cache.entries[0].key, second);
    assert_eq!(cache.entries[0].segments, 0..count);
}

#[test]
fn failed_baseline_admission_compacts_evicted_segments_and_preserves_pins() {
    let (_, _, source_store, source_path) = placement_fixture();
    let path = source_store.get(source_path.path()).unwrap();
    let count = PathMeasure::new(path, 0, DEFAULT_TOLERANCE)
        .unwrap()
        .requirements()
        .unwrap()
        .segments;
    let mut store = PathStore::new(3).unwrap();
    let first = MeasurementKey {
        path: store.insert(path.clone()).unwrap(),
        revision: None,
        subpath: 0,
        tolerance: DEFAULT_TOLERANCE,
    };
    let pinned = MeasurementKey {
        path: store.insert(path.clone()).unwrap(),
        ..first
    };
    let incoming = MeasurementKey {
        path: store.insert(path.clone()).unwrap(),
        ..first
    };
    let mut cache = PathBaselineCache::new(capacity_bytes(2, 2 * count), 4);
    cache.resolve(first, path).unwrap();
    let expected = cache.resolve(pinned, path).unwrap().length();
    cache.begin_frame();
    cache.resolve(pinned, path).unwrap();
    assert!(matches!(
        cache.make_room(count + 1),
        Err(PathBaselineError::CacheBudget { .. })
    ));
    assert_eq!(cache.segments.len(), count);
    assert_eq!(cache.entries[0].segments, 0..count);
    assert_eq!(cache.prepared(pinned, path).unwrap().length(), expected);
    assert_eq!(cache.resolve(incoming, path).unwrap().length(), expected);
    assert_eq!(cache.prepared(pinned, path).unwrap().length(), expected);
}

#[test]
fn failed_frame_admission_compacts_evicted_values_and_preserves_pins() {
    let (_, handle, store, path) = placement_fixture();
    let old = placement_key(&store, path, handle);
    let pinned = PlacementKey {
        owner: Entity {
            id: 2,
            generation: 0,
        },
        ..old
    };
    let incoming = PlacementKey {
        owner: Entity {
            id: 3,
            generation: 0,
        },
        ..old
    };
    let mut arena = FrameArena::<u32>::new(frame_capacity_bytes::<u32>(2, 100), 4);
    arena.resolve(old, 50, |_| Ok(())).unwrap();
    arena
        .resolve(pinned, 50, |values| {
            values.fill(7);
            Ok(())
        })
        .unwrap();
    arena.begin_frame();
    arena.pin(pinned);
    assert!(matches!(
        arena.resolve(incoming, 75, |_| Ok(())),
        Err(PathBaselineError::CacheBudget { .. })
    ));
    assert_eq!(arena.values.len(), 50);
    assert_eq!(arena.get(pinned).unwrap(), &[7; 50]);
    arena
        .resolve(incoming, 50, |values| {
            values.fill(9);
            Ok(())
        })
        .unwrap();
    assert_eq!(arena.get(incoming).unwrap(), &[9; 50]);
    assert_eq!(arena.get(pinned).unwrap(), &[7; 50]);
}

#[test]
fn failed_placement_writer_never_publishes_partial_frames() {
    let (_, handle, store, path) = placement_fixture();
    let key = placement_key(&store, path, handle);
    let mut arena = FrameArena::<u32>::new(4096, 4);
    assert_eq!(
        arena.resolve(key, 2, |values| {
            values[0] = 99;
            Err(PathBaselineError::Unavailable)
        }),
        Err(PathBaselineError::Unavailable)
    );
    assert!(arena.get(key).is_none());
    assert!(arena.entries.is_empty());
    assert!(arena.values.is_empty());
    arena
        .resolve(key, 2, |values| {
            values.fill(5);
            Ok(())
        })
        .unwrap();
    assert_eq!(arena.get(key).unwrap(), &[5, 5]);
}

#[test]
fn measured_cache_rejects_an_entry_larger_than_its_budget() {
    let path = Path::from_owned(alloc::vec![
        PathCmd::MoveTo(point(0, 0)),
        PathCmd::QuadTo {
            ctrl: point(50, 100),
            end: point(100, 0),
        },
    ]);
    let mut cache = PathBaselineCache::new(1, 1);
    let key = MeasurementKey {
        path: path_id(path.clone()),
        revision: None,
        subpath: 0,
        tolerance: DEFAULT_TOLERANCE,
    };
    assert!(matches!(
        cache.resolve(key, &path),
        Err(PathBaselineError::CacheBudget { budget: 1, .. })
    ));
    assert!(cache.entries.is_empty());
    assert!(cache.segments.is_empty());
}

#[test]
fn placement_cache_pairs_layout_glyphs_with_path_frames() {
    let face = SimpleTypeface::new(&Mono);
    let mut layouts = TextLayoutCache::default();
    let handle = layouts
        .layout(
            TextLayoutRequest {
                text: "ab",
                max_width: 100 << 8,
                width: Some(100 << 8),
                max_lines: 1,
                line_height: 256,
                baseline: 192,
                direction: BaseDirection::LeftToRight,
                wrap: WrapMode::NoWrap,
                alignment: Alignment::Start,
                overflow: Overflow::Clip,
                spacing: TextSpacing::default(),
                features: &[],
                line_widths: None,
            },
            &[&face],
        )
        .unwrap();
    let layout = layouts.get(handle).unwrap();
    let mut store = PathStore::new(1).unwrap();
    let path = store
        .insert(Path::from_owned(alloc::vec![
            PathCmd::MoveTo(point(10, 20)),
            PathCmd::LineTo(point(110, 20)),
        ]))
        .unwrap();
    let mut runtime = PathTextRuntime::default();
    let text_path = TextPath::new(path);

    let key = placement_key(&store, text_path, handle);
    runtime.prepare_glyph_frames(&store, key, &layout).unwrap();
    runtime
        .glyph_frames(key, |frames| {
            assert_eq!(frames.len(), 2);
            assert_eq!(
                frames[0].local_origin,
                FlowPoint {
                    x: 10 << 8,
                    y: 20 << 8
                }
            );
            assert_eq!(frames[0].unit_tangent, FlowPoint { x: 256, y: 0 });
            assert_eq!(
                frames[1].local_origin,
                FlowPoint {
                    x: 11 << 8,
                    y: 20 << 8
                }
            );
        })
        .unwrap();

    assert_eq!(runtime.placements.glyphs.entries.len(), 1);
    assert!(runtime.placements.carets.entries.is_empty());
    assert!(runtime.placements.carets.values.is_empty());

    runtime.prepare_caret_frames(&store, key, &layout).unwrap();
    runtime
        .caret_frames(key, |frames| {
            assert_eq!(frames.len(), 3);
            assert_eq!(frames[0].local_origin.x, 10 << 8);
            assert_eq!(frames[1].local_origin.x, 11 << 8);
            assert_eq!(frames[2].local_origin.x, 12 << 8);
            assert!(
                frames
                    .iter()
                    .all(|frame| frame.unit_tangent == FlowPoint { x: 256, y: 0 })
            );
        })
        .unwrap();

    assert_eq!(runtime.placements.glyphs.entries.len(), 1);
    assert_eq!(runtime.placements.carets.entries.len(), 1);
}

#[test]
fn consecutive_subpaths_constrain_and_place_independent_lines() {
    let face = SimpleTypeface::new(&Mono);
    let mut layouts = TextLayoutCache::default();
    let mut store = PathStore::new(1).unwrap();
    let path = store
        .insert(Path::from_owned(alloc::vec![
            PathCmd::MoveTo(point(10, 20)),
            PathCmd::LineTo(point(12, 20)),
            PathCmd::MoveTo(point(30, 40)),
            PathCmd::LineTo(point(31, 40)),
        ]))
        .unwrap();
    let mut runtime = PathTextRuntime::default();
    let text_path = TextPath::new(path);
    let handle = runtime
        .with_lines(&store, text_path, DEFAULT_TOLERANCE, 2, |widths| {
            layouts.layout(
                TextLayoutRequest {
                    text: "abc",
                    max_width: i32::MAX,
                    width: None,
                    max_lines: 2,
                    line_height: 256,
                    baseline: 192,
                    direction: BaseDirection::LeftToRight,
                    wrap: WrapMode::Grapheme,
                    alignment: Alignment::Start,
                    overflow: Overflow::Clip,
                    spacing: TextSpacing::default(),
                    features: &[],
                    line_widths: Some(widths),
                },
                &[&face],
            )
        })
        .unwrap()
        .unwrap();
    let layout = layouts.get(handle).unwrap();
    assert_eq!(layout.lines().len(), 2);
    assert_eq!(layout.lines()[0].advance(), 2 << 8);
    assert_eq!(layout.lines()[1].advance(), 1 << 8);

    let key = placement_key(&store, text_path, handle);
    runtime.prepare_glyph_frames(&store, key, &layout).unwrap();
    runtime
        .glyph_frames(key, |frames| {
            assert_eq!(frames.len(), 3);
            assert_eq!(
                frames[0].local_origin,
                FlowPoint {
                    x: 10 << 8,
                    y: 20 << 8
                }
            );
            assert_eq!(
                frames[1].local_origin,
                FlowPoint {
                    x: 11 << 8,
                    y: 20 << 8
                }
            );
            assert_eq!(
                frames[2].local_origin,
                FlowPoint {
                    x: 30 << 8,
                    y: 40 << 8
                }
            );
        })
        .unwrap();
}

#[test]
fn available_subpaths_bound_ellipsis_to_the_last_path_line() {
    let face = SimpleTypeface::new(&Mono);
    let mut layouts = TextLayoutCache::default();
    let mut store = PathStore::new(1).unwrap();
    let path = store
        .insert(Path::from_owned(alloc::vec![
            PathCmd::MoveTo(point(10, 20)),
            PathCmd::LineTo(point(13, 20)),
        ]))
        .unwrap();
    let mut runtime = PathTextRuntime::default();
    let text_path = TextPath::new(path);
    let handle = runtime
        .with_lines(&store, text_path, DEFAULT_TOLERANCE, 2, |widths| {
            layouts.layout(
                TextLayoutRequest {
                    text: "ab cd",
                    max_width: i32::MAX,
                    width: None,
                    max_lines: widths.line_count(),
                    line_height: 256,
                    baseline: 192,
                    direction: BaseDirection::LeftToRight,
                    wrap: WrapMode::Word,
                    alignment: Alignment::Start,
                    overflow: Overflow::Ellipsis,
                    spacing: TextSpacing::default(),
                    features: &[],
                    line_widths: Some(widths),
                },
                &[&face],
            )
        })
        .unwrap()
        .unwrap();
    let layout = layouts.get(handle).unwrap();

    assert_eq!(layout.lines().len(), 1);
    assert_eq!(layout.glyphs().len(), 3);
    assert_eq!(
        layout.glyphs()[2].glyph_id(),
        GlyphId::new('\u{2026}' as u16)
    );
    let key = placement_key(&store, text_path, handle);
    runtime.prepare_glyph_frames(&store, key, &layout).unwrap();
    runtime
        .glyph_frames(key, |frames| assert_eq!(frames.len(), 3))
        .unwrap();
}

#[test]
fn rtl_ellipsis_occupies_the_visual_start_of_the_path() {
    let face = SimpleTypeface::new(&Mono);
    let mut layouts = TextLayoutCache::default();
    let mut store = PathStore::new(1).unwrap();
    let path = store
        .insert(Path::from_owned(alloc::vec![
            PathCmd::MoveTo(point(10, 20)),
            PathCmd::LineTo(point(13, 20)),
        ]))
        .unwrap();
    let mut runtime = PathTextRuntime::default();
    let text_path = TextPath::new(path);
    let handle = runtime
        .with_lines(&store, text_path, DEFAULT_TOLERANCE, 1, |widths| {
            layouts.layout(
                TextLayoutRequest {
                    text: "ab cd",
                    max_width: i32::MAX,
                    width: None,
                    max_lines: widths.line_count(),
                    line_height: 256,
                    baseline: 192,
                    direction: BaseDirection::RightToLeft,
                    wrap: WrapMode::Word,
                    alignment: Alignment::Start,
                    overflow: Overflow::Ellipsis,
                    spacing: TextSpacing::default(),
                    features: &[],
                    line_widths: Some(widths),
                },
                &[&face],
            )
        })
        .unwrap()
        .unwrap();
    let layout = layouts.get(handle).unwrap();

    assert_eq!(
        layout.glyphs()[0].glyph_id(),
        GlyphId::new('\u{2026}' as u16)
    );
    let key = placement_key(&store, text_path, handle);
    runtime.prepare_glyph_frames(&store, key, &layout).unwrap();
    runtime
        .glyph_frames(key, |frames| {
            assert_eq!(frames[0].local_origin.x, 10 << 8);
            assert!(
                frames
                    .windows(2)
                    .all(|pair| pair[0].local_origin.x <= pair[1].local_origin.x)
            );
        })
        .unwrap();
}
