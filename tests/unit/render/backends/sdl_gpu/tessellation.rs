use super::*;
use crate::types::Fixed;

#[test]
fn primitive_and_dashed_cache_hits_skip_construction_workspaces() {
    use crate::render::path::{rounded_quad_commands, rounded_rect_commands};
    use crate::render::raster::{LineCap, LineJoin};
    let quad = [
        Point::new(0, 0),
        Point::new(80, 0),
        Point::new(80, 40),
        Point::new(0, 40),
    ];
    let mut cache = TessellationCache::new();
    let mut scratch = StrokeScratch::new();
    let spec = StrokeSpec {
        width: 2.into(),
        cap: LineCap::Round,
        join: LineJoin::Round,
        miter_limit: 4.into(),
        dash: &[5.into(), 3.into()],
        dash_scale: Fixed::ONE,
    };
    let color = Color::rgb(30, 80, 120);
    let mut draw = |scratch: &mut StrokeScratch| {
        let rect = rounded_rect_commands(0.into(), 0.into(), 80.into(), 40.into(), 6.into());
        cache
            .fill(rect.commands(), None, &color, 255, FillRule::NonZero)
            .unwrap();
        let rounded = rounded_quad_commands(&quad, 6.into());
        cache
            .fill(rounded.commands(), None, &color, 255, FillRule::NonZero)
            .unwrap();
        cache
            .dashed_stroke(rounded.commands(), None, spec, scratch, &color, 255)
            .unwrap();
    };
    draw(&mut scratch);
    // A fresh scratch has no flattened paths or stroke buffers to reuse.
    let mut untouched = StrokeScratch::new();
    assert_eq!(
        crate::test_support::tracked_allocations(|| draw(&mut untouched)),
        0
    );
    let mut uncached = TessellationCache::new();
    uncached.geometry = GeometryCache::new(0);
    let rounded = rounded_quad_commands(&quad, 6.into());
    uncached
        .dashed_stroke(rounded.commands(), None, spec, &mut scratch, &color, 255)
        .unwrap();
    assert_eq!(cache.indices, uncached.indices);
    assert_eq!(cache.verts.len(), uncached.verts.len());
    for (cached, direct) in cache.verts.iter().zip(&uncached.verts) {
        assert_eq!(
            (cached.position.x, cached.position.y),
            (direct.position.x, direct.position.y)
        );
    }
}

#[test]
fn translated_recolored_mesh_hits_without_allocation_or_tessellation() {
    let path = Path::rounded_rect(0.into(), 0.into(), 40.into(), 20.into(), 3.into());
    let mut cache = TessellationCache::new();
    cache
        .fill(
            path.commands(),
            None,
            &Color::rgb(100, 20, 30),
            255,
            FillRule::NonZero,
        )
        .unwrap();
    let positions: Vec<_> = cache
        .verts
        .iter()
        .map(|v| (v.position.x, v.position.y))
        .collect();
    let translate = Transform {
        tx: Fixed::from_ratio(13, 2),
        ty: Fixed::from_int(-7),
        ..Transform::IDENTITY
    };
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            cache
                .fill(
                    path.commands(),
                    Some(&translate),
                    &Color::rgb(10, 80, 30),
                    128,
                    FillRule::NonZero,
                )
                .unwrap();
        }),
        0
    );
    assert_eq!(cache.geometry.inspect().cache_stats().hit_count, 1);
    for (vertex, original) in cache.verts.iter().zip(positions) {
        assert_eq!(vertex.position.x, original.0 + 6.5);
        assert_eq!(vertex.position.y, original.1 - 7.0);
        assert_eq!(
            (vertex.color.r, vertex.color.g, vertex.color.a),
            (10, 80, 128)
        );
    }
    let mut uncached = TessellationCache::new();
    uncached.geometry = GeometryCache::new(0);
    uncached
        .fill(
            path.commands(),
            Some(&translate),
            &Color::rgb(10, 80, 30),
            128,
            FillRule::NonZero,
        )
        .unwrap();
    assert_eq!(cache.indices, uncached.indices);
    for (a, b) in cache.verts.iter().zip(&uncached.verts) {
        assert_eq!((a.position.x, a.position.y), (b.position.x, b.position.y));
    }
    assert_eq!(uncached.geometry.inspect().cache_len(), 0);
}
