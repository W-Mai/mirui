use super::*;
use crate::render::budget::MemoryBudget;
use crate::render::texture::ColorFormat;

#[test]
fn publication_matches_prepared_geometry_and_keeps_pinned_versions_immutable() {
    let layout = PlaneLayout::packed(2, 2, ColorFormat::RGBA8888).unwrap();
    let charge = layout.required_bytes() + CachedPixels::OWNER_BYTES;
    let budget = MemoryBudget::new(2 * charge);
    let mut cache = PixelCache::new(budget.reserve(2 * charge).unwrap().retain(), layout);
    let first = Region::new(3, 4, 2, 2).unwrap();
    assert!(cache.publish(1, first).is_err());
    cache.prepare(layout).unwrap();
    cache.bytes_mut().unwrap().fill(0x11);
    assert!(cache.view(1, first).unwrap().is_none());
    assert!(cache.publish(1, Region::new(3, 4, 1, 2).unwrap()).is_err());
    assert!(cache.view(1, first).unwrap().is_none());
    cache.publish(1, first).unwrap();
    let pinned = cache.view(1, first).unwrap().unwrap();
    assert!(matches!(cache.bytes_mut(), Err(RenderError::InvalidScope)));
    assert!(cache.contains(1, first));
    cache.prepare(layout).unwrap();
    cache.bytes_mut().unwrap().fill(0x22);
    let second = Region::new(8, 9, 2, 2).unwrap();
    cache.publish(1, second).unwrap();
    assert_eq!(pinned.bytes(), &[0x11; 16]);
    let newest = cache.view(1, second).unwrap().unwrap();
    assert_eq!(newest.bytes(), &[0x22; 16]);
    assert!(matches!(
        cache.prepare(layout),
        Err(RenderError::ResourceLimit(RenderResource::Texture))
    ));
    assert!(
        cache.contains(1, second),
        "failed admission preserves the published version"
    );
    drop(newest);
    drop(pinned);
    let allocations = crate::test_support::tracked_allocations(|| {
        cache.prepare(layout).unwrap();
        cache.bytes_mut().unwrap().fill(0x33);
        cache.publish(2, first).unwrap();
        cache.retag(1, 9);
        assert!(cache.contains(2, first));
        cache.retag(2, 3);
        assert!(cache.view(2, first).unwrap().is_none());
        assert_eq!(cache.view(3, first).unwrap().unwrap().bytes(), &[0x33; 16]);
    });
    assert_eq!(allocations, 0);
    let held = cache.view(3, first).unwrap().unwrap();
    drop(cache);
    assert_eq!(budget.allocated(), charge);
    assert_eq!(held.bytes(), &[0x33; 16]);
    drop(held);
    assert_eq!(budget.available(), 2 * charge);
}
