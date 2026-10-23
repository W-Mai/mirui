use super::{EncodedImageError, EncodedImageView, groups::GroupSource};
use crate::{
    PayloadLimits,
    image::{
        CoverageBudget, DecodeUnitRef, SurfaceRequirements, UnitDecodeError, UnitGroup,
        UnitMemoryPlan, units::ScalarProfile,
    },
};

#[cfg(test)]
mod tests;

pub(crate) struct Preflight<'a> {
    limits: &'a PayloadLimits,
    budget: CoverageBudget,
    total_groups: usize,
    total_units: u64,
}

/// Bounded validation shared by source-backed IMAGE metadata and unit reads.
///
/// This retains only limits and counters, not metadata or DATA borrows. Submit each image layout once and every stored unit exactly once, and charge all DATA bytes before checksum scanning. Individual successful calls do not establish completeness or verify checksums; the caller owns traversal and immutable source identity.
pub struct ImageValidation<'a> {
    inner: Preflight<'a>,
}

impl<'a> ImageValidation<'a> {
    pub fn new(limits: &'a PayloadLimits) -> Self {
        Self {
            inner: Preflight::new(limits, 0).expect("zero groups fit every limit"),
        }
    }

    /// Checks canonical group ranges, exact coverage, supported profiles and aggregate group/unit limits without retaining the supplied layout. Input syntax and decoded-size limits are checked by `validate_unit` when bytes are available.
    pub fn validate_layout(
        &mut self,
        image: super::EncodedImageLayout<'_>,
    ) -> Result<(), EncodedImageError> {
        self.inner.add_groups(image.group_count())?;
        image.validate_groups(&mut self.inner.budget)?;
        for index in 0..image.group_count() {
            let record = image.metadata.record(index)?;
            self.inner.spend(image.metadata.resolution_cost(record))?;
            let group = image.metadata.resolve_record(index, record)?.0;
            self.inner.add_units(group.selection.len() as u64)?;
            ScalarProfile::new(group.coding, group.surface.sample_layout()).map_err(|error| {
                EncodedImageError::Coding {
                    group: index,
                    error,
                }
            })?;
        }
        Ok(())
    }

    /// Validates one complete unit using the resident reader's scalar syntax, decoded-size limit and work accounting. Group and ordinal identify diagnostics; the caller must not omit or duplicate units.
    pub fn validate_unit(
        &mut self,
        group: usize,
        ordinal: usize,
        unit: DecodeUnitRef<'_>,
    ) -> Result<UnitMemoryPlan, EncodedImageError> {
        let profile = ScalarProfile::for_unit(unit)
            .map_err(|error| EncodedImageError::Coding { group, error })?;
        self.inner.unit(group, ordinal, unit, profile)
    }

    /// Charges checksum input work. Checksum comparison belongs to the existing integrity scanners.
    pub fn charge_data(&mut self, bytes: u32) -> Result<(), EncodedImageError> {
        self.inner.spend(u64::from(bytes))
    }

    pub fn work(&self) -> u64 {
        self.inner.work()
    }
    /// Total units declared by admitted layouts, not the number of inputs validated.
    pub fn unit_count(&self) -> u64 {
        self.inner.total_units()
    }
}

impl<'a> Preflight<'a> {
    pub(crate) fn new(limits: &'a PayloadLimits, groups: usize) -> Result<Self, EncodedImageError> {
        let mut preflight = Self {
            limits,
            budget: CoverageBudget::new(limits.max_raster_work()),
            total_groups: 0,
            total_units: 0,
        };
        preflight.add_groups(groups)?;
        Ok(preflight)
    }

    pub(super) fn add_groups(&mut self, count: usize) -> Result<(), EncodedImageError> {
        let total = self
            .total_groups
            .checked_add(count)
            .ok_or(EncodedImageError::SizeOverflow)?;
        if total > self.limits.max_raster_groups() as usize {
            return Err(EncodedImageError::TooManyGroups {
                limit: self.limits.max_raster_groups(),
                actual: total,
            });
        }
        self.total_groups = total;
        Ok(())
    }

    /// Adds static coverage and scalar syntax, without resetting resource counters.
    /// DATA integrity is scheduled separately by the complete media caller.
    pub(crate) fn image(&mut self, image: EncodedImageView<'_>) -> Result<(), EncodedImageError> {
        self.add_groups(image.group_count())?;
        self.groups(image.group_source())
    }

    /// Charges a validated media payload's derived DATA size before scanning it.
    pub(crate) fn data(
        &mut self,
        media: crate::media::MediaPayload<'_>,
        byte_len: u32,
    ) -> Result<(), EncodedImageError> {
        self.spend(u64::from(byte_len))?;
        media.validate_data().map_err(EncodedImageError::Media)
    }

    pub(crate) fn spend(&mut self, work: u64) -> Result<(), EncodedImageError> {
        self.budget
            .spend_many(work)
            .map_err(EncodedImageError::Coverage)
    }

    pub(super) fn groups(&mut self, source: GroupSource<'_>) -> Result<(), EncodedImageError> {
        let metadata = source.metadata()?;
        metadata.validate_groups(&mut self.budget)?;
        for index in 0..metadata.group_count() {
            let record = metadata.record(index)?;
            self.spend(metadata.resolution_cost(record))?;
            self.group(index, source.resolve_record(index, record)?.0)?;
        }
        Ok(())
    }

    pub(crate) fn native_groups(
        &mut self,
        surface: crate::image::SurfaceDescriptor,
        groups: &[UnitGroup<'_>],
    ) -> Result<(), EncodedImageError> {
        surface
            .validate_coverage(groups, &mut self.budget)
            .map_err(EncodedImageError::Coverage)?;
        for (index, &group) in groups.iter().enumerate() {
            self.group(index, group)?;
        }
        Ok(())
    }

    pub(crate) fn total_units(&self) -> u64 {
        self.total_units
    }

    pub(crate) fn work(&self) -> u64 {
        self.limits.max_raster_work() - self.budget.remaining()
    }

    pub(crate) fn group(
        &mut self,
        index: usize,
        group: UnitGroup<'_>,
    ) -> Result<(), EncodedImageError> {
        self.add_units(group.len() as u64)?;
        let profile = ScalarProfile::new(group.coding(), group.surface().sample_layout()).map_err(
            |error| EncodedImageError::Coding {
                group: index,
                error,
            },
        )?;
        for (ordinal, unit) in group.iter().enumerate() {
            self.unit(index, ordinal, unit, profile)?;
        }
        Ok(())
    }

    pub(super) fn add_units(&mut self, count: u64) -> Result<(), EncodedImageError> {
        self.total_units = self
            .total_units
            .checked_add(count)
            .ok_or(EncodedImageError::SizeOverflow)?;
        if self.total_units > u64::from(self.limits.max_raster_units()) {
            return Err(EncodedImageError::TooManyUnits {
                limit: self.limits.max_raster_units(),
                actual: self.total_units,
            });
        }
        Ok(())
    }

    pub(super) fn unit(
        &mut self,
        index: usize,
        ordinal: usize,
        unit: DecodeUnitRef<'_>,
        profile: ScalarProfile,
    ) -> Result<UnitMemoryPlan, EncodedImageError> {
        let fail = |error| EncodedImageError::Unit {
            group: index,
            ordinal,
            error,
        };
        let memory = unit
            .memory_plan(SurfaceRequirements::new())
            .map_err(|error| fail(UnitDecodeError::Memory(error)))?;
        let size = memory.sample_byte_len();
        if size > self.limits.max_decoded_bytes() {
            return Err(EncodedImageError::DecodedUnitTooLarge {
                group: index,
                ordinal,
                limit: self.limits.max_decoded_bytes(),
                actual: size,
            });
        }
        let profile_work = profile.extra_work(memory).map_err(fail)?;
        let work = (size as u64)
            .checked_add(unit.data().len() as u64)
            .and_then(|work| work.checked_add(profile_work))
            .ok_or(EncodedImageError::SizeOverflow)?;
        self.spend(work)?;
        profile.plan(unit.data(), memory).map_err(fail)?;
        Ok(memory)
    }

    pub(crate) fn spend_replay(
        &mut self,
        input: usize,
        decoded: usize,
        profile_work: u64,
    ) -> Result<(), EncodedImageError> {
        let work = (input as u64)
            .checked_add(decoded as u64)
            .and_then(|v| v.checked_mul(2))
            .and_then(|v| v.checked_add(decoded as u64))
            .and_then(|v| {
                profile_work
                    .checked_mul(2)
                    .and_then(|extra| v.checked_add(extra))
            })
            .and_then(|v| v.checked_add(1))
            .ok_or(EncodedImageError::SizeOverflow)?;
        self.spend(work)
    }
}

impl EncodedImageView<'_> {
    /// Checks static groups, supported scalar syntax and all DATA integrity.
    ///
    /// No group table or decoded samples are allocated. The decoded-byte limit
    /// applies to one tight unit, not the whole image. Work limits cover group
    /// resolution/coverage, unit input and decoded bytes, and one DATA checksum
    /// scan. Metadata opening has already occurred before this budget starts.
    /// Target-specific stride, addresses and allocation requirements remain a
    /// separate decode-plan concern.
    pub fn preflight(self, limits: &PayloadLimits) -> Result<(), EncodedImageError> {
        let mut preflight = Preflight::new(limits, 0)?;
        preflight.image(self)?;
        preflight.data(self.media, self.checksum_bytes)
    }
}
