use super::{DecodeError, EncodedImageError, ImageDecodePlan, ImageGroups, Preflight};
use crate::{
    PayloadLimits,
    image::{
        BufferRequirements, DecodeRequest, DecodeUnitRef, DecodeUnits, EncodedImageView,
        GroupPlanes, Region, RegionMemoryPlan, RegionUnits, SurfaceRequirements, UnitGroup,
        units::ScalarProfile,
    },
};

#[derive(Clone, Copy, Debug)]
pub(super) enum DecodeScope {
    Whole,
    Region,
}

impl DecodeScope {
    pub(super) fn units<'a>(
        self,
        group: UnitGroup<'a>,
        region: RegionMemoryPlan,
    ) -> PlannedUnits<'a> {
        match self {
            Self::Whole => PlannedUnits::Whole(group.iter()),
            Self::Region => {
                let query = match group.planes() {
                    GroupPlanes::Joint(_) => region.region(),
                    GroupPlanes::Plane(index) => {
                        region.plane_region(index).expect("selected plane")
                    }
                };
                PlannedUnits::Region(group.units_in(query).expect("exact source region"))
            }
        }
    }
}

pub(super) enum PlannedUnits<'a> {
    Whole(DecodeUnits<'a>),
    Region(RegionUnits<'a>),
}

impl PlannedUnits<'_> {
    fn work_bound(&self) -> u64 {
        match self {
            Self::Whole(_) => 0,
            Self::Region(units) => units.work_bound(),
        }
    }
}

impl<'a> Iterator for PlannedUnits<'a> {
    type Item = DecodeUnitRef<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Whole(units) => units.next(),
            Self::Region(units) => units.next(),
        }
    }
}

#[derive(Default)]
struct Checksums {
    end: u32,
    byte_len: u64,
}

impl Checksums {
    fn verify(
        &mut self,
        image: EncodedImageView<'_>,
        range: core::ops::Range<u32>,
        preflight: &mut Preflight<'_>,
    ) -> Result<(), EncodedImageError> {
        if range.end <= self.end {
            return Ok(());
        }
        let media = image.media();
        // Includes bounded section scans and both integrity index searches.
        preflight.spend(u64::from(media.header().section_count()) * 2 + 64)?;
        let plan = media
            .data_check_plan(range.start.max(self.end)..range.end)
            .map_err(EncodedImageError::Media)?;
        preflight.spend(u64::from(plan.byte_len()))?;
        let end = plan.covered_end().unwrap_or_else(|| {
            let data = image.data.descriptor();
            data.offset() + data.size()
        });
        let byte_len = u64::from(plan.byte_len());
        plan.verify().map_err(EncodedImageError::Media)?;
        self.end = end;
        self.byte_len += byte_len;
        Ok(())
    }
}

impl<'a, 'g> ImageGroups<'a, 'g> {
    /// Preflights selected scalar units and plans exact cropped reconstruction.
    ///
    /// Source groups already have verified static coverage. Complete intersecting
    /// units are decoded into reusable workspace, then only the requested sample
    /// intersections are copied. Chroma boundaries are exact, not expanded.
    /// Shared checksum partitions are verified once; whole-DATA integrity still
    /// requires one complete DATA scan. Unselected coding syntax is not consumed.
    /// No allocation or file I/O occurs. All failures precede final output writes.
    ///
    /// ```
    /// use mirx::{types::ByteAlignment, PayloadLimits, coding::Rle, media::DataIntegrity, image::{
    ///     ColorDescription, CoverageBudget, EncodedImageAsset, EncodedImageView,
    ///     SampleLayout, SurfaceDescriptor, SurfaceRequirements, UnitGroupRecord,
    /// }};
    /// let surface = SurfaceDescriptor::new(4, 2, SampleLayout::A8, ColorDescription::NONE).unwrap();
    /// let codings = [Rle::new().record()];
    /// let records = [UnitGroupRecord::new(0, 0..8).unwrap().with_tiles(2, 1)];
    /// let data = [0x81, 10, 0x81, 20, 0x81, 30, 0x81, 40];
    /// let bytes = EncodedImageAsset::from_groups(surface, &codings, &records, &data)
    ///     .with_integrity(DataIntegrity::Indexed(&[2, 4, 6, 8])).encode().unwrap();
    /// let image = EncodedImageView::open(&bytes).unwrap();
    /// let mut slots = [None];
    /// let groups = image.groups_into(&mut slots, &mut CoverageBudget::new(100)).unwrap();
    /// let plan = groups.decode_region_plan(surface.region(2, 0, 2, 2).unwrap(),
    ///     SurfaceRequirements::new().with_base_alignment(ByteAlignment::new(64).unwrap()).with_stride_multiple(64),
    ///     &PayloadLimits::EMBEDDED).unwrap();
    /// assert_eq!(plan.unit_count(), 2);
    /// assert_eq!(plan.input_byte_len(), 4);
    /// assert_eq!(plan.checksum_byte_len(), 4);
    /// assert_eq!(plan.workspace_requirements().byte_len(), 2);
    /// #[repr(align(64))]
    /// struct Output([u8; 128]);
    /// let mut output = Output([0; 128]);
    /// let mut workspace = [0; 2];
    /// let decoded = plan.decode_into(&mut output.0, &mut workspace).unwrap();
    /// assert_eq!(decoded.plane(0).unwrap().row(0).unwrap(), Some(&[20, 20][..]));
    /// assert_eq!(decoded.plane(0).unwrap().row(1).unwrap(), Some(&[40, 40][..]));
    /// ```
    pub fn decode_region_plan(
        self,
        requested: Region,
        requirements: SurfaceRequirements,
        limits: &PayloadLimits,
    ) -> Result<ImageDecodePlan<'a, 'g>, DecodeError> {
        self.decode_region_plan_for(requested, DecodeRequest::new(requirements), limits)
    }

    /// Preflights cropped reconstruction for explicit execution and memory policy.
    pub fn decode_region_plan_for(
        self,
        requested: Region,
        request: DecodeRequest,
        limits: &PayloadLimits,
    ) -> Result<ImageDecodePlan<'a, 'g>, DecodeError> {
        request
            .validate_reconstruction()
            .map_err(DecodeError::Request)?;
        let mut preflight = Preflight::new(limits, self.len()).map_err(DecodeError::Image)?;
        let memory = self
            .image()
            .surface()
            .region_plan(requested, request.requirements())
            .map_err(DecodeError::Memory)?;
        preflight
            .spend(u64::from(memory.memory_plan().byte_len()) + 2 * self.len() as u64)
            .map_err(DecodeError::Image)?;
        let mut workspace = 0;
        let mut input_bytes = 0;
        let mut input_alignment = crate::ByteAlignment::ONE;
        let mut input_addresses_aligned = true;
        let mut checksums = Checksums::default();
        for (index, group) in self.iter().enumerate() {
            let units = DecodeScope::Region.units(group, memory);
            preflight
                .spend(units.work_bound() * 2)
                .map_err(DecodeError::Image)?;
            let mut profile = None;
            let base = self.image.data.descriptor().offset()
                + self
                    .image
                    .group_source()
                    .record(index)
                    .map_err(DecodeError::Image)?
                    .data_range()
                    .start;
            for unit in units {
                input_alignment = input_alignment.max(unit.input_alignment());
                input_addresses_aligned &= unit.data_address_is_aligned();
                preflight.add_units(1).map_err(DecodeError::Image)?;
                let selected_profile = match profile {
                    Some(profile) => profile,
                    None => {
                        let value =
                            ScalarProfile::new(group.coding(), group.surface().sample_layout())
                                .map_err(|error| {
                                    DecodeError::Image(EncodedImageError::Coding {
                                        group: index,
                                        error,
                                    })
                                })?;
                        profile = Some(value);
                        value
                    }
                };
                let ordinal = group
                    .selection()
                    .position(unit.cell())
                    .expect("selected ordinal");
                let unit_memory = preflight
                    .unit(index, ordinal, unit, selected_profile)
                    .map_err(DecodeError::Image)?;
                workspace = workspace.max(unit_memory.byte_len());
                input_bytes += unit.data().len() as u64;
                let profile_work = selected_profile
                    .extra_work(unit_memory)
                    .expect("preflighted profile work");
                preflight
                    .spend_replay(
                        unit.data().len(),
                        unit_memory.sample_byte_len(),
                        profile_work,
                    )
                    .map_err(DecodeError::Image)?;
                let range = unit.data_range();
                checksums
                    .verify(
                        self.image,
                        base + range.start..base + range.end,
                        &mut preflight,
                    )
                    .map_err(DecodeError::Image)?;
            }
        }
        Ok(ImageDecodePlan {
            groups: self,
            memory,
            scope: DecodeScope::Region,
            request,
            workspace: BufferRequirements::new(workspace, request.workspace_alignment())
                .map_err(DecodeError::Memory)?,
            units: preflight.total_units(),
            work: preflight.work(),
            input_bytes,
            input_alignment,
            input_addresses_aligned,
            checksum_bytes: checksums.byte_len,
        })
    }
}

#[cfg(test)]
mod tests;
