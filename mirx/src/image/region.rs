use super::{
    Region, SurfaceDescriptor, SurfaceMemoryPlan, SurfacePlanError, SurfacePlane,
    SurfaceRequirements, samples,
};

/// Exact source region and a checked, independently aligned cropped allocation.
///
/// Sample layout, color, flags and pixel aspect are preserved. Interior YUV
/// boundaries cannot split a chroma sample; packed samples need not start at
/// byte boundaries. The plan never silently expands the requested region.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegionMemoryPlan {
    source: SurfaceDescriptor,
    region: Region,
    memory: SurfaceMemoryPlan,
}

impl SurfaceDescriptor {
    /// Plans storage for an exact region in logical surface coordinates.
    ///
    /// ```
    /// use mirx::{types::ByteAlignment, image::{ColorDescription, SampleLayout, SurfaceDescriptor, SurfaceRequirements}};
    /// let surface = SurfaceDescriptor::new(5, 3, SampleLayout::NV12,
    ///     ColorDescription::BT709_YUV_LIMITED).unwrap();
    /// let region = surface.region(2, 0, 3, 3).unwrap();
    /// let plan = surface.region_plan(region, SurfaceRequirements::new()
    ///     .with_base_alignment(ByteAlignment::new(64).unwrap()).with_plane_alignment(ByteAlignment::new(64).unwrap()).with_stride_multiple(64)).unwrap();
    /// assert_eq!(plan.memory_plan().surface().width(), 3);
    /// assert_eq!(plan.plane_region(1).unwrap().width(), 2);
    /// assert_eq!(plan.memory_plan().byte_len(), 320);
    /// ```
    pub fn region_plan(
        self,
        region: Region,
        requirements: SurfaceRequirements,
    ) -> Result<RegionMemoryPlan, SurfacePlanError> {
        self.region(region.x(), region.y(), region.width(), region.height())
            .map_err(SurfacePlanError::Region)?;
        for index in 0..self.plane_count() {
            region
                .for_plane(self, index)
                .map_err(SurfacePlanError::Region)?;
        }
        let memory = self
            .with_extent(region.width(), region.height())
            .memory_plan(requirements)?;
        Ok(RegionMemoryPlan {
            source: self,
            region,
            memory,
        })
    }
}

impl RegionMemoryPlan {
    pub const fn source_surface(self) -> SurfaceDescriptor {
        self.source
    }

    pub const fn region(self) -> Region {
        self.region
    }

    /// Describes cropped output at local origin, including its physical padding.
    pub const fn memory_plan(self) -> SurfaceMemoryPlan {
        self.memory
    }

    /// Returns original plane coordinates, without renumbering chroma planes.
    pub fn plane_region(self, index: u8) -> Option<Region> {
        self.region.for_plane(self.source, index).ok()
    }

    pub(super) fn whole(memory: SurfaceMemoryPlan) -> Self {
        let source = memory.surface();
        Self {
            source,
            region: source
                .region(0, 0, source.width(), source.height())
                .unwrap(),
            memory,
        }
    }

    /// Copies validated linear samples; callers check source identity and output.
    pub(super) fn copy_plane(
        self,
        source: SurfacePlane<'_>,
        index: u8,
        source_region: Region,
        output: &mut [u8],
    ) {
        let target_region = self.plane_region(index).expect("selected target plane");
        let Some(overlap) = source_region.intersection(target_region) else {
            return;
        };
        let target = self.memory.plane(index).expect("selected target storage");
        let element_bits = u64::from(source.geometry().bits_per_element());
        let source_start = u64::from(overlap.x() - source_region.x()) * element_bits;
        let target_start = u64::from(overlap.x() - target_region.x()) * element_bits;
        let bits = u64::from(overlap.width()) * element_bits;
        let source_byte = (source_start / 8) as usize;
        let target_byte = (target_start / 8) as usize;
        let source_bit = (source_start % 8) as u8;
        let target_bit = (target_start % 8) as u8;
        let row_len = (u64::from(target_bit) + bits).div_ceil(8) as usize;
        for row in 0..overlap.height() {
            let source_row = overlap.y() - source_region.y() + row;
            let target_row = overlap.y() - target_region.y() + row;
            let bytes = source
                .row(source_row)
                .expect("validated linear samples")
                .expect("source intersection row");
            let offset = target.data_offset() as usize
                + target_row as usize * target.stride() as usize
                + target_byte;
            samples::copy(
                &bytes[source_byte..],
                source_bit,
                &mut output[offset..offset + row_len],
                target_bit,
                bits,
            );
        }
    }
}

#[cfg(test)]
mod tests;
