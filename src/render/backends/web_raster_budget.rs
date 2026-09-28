use crate::render::renderer::{RenderError, RenderResource};

/// Checked pixel and Rust slot-storage requests for one Web glyph scratch generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RasterScratchPlan {
    pub width: u16,
    pub height: u16,
    pub slots: usize,
    pub rgba_bytes_per_slot: usize,
    pub rgba_bytes_total: usize,
    pub rust_requested_bytes: usize,
}

impl RasterScratchPlan {
    pub fn new(
        width: u16,
        height: u16,
        slots: usize,
        slot_metadata_bytes: usize,
        generation_pixel_budget: usize,
    ) -> Result<Self, RenderError> {
        if width == 0 || height == 0 || slots == 0 {
            return Err(RenderError::InvalidGeometry);
        }
        let rgba_bytes_per_slot = usize::from(width)
            .checked_mul(usize::from(height))
            .and_then(|pixels| pixels.checked_mul(4))
            .filter(|bytes| *bytes <= u32::MAX as usize)
            .ok_or(RenderError::ResourceLimit(RenderResource::Target))?;
        let rgba_bytes_total = rgba_bytes_per_slot
            .checked_mul(slots)
            .filter(|bytes| *bytes <= generation_pixel_budget)
            .ok_or(RenderError::ResourceLimit(RenderResource::Target))?;
        let metadata_bytes = slot_metadata_bytes
            .checked_mul(slots)
            .ok_or(RenderError::ResourceLimit(RenderResource::Target))?;
        let rust_requested_bytes = rgba_bytes_total
            .checked_add(metadata_bytes)
            .ok_or(RenderError::ResourceLimit(RenderResource::Target))?;
        Ok(Self {
            width,
            height,
            slots,
            rgba_bytes_per_slot,
            rgba_bytes_total,
            rust_requested_bytes,
        })
    }

    /// Bound the overlap while a replacement is built and the old pool is retained.
    pub fn checked_resize_peak(
        self,
        previous: Option<Self>,
        owned_rust_request_budget: usize,
    ) -> Result<usize, RenderError> {
        let peak = previous
            .map_or(0, |plan| plan.rust_requested_bytes)
            .checked_add(self.rust_requested_bytes)
            .filter(|bytes| *bytes <= owned_rust_request_budget)
            .ok_or(RenderError::ResourceLimit(RenderResource::Target))?;
        Ok(peak)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUDGET: usize = 8 * 1024 * 1024;

    #[test]
    fn rejects_zero_geometry_and_checked_overflow() {
        assert_eq!(
            RasterScratchPlan::new(0, 8, 1, 64, BUDGET),
            Err(RenderError::InvalidGeometry)
        );
        assert_eq!(
            RasterScratchPlan::new(1, 1, usize::MAX, 64, BUDGET),
            Err(RenderError::ResourceLimit(RenderResource::Target))
        );
        assert_eq!(
            RasterScratchPlan::new(1, 1, 2, usize::MAX, BUDGET),
            Err(RenderError::ResourceLimit(RenderResource::Target))
        );
    }

    #[test]
    fn pixel_generation_limit_is_independent_of_slot_metadata() {
        let plan = RasterScratchPlan::new(256, 8, 10, 96, BUDGET).unwrap();
        assert_eq!(plan.rgba_bytes_per_slot, 8_192);
        assert_eq!(plan.rgba_bytes_total, 81_920);
        assert_eq!(plan.rust_requested_bytes, 82_880);
    }

    #[test]
    fn resize_peak_includes_old_and_new_rgba_and_metadata() {
        let old = RasterScratchPlan::new(256, 8, 10, 96, BUDGET).unwrap();
        let new = RasterScratchPlan::new(512, 16, 10, 96, BUDGET).unwrap();
        let peak = new.checked_resize_peak(Some(old), BUDGET).unwrap();
        assert_eq!(peak, old.rust_requested_bytes + new.rust_requested_bytes);
    }

    #[test]
    fn owned_rust_request_limit_includes_metadata_and_accepts_equality() {
        let plan = RasterScratchPlan::new(1, 1, 1, 12, BUDGET).unwrap();
        assert_eq!(plan.checked_resize_peak(Some(plan), 32), Ok(32));
        assert_eq!(
            plan.checked_resize_peak(Some(plan), 31),
            Err(RenderError::ResourceLimit(RenderResource::Target))
        );
        let metadata_heavy = RasterScratchPlan::new(1, 1, 100_000, 96, BUDGET).unwrap();
        assert!(metadata_heavy.rgba_bytes_total < BUDGET);
        assert_eq!(
            metadata_heavy.checked_resize_peak(None, BUDGET),
            Err(RenderError::ResourceLimit(RenderResource::Target))
        );
    }

    #[test]
    fn rejects_overlap_even_when_each_generation_fits() {
        let old = RasterScratchPlan::new(2048, 512, 1, 96, BUDGET).unwrap();
        let new = RasterScratchPlan::new(2048, 768, 1, 96, BUDGET).unwrap();
        assert!(old.checked_resize_peak(None, BUDGET).is_ok());
        assert!(new.checked_resize_peak(None, BUDGET).is_ok());
        assert_eq!(
            new.checked_resize_peak(Some(old), BUDGET),
            Err(RenderError::ResourceLimit(RenderResource::Target))
        );
        assert_eq!(old.width, 2048);
    }
}
