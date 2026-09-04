use super::{EncodedImageError, EncodedImageView, groups::GroupSource};
use crate::{
    PayloadLimits,
    image::{
        CoverageBudget, SurfaceRequirements, UnitDecodeError, UnitGroup, units::ScalarProfile,
    },
};

#[cfg(test)]
mod tests;

pub(super) struct Preflight<'a> {
    limits: &'a PayloadLimits,
    budget: CoverageBudget,
    total_units: u64,
}

impl<'a> Preflight<'a> {
    pub(super) fn new(limits: &'a PayloadLimits, groups: usize) -> Result<Self, EncodedImageError> {
        if groups > limits.max_image_groups() as usize {
            return Err(EncodedImageError::TooManyGroups {
                limit: limits.max_image_groups(),
                actual: groups,
            });
        }
        Ok(Self {
            limits,
            budget: CoverageBudget::new(limits.max_image_work()),
            total_units: 0,
        })
    }

    pub(super) fn spend(&mut self, work: u64) -> Result<(), EncodedImageError> {
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
        self.spend(source.data.len() as u64)
    }

    pub(super) fn total_units(&self) -> u64 {
        self.total_units
    }

    pub(super) fn work(&self) -> u64 {
        self.limits.max_image_work() - self.budget.remaining()
    }

    pub(super) fn group(
        &mut self,
        index: usize,
        group: UnitGroup<'_>,
    ) -> Result<(), EncodedImageError> {
        self.total_units = self
            .total_units
            .checked_add(group.len() as u64)
            .ok_or(EncodedImageError::SizeOverflow)?;
        if self.total_units > u64::from(self.limits.max_image_units()) {
            return Err(EncodedImageError::TooManyUnits {
                limit: self.limits.max_image_units(),
                actual: self.total_units,
            });
        }
        let profile = ScalarProfile::new(group.coding(), group.surface().sample_layout()).map_err(
            |error| EncodedImageError::Coding {
                group: index,
                error,
            },
        )?;
        for (ordinal, unit) in group.iter().enumerate() {
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
        }
        Ok(())
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
        let mut preflight = Preflight::new(limits, self.group_count())?;
        preflight.groups(self.group_source())?;
        self.validate_data()
    }
}
