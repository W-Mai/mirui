use super::DecodedUnit;
use crate::image::{SurfaceCopyError, SurfaceMemoryPlan, samples};

impl DecodedUnit<'_> {
    /// Places selected samples at their original coordinates in a whole surface.
    ///
    /// The descriptor, capacity and actual target address are checked before
    /// any write. Other samples, absent planes, padding and the output suffix
    /// are preserved, including neighbours within the same sub-byte pixel byte.
    /// The source can be a reusable caller-owned unit buffer. No allocation,
    /// color conversion or decoding is performed; this does not validate that
    /// other units have filled the complete surface.
    ///
    /// ```
    /// use mirx::{image::{ColorDescription, SampleLayout, SurfaceDescriptor,
    ///     SurfaceRequirements, UnitGroup}, media::CodingRecord};
    /// let surface = SurfaceDescriptor::new(9, 1, SampleLayout::A1, ColorDescription::NONE).unwrap();
    /// let group = UnitGroup::builder(surface, CodingRecord::RAW, &[0xe0, 0xa0, 0x40])
    ///     .with_tiles(3, 1).build().unwrap();
    /// let mut scratch = [0; 1];
    /// let decoded = group.get(1).unwrap().decode_plan(SurfaceRequirements::new())
    ///     .unwrap().decode_into(&mut scratch).unwrap();
    /// let target = surface.memory_plan(SurfaceRequirements::new().with_stride_multiple(8)).unwrap();
    /// let mut output = [0x5a; 8];
    /// decoded.copy_into(&mut output, target).unwrap();
    /// assert_eq!(output[0], 0b0101_0110); // Only samples 3..6 changed.
    /// assert_eq!(&output[1..], &[0x5a; 7]);
    /// ```
    pub fn copy_into(
        self,
        output: &mut [u8],
        plan: SurfaceMemoryPlan,
    ) -> Result<(), SurfaceCopyError> {
        if self.memory_plan().source_surface() != plan.surface() {
            return Err(SurfaceCopyError::SurfaceMismatch);
        }
        plan.buffer_requirements()
            .validate(output)
            .map_err(SurfaceCopyError::Output)?;
        for plane in self.memory_plan().planes() {
            let region = plane.source_region();
            if region.is_empty() {
                continue;
            }
            let target = plan.plane(plane.index()).expect("matching surface plane");
            let element_bits = u64::from(plane.geometry().bits_per_element());
            let bit_start = u64::from(region.x()) * element_bits;
            let bits = u64::from(region.width()) * element_bits;
            let byte_start = (bit_start / 8) as usize;
            let bit_start = (bit_start % 8) as u8;
            let row_len = (u64::from(bit_start) + bits).div_ceil(8) as usize;
            let source = self.plane(plane.index()).expect("decoded selected plane");
            for (row, bytes) in source.rows().expect("decoded linear samples").enumerate() {
                let offset = target.data_offset() as usize
                    + (region.y() as usize + row) * target.stride() as usize
                    + byte_start;
                samples::copy(
                    bytes,
                    0,
                    &mut output[offset..offset + row_len],
                    bit_start,
                    bits,
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
