use alloc::vec::Vec;
use core::{mem::size_of, ops::Range};

use sdl2_sys::SDL_Vertex;
use textflow::shaping::{GlyphId, PositionedGlyph};

use super::{COVERAGE_ENTRY_OVERHEAD, LabelCache, MAX_BATCH_INDICES, RUN_BUDGET, scalar_admitted};
use crate::core::cache::HasSize;
use crate::render::font::scalar::ScalarSurfaceKey;
use crate::render::font::{Font, RasterQuad, raster_glyph_origin, raster_glyph_rect};
use crate::render::{RenderError, RenderResource};
use crate::types::{Color, Fixed, Point, Rect, Transform, Viewport};

pub(super) struct Batch {
    pub key: ScalarSurfaceKey,
    pub glyph: GlyphId,
    pub vertices: Range<usize>,
    pub indices: Range<usize>,
}

pub(super) struct CoverageRun {
    signature: Vec<PositionedGlyph>,
    pub offset: Point,
    pub vertices: Vec<SDL_Vertex>,
    pub indices: Vec<i32>,
    pub batches: Vec<Batch>,
}

impl HasSize for CoverageRun {
    fn cache_size(&self) -> usize {
        size_of::<Self>()
            + self.signature.capacity() * size_of::<PositionedGlyph>()
            + self.vertices.capacity() * size_of::<SDL_Vertex>()
            + self.indices.capacity() * size_of::<i32>()
            + self.batches.capacity() * size_of::<Batch>()
    }
}

impl CoverageRun {
    pub fn matches(&self, glyphs: &[PositionedGlyph]) -> bool {
        self.signature == glyphs
    }

    pub fn prepare(
        glyphs: &[PositionedGlyph],
        font: &Font,
        scale: Fixed,
        device_limit: (u32, u32),
    ) -> Result<Option<Self>, RenderError> {
        let Some(first) = glyphs.first().filter(|_| scale > Fixed::ZERO) else {
            return Ok(None);
        };
        let requested = font.size.max(1);
        let output = crate::render::font::output_ppem(requested, scale);
        let rasters = || {
            glyphs.iter().filter_map(|glyph| {
                let raster = font.raster_for_output(glyph.glyph_id(), requested, output)?;
                let region = raster.region.filter(|r| r.width() > 0 && r.height() > 0)?;
                Some((glyph, raster, region))
            })
        };
        let mut count = 0usize;
        let mut batch_count = 0usize;
        let mut in_batch = 0usize;
        let mut active = None;
        for (_, raster, _) in rasters() {
            if !scalar_admitted(raster, device_limit) {
                return Ok(None);
            }
            let key = ScalarSurfaceKey::new(font.face_id(), font.revision(), raster.surface);
            if active != Some(key) || (in_batch + 1) * 54 > MAX_BATCH_INDICES {
                active = Some(key);
                in_batch = 0;
                batch_count += 1;
            }
            in_batch += 1;
            count += 1;
        }
        let Some(bytes) = count
            .checked_mul(16 * size_of::<SDL_Vertex>() + 54 * size_of::<i32>())
            .and_then(|n| n.checked_add(glyphs.len().checked_mul(size_of::<PositionedGlyph>())?))
            .and_then(|n| n.checked_add(batch_count.checked_mul(size_of::<Batch>())?))
            .and_then(|n| n.checked_add(size_of::<Self>() + COVERAGE_ENTRY_OVERHEAD))
        else {
            return Ok(None);
        };
        if bytes > RUN_BUDGET {
            return Ok(None);
        }
        let Some(bounds) = font.raster_run_bounds(glyphs, output, scale) else {
            return Ok(None);
        };
        let mut run = Self {
            signature: Vec::new(),
            offset: bounds.offset,
            vertices: Vec::new(),
            indices: Vec::new(),
            batches: Vec::new(),
        };
        let capacity_error = |_| RenderError::ResourceLimit(RenderResource::Texture);
        run.signature
            .try_reserve_exact(glyphs.len())
            .map_err(capacity_error)?;
        run.vertices
            .try_reserve_exact(count * 16)
            .map_err(capacity_error)?;
        run.indices
            .try_reserve_exact(count * 54)
            .map_err(capacity_error)?;
        run.batches
            .try_reserve_exact(batch_count)
            .map_err(capacity_error)?;
        if run.cache_size() + COVERAGE_ENTRY_OVERHEAD > RUN_BUDGET {
            return Ok(None);
        }
        run.signature.extend_from_slice(glyphs);
        let ascender = font.metrics(requested).ascender;
        for (glyph, raster, region) in rasters() {
            let Some(origin) = raster_glyph_origin(
                Point {
                    x: -bounds.offset.x * scale,
                    y: -bounds.offset.y * scale,
                },
                first.origin,
                glyph,
                ascender,
                scale,
            ) else {
                continue;
            };
            let (x, y, width, height) = raster_glyph_rect(
                origin,
                Point {
                    x: raster.offset_x,
                    y: raster.offset_y,
                },
                region,
                requested,
                raster.representation.design_ppem(),
                scale,
            );
            let key = ScalarSurfaceKey::new(font.face_id(), font.revision(), raster.surface);
            if run.batches.last().is_none_or(|batch| {
                batch.key != key || batch.indices.len() + 54 > MAX_BATCH_INDICES
            }) {
                run.batches.push(Batch {
                    key,
                    glyph: glyph.glyph_id(),
                    vertices: run.vertices.len()..run.vertices.len(),
                    indices: run.indices.len()..run.indices.len(),
                });
            }
            let batch = run.batches.last_mut().expect("prepared glyph batch");
            let quad = RasterQuad {
                rect: Rect::new(x, y, width, height),
                transform: Transform::IDENTITY,
            };
            if let Some((vertices, indices)) = LabelCache::scalar_glyph_geometry(
                (run.vertices.len() - batch.vertices.start) as i32,
                quad,
                region,
                raster.surface,
                Color::rgb(255, 255, 255),
                255,
                Viewport::new(1, 1, Fixed::ONE),
            ) {
                run.vertices.extend_from_slice(&vertices);
                run.indices.extend_from_slice(&indices);
                batch.vertices.end = run.vertices.len();
                batch.indices.end = run.indices.len();
            }
        }
        Ok(Some(run))
    }
}

#[cfg(test)]
mod tests {
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
}
