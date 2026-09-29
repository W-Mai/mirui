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
mod tests {
    use super::*;

    const BUDGET: usize = 8 * 1024 * 1024;

    #[test]
    fn physical_extent_rounds_up_and_retains_phase_margin() {
        let cases = [
            (0, Fixed::ONE, 1),
            (1, Fixed::from_ratio(1, 256), 2),
            (2, Fixed::HALF, 2),
            (3, Fixed::HALF, 3),
            (17, Fixed::from_ratio(5, 4), 23),
            (u16::MAX - 1, Fixed::ONE, u16::MAX),
        ];
        for (logical, scale, expected) in cases {
            assert_eq!(physical_raster_scratch_extent(logical, scale), Ok(expected));
        }
    }

    #[test]
    fn physical_extent_rejects_values_beyond_target_limits() {
        let error = Err(RenderError::ResourceLimit(RenderResource::Target));
        assert_eq!(physical_raster_scratch_extent(u16::MAX, Fixed::ONE), error);
        assert_eq!(physical_raster_scratch_extent(u16::MAX, Fixed::MAX), error);
    }

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

    fn request(width: u16, slots: usize) -> ScratchPrepareRequest {
        ScratchPrepareRequest {
            width,
            height: 16,
            slots,
            previous: None,
        }
    }

    #[test]
    fn deterministic_failure_rejects_repeats_until_request_or_pool_changes() {
        let mut retry = ScratchPrepareRetry::default();
        let initial = request(512, 10);
        let error = RenderError::ResourceLimit(RenderResource::Target);
        assert_eq!(retry.before_attempt(initial), Ok(()));
        retry.deterministic_failure(initial, error);
        for _ in 0..128 {
            assert_eq!(retry.before_attempt(initial), Err(error));
        }
        let resized = request(513, 10);
        assert_eq!(retry.before_attempt(resized), Ok(()));
        retry.deterministic_failure(resized, error);
        let with_pool = ScratchPrepareRequest {
            previous: Some(RasterScratchPlan::new(256, 8, 10, 96, BUDGET).unwrap()),
            ..resized
        };
        assert_eq!(retry.before_attempt(with_pool), Ok(()));
    }

    #[test]
    fn transient_failure_retries_with_bounded_exponential_backoff() {
        let mut retry = ScratchPrepareRetry::default();
        let request = request(512, 10);
        let error = RenderError::BackendFailure;
        let mut interval = 1;
        for _ in 0..10 {
            retry.transient_failure(request, error);
            for _ in 0..interval {
                assert_eq!(retry.before_attempt(request), Err(error));
            }
            assert_eq!(retry.before_attempt(request), Ok(()));
            interval = (interval * 2).min(MAX_TRANSIENT_RETRY_SKIP_CALLS);
        }
        retry.clear();
        assert_eq!(retry.before_attempt(request), Ok(()));
    }

    #[test]
    fn changed_request_clears_transient_backoff() {
        let mut retry = ScratchPrepareRetry::default();
        let initial = request(512, 10);
        retry.transient_failure(initial, RenderError::BackendFailure);
        retry.transient_failure(initial, RenderError::BackendFailure);
        assert_eq!(retry.before_attempt(request(512, 11)), Ok(()));
        assert_eq!(retry.before_attempt(initial), Ok(()));
    }
}
