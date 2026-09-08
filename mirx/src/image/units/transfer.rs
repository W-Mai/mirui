use super::DecodedUnit;
use crate::image::{RegionMemoryPlan, SampleLayout, SurfaceCopyError, SurfaceMemoryPlan};

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
    ///     SurfaceRequirements, UnitGroup}, coding::CodingRecord};
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

    pub(crate) fn source_over_into(
        self,
        output: &mut [u8],
        plan: SurfaceMemoryPlan,
    ) -> Result<(), SurfaceCopyError> {
        let layout = plan.surface().sample_layout();
        if !layout.supports_source_over() {
            return Err(SurfaceCopyError::UnsupportedSourceOver(layout));
        }
        if !matches!(
            layout,
            SampleLayout::A8 | SampleLayout::RGBA8888 | SampleLayout::BGRA8888
        ) {
            return self.copy_into(output, plan);
        }
        if self.memory_plan().source_surface() != plan.surface() {
            return Err(SurfaceCopyError::SurfaceMismatch);
        }
        plan.buffer_requirements()
            .validate(output)
            .map_err(SurfaceCopyError::Output)?;
        let source = self.plane(0).expect("alpha-bearing packed unit");
        let region = self
            .memory_plan()
            .plane(0)
            .expect("alpha-bearing packed unit")
            .source_region();
        let target = plan.plane(0).expect("alpha-bearing packed surface");
        let channels = if layout == SampleLayout::A8 { 1 } else { 4 };
        for row in 0..region.height() {
            let source = source
                .row(row)
                .expect("validated source row")
                .expect("selected source row");
            let offset = target.data_offset() as usize
                + (region.y() + row) as usize * target.stride() as usize
                + region.x() as usize * channels;
            let target = &mut output[offset..offset + region.width() as usize * channels];
            if channels == 1 {
                for (source, target) in source.iter().zip(target) {
                    *target = alpha_over(*source, *target);
                }
            } else {
                for (source, target) in source.chunks_exact(4).zip(target.chunks_exact_mut(4)) {
                    rgba_over(source, target);
                }
            }
        }
        Ok(())
    }
}

fn alpha_over(source: u8, target: u8) -> u8 {
    let source = u32::from(source);
    let target = u32::from(target);
    ((source * 255 + target * (255 - source) + 127) / 255) as u8
}

fn rgba_over(source: &[u8], target: &mut [u8]) {
    let source_alpha = u32::from(source[3]);
    let target_alpha = u32::from(target[3]);
    let inverse = 255 - source_alpha;
    let output_alpha = source_alpha * 255 + target_alpha * inverse;
    if output_alpha == 0 {
        target.fill(0);
        return;
    }
    for channel in 0..3 {
        let numerator = u32::from(source[channel]) * source_alpha * 255
            + u32::from(target[channel]) * target_alpha * inverse;
        target[channel] = ((numerator + output_alpha / 2) / output_alpha) as u8;
    }
    target[3] = ((output_alpha + 127) / 255) as u8;
}

#[cfg(test)]
mod tests;
