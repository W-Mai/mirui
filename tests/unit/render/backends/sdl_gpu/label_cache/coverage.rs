use super::*;
use textflow::shaping::FlowPoint;

#[test]
fn retained_geometry_checks_full_signature_and_keeps_ordered_bounded_batches() {
    let font = Font::default();
    let glyph = PositionedGlyph::new(font.map_char('A').unwrap(), FlowPoint { x: 0, y: 0 });
    let mut glyphs = alloc::vec![glyph; 500];
    let run = CoverageRun::prepare(&glyphs, &font, Fixed::ONE, (4096, 4096))
        .unwrap()
        .unwrap();
    assert!(run.matches(&glyphs));
    assert_eq!(run.batches.len(), 3);
    assert!(run.cache_size() <= RUN_BUDGET);
    for batch in &run.batches {
        assert!(batch.indices.len() <= MAX_BATCH_INDICES);
        assert!(
            run.indices[batch.indices.clone()]
                .iter()
                .all(|i| *i >= 0 && (*i as usize) < batch.vertices.len())
        );
    }
    glyphs[499] = glyph.with_offset(FlowPoint { x: 1, y: 0 });
    assert!(!run.matches(&glyphs));
}

#[test]
fn admission_rejects_capacity_and_unsupported_surfaces_before_allocating() {
    let font = Font::default();
    let glyph = PositionedGlyph::new(font.map_char('A').unwrap(), FlowPoint { x: 0, y: 0 });
    let oversized = alloc::vec![glyph; 10_000];
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            assert!(
                CoverageRun::prepare(&oversized, &font, Fixed::ONE, (4096, 4096))
                    .unwrap()
                    .is_none()
            );
            assert!(
                CoverageRun::prepare(&[glyph], &font, Fixed::ONE, (1, 1))
                    .unwrap()
                    .is_none()
            );
        }),
        0
    );
}

#[test]
fn geometry_hits_keep_storage_and_reject_signature_collisions_without_allocating() {
    use super::super::{CachedRun, RunKey};
    use crate::core::cache::{CacheInspect, LruCache, MaxSize};

    let font = Font::default();
    let glyphs = [PositionedGlyph::new(
        font.map_char('A').unwrap(),
        FlowPoint { x: 0, y: 0 },
    )];
    let run = CoverageRun::prepare(&glyphs, &font, Fixed::ONE, (4096, 4096))
        .unwrap()
        .unwrap();
    let bytes = run.cache_size() + COVERAGE_ENTRY_OVERHEAD;
    let mut cache = LruCache::builder().max_size(MaxSize::Bytes(bytes)).build();
    let key =
        RunKey::Coverage(font.raster_run_key(&glyphs, &Color::rgb(255, 255, 255), Fixed::ONE));
    drop(cache.entry(key).or_insert_with(|| CachedRun::Coverage(run)));
    let changed = [glyphs[0].with_offset(FlowPoint { x: 1, y: 0 })];
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            for _ in 0..32 {
                let entry = cache.acquire(&key).unwrap();
                let CachedRun::Coverage(run) = &*entry else {
                    unreachable!()
                };
                assert!(run.matches(&glyphs));
                assert!(!run.matches(&changed));
                assert_eq!(entry.cache_size(), bytes);
            }
        }),
        0
    );
    assert_eq!(cache.cache_len(), 1);
    assert_eq!(cache.cache_stats().insert_count, 1);
}
