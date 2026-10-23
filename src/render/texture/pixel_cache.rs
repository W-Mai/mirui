use alloc::vec::Vec;

use super::access::CachedPixels;
use super::{Region, TexturePixels};
use crate::core::cache::Handle;
use crate::core::cache::handle::{CacheEntry, EntryRc};
use crate::render::budget::{Allocation, MemoryBudget, RetainedAllocation, RetainedReservation};
use crate::render::{PlaneLayout, RenderError, RenderResource};

/// Immutable published pixels and reusable exclusive storage share one retained allowance.
pub(super) struct PixelCache {
    pixels: Option<Handle<CachedPixels>>,
    reservation: RetainedReservation,
    revision: Option<u64>,
    region: Region,
    layout: PlaneLayout,
}

fn capacity_error() -> RenderError {
    RenderError::ResourceLimit(RenderResource::Texture)
}

pub(super) enum PixelAllocation {
    Retained(RetainedAllocation),
    Shared(Allocation),
}

impl PixelAllocation {
    pub(super) fn bytes(&self) -> usize {
        match self {
            Self::Retained(allocation) => allocation.bytes(),
            Self::Shared(allocation) => allocation.bytes(),
        }
    }
}

impl crate::core::cache::HasSize for CachedPixels {
    fn cache_size(&self) -> usize {
        self._allocation.bytes()
    }
}

impl CachedPixels {
    pub(super) fn shared(
        layout: PlaneLayout,
        budget: &MemoryBudget,
    ) -> Result<Handle<Self>, RenderError> {
        let charged = layout
            .required_bytes()
            .checked_add(Self::OWNER_BYTES)
            .ok_or_else(capacity_error)?;
        let reservation = budget.reserve(charged).ok_or_else(capacity_error)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(layout.required_bytes())
            .map_err(|_| capacity_error())?;
        let actual = bytes
            .capacity()
            .checked_add(Self::OWNER_BYTES)
            .ok_or_else(capacity_error)?;
        let allocation = reservation.commit(actual).map_err(|_| capacity_error())?;
        bytes.resize(layout.required_bytes(), 0);
        Ok(Handle::from_rc(EntryRc::new(CacheEntry::new(Self {
            bytes,
            _allocation: PixelAllocation::Shared(allocation),
        }))))
    }
}

impl PixelCache {
    pub(super) fn new(reservation: RetainedReservation, layout: PlaneLayout) -> Self {
        Self {
            pixels: None,
            reservation,
            revision: None,
            region: Region::new(0, 0, u32::from(layout.width()), u32::from(layout.height()))
                .expect("u16 plane dimensions fit a region"),
            layout,
        }
    }

    pub(super) fn contains(&self, revision: u64, region: Region) -> bool {
        self.revision == Some(revision)
            && region.x() >= self.region.x()
            && region.y() >= self.region.y()
            && region.right() <= self.region.right()
            && region.bottom() <= self.region.bottom()
    }

    pub(super) fn view(
        &self,
        revision: u64,
        region: Region,
    ) -> Result<Option<TexturePixels<'static>>, RenderError> {
        if !self.contains(revision, region) {
            return Ok(None);
        }
        let local = Region::new(
            region.x() - self.region.x(),
            region.y() - self.region.y(),
            region.width(),
            region.height(),
        )
        .map_err(|_| RenderError::InvalidTexture)?;
        TexturePixels::cached(
            self.pixels
                .as_ref()
                .ok_or(RenderError::InvalidTexture)?
                .clone(),
            self.layout,
        )
        .into_region(local)
        .map(Some)
    }

    pub(super) fn invalidate(&mut self) {
        self.revision = None;
    }

    pub(super) fn retag(&mut self, previous: u64, revision: u64) {
        if self.revision == Some(previous) {
            self.revision = Some(revision);
        }
    }

    pub(super) fn publish(&mut self, revision: u64, region: Region) -> Result<(), RenderError> {
        if region.width() != u32::from(self.layout.width())
            || region.height() != u32::from(self.layout.height())
            || self.pixels.is_none()
        {
            return Err(RenderError::InvalidTexture);
        }
        self.region = region;
        self.revision = Some(revision);
        Ok(())
    }

    pub(super) fn prepare(&mut self, layout: PlaneLayout) -> Result<(), RenderError> {
        if let Some(pixels) = self.pixels.as_mut()
            && let Some(entry) = EntryRc::get_mut(&mut pixels.inner)
        {
            if entry.payload.bytes.capacity() >= layout.required_bytes() {
                entry.payload.bytes.resize(layout.required_bytes(), 0);
                self.revision = None;
                self.layout = layout;
                return Ok(());
            }
            self.pixels = None;
            self.revision = None;
        }
        let charged = layout
            .required_bytes()
            .checked_add(CachedPixels::OWNER_BYTES)
            .ok_or_else(capacity_error)?;
        if charged > self.reservation.capacity() {
            return Err(capacity_error());
        }
        let mut allocation = self
            .reservation
            .allocate(charged)
            .ok_or_else(capacity_error)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(layout.required_bytes())
            .map_err(|_| capacity_error())?;
        if !allocation.resize(
            bytes
                .capacity()
                .checked_add(CachedPixels::OWNER_BYTES)
                .ok_or_else(capacity_error)?,
        ) {
            return Err(capacity_error());
        }
        bytes.resize(layout.required_bytes(), 0);
        self.pixels = Some(Handle::from_rc(EntryRc::new(CacheEntry::new(
            CachedPixels {
                bytes,
                _allocation: PixelAllocation::Retained(allocation),
            },
        ))));
        self.revision = None;
        self.layout = layout;
        Ok(())
    }

    pub(super) fn bytes_mut(&mut self) -> Result<&mut [u8], RenderError> {
        let pixels = self.pixels.as_mut().ok_or(RenderError::InvalidTexture)?;
        let entry = EntryRc::get_mut(&mut pixels.inner).ok_or(RenderError::InvalidScope)?;
        self.revision = None;
        Ok(&mut entry.payload.bytes)
    }
}

#[cfg(test)]
mod tests {
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
}
