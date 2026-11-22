use alloc::vec::Vec;

use super::access::CachedPixels;
use super::{Region, TexturePixels};
use crate::core::cache::Handle;
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
        Self::shared_bytes(layout.required_bytes(), budget)
    }

    pub(super) fn shared_bytes(
        size: usize,
        budget: &MemoryBudget,
    ) -> Result<Handle<Self>, RenderError> {
        let charged = size
            .checked_add(Self::OWNER_BYTES)
            .ok_or_else(capacity_error)?;
        let reservation = budget.reserve(charged).ok_or_else(capacity_error)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(size)
            .map_err(|_| capacity_error())?;
        let actual = bytes
            .capacity()
            .checked_add(Self::OWNER_BYTES)
            .ok_or_else(capacity_error)?;
        let allocation = reservation.commit(actual).map_err(|_| capacity_error())?;
        bytes.resize(size, 0);
        Ok(Handle::new(Self {
            bytes,
            _allocation: PixelAllocation::Shared(allocation),
        }))
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
            && let Some(entry) = pixels.get_mut()
        {
            if entry.bytes.capacity() >= layout.required_bytes() {
                entry.bytes.resize(layout.required_bytes(), 0);
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
        self.pixels = Some(Handle::new(CachedPixels {
            bytes,
            _allocation: PixelAllocation::Retained(allocation),
        }));
        self.revision = None;
        self.layout = layout;
        Ok(())
    }

    pub(super) fn bytes_mut(&mut self) -> Result<&mut [u8], RenderError> {
        let pixels = self.pixels.as_mut().ok_or(RenderError::InvalidTexture)?;
        let entry = pixels.get_mut().ok_or(RenderError::InvalidScope)?;
        self.revision = None;
        Ok(&mut entry.bytes)
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/render/texture/pixel_cache.rs"]
mod tests;
