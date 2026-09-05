use super::SurfaceView;
use crate::image::{
    BufferRequirementError, BufferRequirements, Region, RegionMemoryPlan, SurfaceDescriptor,
    SurfaceMemoryPlan, UnitMemoryPlan, samples,
};

/// Failure before any destination pixel or padding is changed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SurfaceCopyError {
    SurfaceMismatch,
    UnsupportedSourceOver(super::SampleLayout),
    UnsupportedPlaneFlags { index: u8, flags: u16 },
    Output(BufferRequirementError),
}

impl<'source> SurfaceView<'source> {
    /// Copies logical RAW samples into a caller-owned physical layout.
    ///
    /// Validation precedes every write. Row padding, allocation-only rows,
    /// inter-plane gaps, and unused low bits in sub-byte rows become zero.
    /// Unused output suffix bytes remain unchanged. No color conversion or
    /// allocation occurs. An indexed color table remains borrowed from the
    /// source; only sample planes move to the destination.
    pub fn copy_into<'output>(
        self,
        output: &'output mut [u8],
        plan: SurfaceMemoryPlan,
    ) -> Result<SurfaceView<'output>, SurfaceCopyError>
    where
        'source: 'output,
    {
        self.check_copy(plan.surface(), plan.buffer_requirements(), output)?;

        let output = &mut output[..plan.byte_len() as usize];
        output.fill(0);
        for (source, memory) in self.planes().zip(plan.planes()) {
            let geometry = source.geometry();
            let row_size = geometry.minimum_stride().expect("validated plane geometry") as usize;
            if row_size == 0 {
                continue;
            }
            let tail_mask = geometry.row_tail_mask();
            for (row, samples) in source.rows().expect("validated linear storage").enumerate() {
                let target_start = memory.data_offset() as usize + row * memory.stride() as usize;
                let target = &mut output[target_start..target_start + row_size];
                target.copy_from_slice(samples);
                if tail_mask != 0xff {
                    target[row_size - 1] &= tail_mask;
                }
            }
        }

        Ok(SurfaceView::from_plan(plan, output, self.color_table))
    }

    /// Copies an exact region into cropped output, with zero physical padding.
    ///
    /// No allocation or color conversion occurs. Palette bytes remain borrowed
    /// from the source. Invalid source identity, storage flags or caller buffers
    /// are rejected before writes; unused output suffix bytes remain unchanged.
    pub fn copy_region_into<'output>(
        self,
        output: &'output mut [u8],
        plan: RegionMemoryPlan,
    ) -> Result<SurfaceView<'output>, SurfaceCopyError>
    where
        'source: 'output,
    {
        self.copy_region_samples_into(output, plan)?;
        Ok(SurfaceView::from_plan(
            plan.memory_plan(),
            output,
            self.color_table,
        ))
    }

    /// Copies sample bytes without retaining source palette metadata.
    pub(crate) fn copy_region_samples_into(
        self,
        output: &mut [u8],
        plan: RegionMemoryPlan,
    ) -> Result<(), SurfaceCopyError> {
        let memory = plan.memory_plan();
        self.check_copy(plan.source_surface(), memory.buffer_requirements(), output)?;
        let output = &mut output[..memory.byte_len() as usize];
        output.fill(0);
        for (index, source) in self.planes().enumerate() {
            let geometry = source.geometry();
            let region = Region::new(0, 0, geometry.width(), geometry.height())
                .expect("logical plane bounds");
            plan.copy_plane(source, index as u8, region, output);
        }
        Ok(())
    }

    /// Copies one unit's selected source regions into the tight output prefix.
    pub(crate) fn copy_unit_tight_into(
        self,
        output: &mut [u8],
        memory: UnitMemoryPlan,
    ) -> Result<(), SurfaceCopyError> {
        self.check_copy(
            memory.source_surface(),
            memory.buffer_requirements(),
            output,
        )?;
        let output = &mut output[..memory.byte_len() as usize];
        output.fill(0);
        let mut target_start = 0;
        for target in memory.planes() {
            let source = self
                .plane(target.index())
                .expect("selected source surface plane");
            let region = target.source_region();
            let bits_per_element = u64::from(source.geometry().bits_per_element());
            let row_bits = u64::from(region.width()) * bits_per_element;
            let row_len = row_bits.div_ceil(8) as usize;
            if row_len == 0 {
                continue;
            }
            let source_start = u64::from(region.x()) * bits_per_element;
            let source_byte = (source_start / 8) as usize;
            let source_bit = (source_start % 8) as u8;
            for row in 0..region.height() {
                let source = source
                    .row(region.y() + row)
                    .expect("validated unit source row")
                    .expect("in-bounds unit source row");
                let target = &mut output[target_start..target_start + row_len];
                samples::copy(&source[source_byte..], source_bit, target, 0, row_bits);
                target_start += row_len;
            }
        }
        debug_assert_eq!(target_start, memory.sample_byte_len());
        Ok(())
    }

    fn check_copy(
        self,
        source: SurfaceDescriptor,
        buffer: BufferRequirements,
        output: &[u8],
    ) -> Result<(), SurfaceCopyError> {
        if self.surface != source {
            return Err(SurfaceCopyError::SurfaceMismatch);
        }
        for (index, plane) in self.planes().enumerate() {
            let flags = plane.memory().flags().bits();
            if flags != 0 {
                return Err(SurfaceCopyError::UnsupportedPlaneFlags {
                    index: index as u8,
                    flags,
                });
            }
        }
        buffer.validate(output).map_err(SurfaceCopyError::Output)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ColorFormat;
    use crate::image::{
        ColorDescription, PlaneMemoryFlags, PlaneMemoryLayout, RawImageAsset, SampleLayout,
        SurfaceDescriptor, SurfaceRequirements,
    };
    use alloc::vec::Vec;

    #[repr(align(64))]
    struct Aligned([u8; 4096]);

    #[test]
    fn every_layout_copies_only_logical_samples_and_clears_physical_padding() {
        let formats = [
            ColorFormat::I1,
            ColorFormat::I2,
            ColorFormat::I4,
            ColorFormat::I8,
            ColorFormat::A1,
            ColorFormat::A2,
            ColorFormat::A4,
            ColorFormat::A8,
            ColorFormat::L8,
            ColorFormat::RGB565,
            ColorFormat::RGB565Swapped,
            ColorFormat::RGB565A8,
            ColorFormat::RGB888,
            ColorFormat::XRGB8888,
            ColorFormat::RGBA8888,
            ColorFormat::BGRA8888,
        ];
        let yuv = [
            SampleLayout::I420,
            SampleLayout::YV12,
            SampleLayout::NV12,
            SampleLayout::NV21,
            SampleLayout::P010,
            SampleLayout::P016,
        ];
        for layout in formats
            .into_iter()
            .map(SampleLayout::from_color_format)
            .chain(yuv)
        {
            let color = if layout.is_alpha() {
                ColorDescription::NONE
            } else if layout.color_format().is_some() {
                ColorDescription::SRGB
            } else {
                ColorDescription::BT709_YUV_LIMITED
            };
            let surface = SurfaceDescriptor::new(5, 3, layout, color).unwrap();
            let source_plan = surface
                .memory_plan(
                    SurfaceRequirements::new()
                        .with_width_multiple(2)
                        .with_height_multiple(4)
                        .with_stride_multiple(3),
                )
                .unwrap();
            let memory: Vec<_> = source_plan.planes().collect();
            let buffers: Vec<_> = memory
                .iter()
                .enumerate()
                .map(|(index, memory)| {
                    (0..memory.byte_len())
                        .map(|byte| {
                            (byte as u8)
                                .wrapping_add(17 * index as u8)
                                .wrapping_add(0x9b)
                        })
                        .collect::<Vec<_>>()
                })
                .collect();
            let planes: Vec<_> = buffers.iter().map(Vec::as_slice).collect();
            let palette = [0x5a; 1024];
            let mut asset = RawImageAsset::new(surface, &planes).with_memory_layouts(&memory);
            if let Some(count) = layout.color_table_entries() {
                asset = asset.with_color_table(&palette[..count as usize * 4]);
            }
            let source = asset.view().unwrap();
            let plan = surface
                .memory_plan(
                    SurfaceRequirements::new()
                        .with_width_multiple(4)
                        .with_height_multiple(2)
                        .with_stride_multiple(3)
                        .with_base_alignment(64)
                        .with_plane_alignment(64),
                )
                .unwrap();
            let mut output = Aligned([0xa5; 4096]);
            let copied = source.copy_into(&mut output.0, plan).unwrap();
            assert_eq!(copied.surface(), surface);
            assert!(copied.data_addresses_are_aligned());
            assert_eq!(
                copied.color_table().map(|table| table.as_bytes().as_ptr()),
                source.color_table().map(|table| table.as_bytes().as_ptr())
            );
            for (index, plane) in copied.planes().enumerate() {
                let geometry = plane.geometry();
                let stride = plane.memory().stride() as usize;
                let row_size = geometry.minimum_stride().unwrap() as usize;
                let tail =
                    (u64::from(geometry.width()) * u64::from(geometry.bits_per_element())) % 8;
                for (offset, &actual) in plane.bytes().iter().enumerate() {
                    let row = offset / stride;
                    let column = offset % stride;
                    let expected = if row < geometry.height() as usize && column < row_size {
                        let sample = buffers[index][row * memory[index].stride() as usize + column];
                        if column + 1 == row_size && tail != 0 {
                            sample & (0xff << (8 - tail))
                        } else {
                            sample
                        }
                    } else {
                        0
                    };
                    assert_eq!(actual, expected, "{layout:?}, plane {index}, byte {offset}");
                }
            }
            let mut previous_end = 0;
            for memory in plan.planes() {
                let start = memory.data_offset() as usize;
                assert!(output.0[previous_end..start].iter().all(|&byte| byte == 0));
                previous_end = memory.data_end() as usize;
            }
            assert!(
                output.0[plan.byte_len() as usize..]
                    .iter()
                    .all(|&byte| byte == 0xa5)
            );
        }
    }

    #[test]
    fn preflight_failures_leave_every_output_byte_unchanged() {
        let surface =
            SurfaceDescriptor::new(2, 2, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let source = RawImageAsset::new(surface, &[&[1, 2, 3, 4]])
            .view()
            .unwrap();
        let plan = surface
            .memory_plan(SurfaceRequirements::new().with_base_alignment(64))
            .unwrap();
        let mut output = Aligned([0xa5; 4096]);
        assert!(matches!(
            source.copy_into(&mut output.0[..3], plan),
            Err(SurfaceCopyError::Output(
                BufferRequirementError::TooSmall { .. }
            ))
        ));
        assert_eq!(output.0, [0xa5; 4096]);
        assert!(matches!(
            source.copy_into(&mut output.0[1..], plan),
            Err(SurfaceCopyError::Output(
                BufferRequirementError::AddressUnaligned { .. }
            ))
        ));
        assert_eq!(output.0, [0xa5; 4096]);
        let different = SurfaceDescriptor::new(1, 4, SampleLayout::A8, ColorDescription::NONE)
            .unwrap()
            .memory_plan(SurfaceRequirements::new())
            .unwrap();
        assert_eq!(
            source.copy_into(&mut output.0, different),
            Err(SurfaceCopyError::SurfaceMismatch)
        );
        assert_eq!(output.0, [0xa5; 4096]);
        let flagged = [PlaneMemoryLayout::builder(surface.plane(0).unwrap())
            .with_flags(PlaneMemoryFlags::from_bits_retain(0x80))
            .build()
            .unwrap()];
        let source = RawImageAsset::new(surface, &[&[1, 2, 3, 4]])
            .with_memory_layouts(&flagged)
            .view()
            .unwrap();
        assert_eq!(
            source.copy_into(&mut output.0, plan),
            Err(SurfaceCopyError::UnsupportedPlaneFlags {
                index: 0,
                flags: 0x80
            })
        );
        assert_eq!(output.0, [0xa5; 4096]);
    }

    #[test]
    fn zero_geometry_needs_no_allocation_or_address_alignment() {
        for (width, height) in [(0, 0), (0, 3), (5, 0), (0, u32::MAX), (u32::MAX, 0)] {
            let surface =
                SurfaceDescriptor::new(width, height, SampleLayout::A8, ColorDescription::NONE)
                    .unwrap();
            let source = RawImageAsset::new(surface, &[&[]]).view().unwrap();
            let plan = surface
                .memory_plan(SurfaceRequirements::new().with_base_alignment(64))
                .unwrap();
            let mut output = [];
            let copied = source.copy_into(&mut output, plan).unwrap();
            assert_eq!(copied.surface(), surface);
            assert_eq!(copied.plane(0).unwrap().bytes(), []);
        }
    }
}
