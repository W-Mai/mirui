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
        source.validate_groups(&mut self.budget)?;
        for index in 0..source.group_count() {
            let record = source.record(index)?;
            self.spend(source.resolution_cost(record))?;
            self.group(index, source.resolve_record(index, record)?.0)?;
        }
        Ok(())
    }

    pub(super) fn total_units(&self) -> u64 {
        self.total_units
    }

    pub(super) fn work(&self) -> u64 {
        self.limits.max_raster_work() - self.budget.remaining()
    }

    pub(super) fn group(
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
        self.spend(size as u64 + unit.data().len() as u64)?;
        profile.plan(unit.data(), memory).map_err(fail)?;
        Ok(memory)
    }

    pub(super) fn spend_replay(
        &mut self,
        input: usize,
        decoded: usize,
    ) -> Result<(), EncodedImageError> {
        let work = (input as u64)
            .checked_add(decoded as u64)
            .and_then(|v| v.checked_mul(2))
            .and_then(|v| v.checked_add(decoded as u64))
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
