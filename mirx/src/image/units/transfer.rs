use super::DecodedUnit;
use crate::image::{RegionMemoryPlan, SurfaceCopyError, SurfaceMemoryPlan};

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
        self.copy_region_into(output, RegionMemoryPlan::whole(plan))
    }

    /// Places only this unit's intersection with an exact cropped output.
    ///
    /// Other samples, absent planes, padding and suffix bytes remain unchanged.
    /// The source descriptor and caller buffer are checked before writing. This
    /// copies already decoded samples; it does not reduce codec staging or work.
    pub fn copy_region_into(
        self,
        output: &mut [u8],
        plan: RegionMemoryPlan,
    ) -> Result<(), SurfaceCopyError> {
        if self.memory_plan().source_surface() != plan.source_surface() {
            return Err(SurfaceCopyError::SurfaceMismatch);
        }
        plan.memory_plan()
            .buffer_requirements()
            .validate(output)
            .map_err(SurfaceCopyError::Output)?;
        for plane in self.memory_plan().planes() {
            let source = self.plane(plane.index()).expect("decoded selected plane");
            plan.copy_plane(source, plane.index(), plane.source_region(), output);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
