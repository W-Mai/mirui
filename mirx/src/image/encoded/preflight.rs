use super::{EncodedImageError, EncodedImageView};
use crate::{
    PayloadLimits,
    image::{CoverageBudget, SurfaceRequirements, UnitDecodeError, units::ScalarProfile},
};

#[cfg(test)]
mod tests;

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
        if self.group_count() > limits.max_image_groups() as usize {
            return Err(EncodedImageError::TooManyGroups {
                limit: limits.max_image_groups(),
                actual: self.group_count(),
            });
        }
        let mut budget = CoverageBudget::new(limits.max_image_work());
        self.validate_groups(&mut budget)?;
        let mut total = 0u64;
        for index in 0..self.group_count() {
            let record = self.record(index)?;
            budget
                .spend_many(self.resolution_cost(record))
                .map_err(EncodedImageError::Coverage)?;
            let group = self.resolve_record(index, record)?.0;
            total = total
                .checked_add(group.len() as u64)
                .ok_or(EncodedImageError::SizeOverflow)?;
            if total > u64::from(limits.max_image_units()) {
                return Err(EncodedImageError::TooManyUnits {
                    limit: limits.max_image_units(),
                    actual: total,
                });
            }
            let profile = ScalarProfile::new(group.coding(), self.surface.sample_layout())
                .map_err(|error| EncodedImageError::Coding {
                    group: index,
                    error,
                })?;
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
                if size > limits.max_decoded_bytes() {
                    return Err(EncodedImageError::DecodedUnitTooLarge {
                        group: index,
                        ordinal,
                        limit: limits.max_decoded_bytes(),
                        actual: size,
                    });
                }
                budget
                    .spend_many(size as u64 + unit.data().len() as u64)
                    .map_err(EncodedImageError::Coverage)?;
                profile.plan(unit.data(), memory).map_err(fail)?;
            }
        }
        budget
            .spend_many(self.data.bytes().len() as u64)
            .map_err(EncodedImageError::Coverage)?;
        self.validate_data()
    }
}
