use crate::render::renderer::{RenderError, RenderResource};
use crate::types::{Fixed, Fixed64};

/// Convert a logical scratch extent to physical pixels and retain one pixel
/// for fractional glyph phase coverage.
pub(super) fn physical_raster_scratch_extent(
    logical: u16,
    scale: Fixed,
) -> Result<u16, RenderError> {
    let scaled = Fixed64::from_int(i64::from(logical)).mul_wide(Fixed64::from_fixed(scale));
    let rounded = (-scaled)
        .to_int()
        .checked_neg()
        .and_then(|value| value.checked_add(1))
        .and_then(|value| u16::try_from(value).ok())
        .ok_or(RenderError::ResourceLimit(RenderResource::Target))?;
    Ok(rounded)
}

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

/// Inputs that determine whether a failed pool replacement can be retried unchanged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ScratchPrepareRequest {
    pub width: u16,
    pub height: u16,
    pub slots: usize,
    pub previous: Option<RasterScratchPlan>,
}

// Retry at least once every 65 preparation calls while a request remains unchanged.
const MAX_TRANSIENT_RETRY_SKIP_CALLS: u8 = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ScratchPrepareFailure {
    request: ScratchPrepareRequest,
    error: RenderError,
    retry: Option<TransientRetry>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TransientRetry {
    skip_calls: u8,
    calls_remaining: u8,
}

/// One-entry failure gate; it retains no browser objects or heap storage.
#[derive(Default)]
pub(super) struct ScratchPrepareRetry {
    failure: Option<ScratchPrepareFailure>,
}

impl ScratchPrepareRetry {
    pub fn before_attempt(&mut self, request: ScratchPrepareRequest) -> Result<(), RenderError> {
        let Some(failure) = self.failure.as_mut() else {
            return Ok(());
        };
        if failure.request != request {
            self.failure = None;
            return Ok(());
        }
        if let Some(retry) = failure.retry.as_mut() {
            if retry.calls_remaining == 0 {
                return Ok(());
            }
            retry.calls_remaining -= 1;
        }
        Err(failure.error)
    }

    pub fn deterministic_failure(&mut self, request: ScratchPrepareRequest, error: RenderError) {
        self.failure = Some(ScratchPrepareFailure {
            request,
            error,
            retry: None,
        });
    }

    pub fn transient_failure(&mut self, request: ScratchPrepareRequest, error: RenderError) {
        let skip_calls = self
            .failure
            .as_ref()
            .filter(|failure| failure.request == request)
            .and_then(|failure| failure.retry)
            .map_or(1, |retry| {
                retry
                    .skip_calls
                    .saturating_mul(2)
                    .min(MAX_TRANSIENT_RETRY_SKIP_CALLS)
            });
        self.failure = Some(ScratchPrepareFailure {
            request,
            error,
            retry: Some(TransientRetry {
                skip_calls,
                calls_remaining: skip_calls,
            }),
        });
    }

    pub fn clear(&mut self) {
        self.failure = None;
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/render/backends/web_raster_budget.rs"]
mod tests;
