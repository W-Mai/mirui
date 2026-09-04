use core::iter::FusedIterator;

use super::{DecodeUnitRef, GroupPlanes, Region, SurfaceDescriptor};
use crate::image::{
    BufferRequirements, PlaneGeometry, PlaneMemoryLayout, SurfacePlanError, SurfaceRequirements,
};

/// Local decoded storage and the source-plane region represented by one unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnitPlane {
    index: u8,
    source_region: Region,
    geometry: PlaneGeometry,
    memory: PlaneMemoryLayout,
}

impl UnitPlane {
    pub const fn index(self) -> u8 {
        self.index
    }
    pub const fn source_region(self) -> Region {
        self.source_region
    }
    pub const fn geometry(self) -> PlaneGeometry {
        self.geometry
    }
    pub const fn memory(self) -> PlaneMemoryLayout {
        self.memory
    }
}

/// Caller-owned decoded-unit allocation, independent of encoded input storage.
///
/// Only selected planes are allocated. Source origins are retained for later
/// placement; this is not an in-place writeback plan for a full-surface buffer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnitMemoryPlan {
    surface: SurfaceDescriptor,
    region: Region,
    planes: GroupPlanes,
    requirements: SurfaceRequirements,
    buffer: BufferRequirements,
}

impl DecodeUnitRef<'_> {
    pub fn memory_plan(
        self,
        requirements: SurfaceRequirements,
    ) -> Result<UnitMemoryPlan, SurfacePlanError> {
        requirements.validate()?;
        let alignment = requirements
            .base_alignment()
            .max(requirements.plane_alignment());
        let mut plan = UnitMemoryPlan {
            surface: self.surface,
            region: self.region,
            planes: self.planes,
            requirements,
            buffer: BufferRequirements::new(0, alignment)?,
        };
        let mut end = 0;
        for index in 0..self.surface.plane_count() {
            if let Some(plane) = plan.planned_plane(index, end)? {
                end = plane.memory.data_end();
            }
        }
        plan.buffer = BufferRequirements::new(end, alignment)?;
        Ok(plan)
    }
}

impl UnitMemoryPlan {
    pub const fn requirements(self) -> SurfaceRequirements {
        self.requirements
    }
    pub const fn buffer_requirements(self) -> BufferRequirements {
        self.buffer
    }
    pub const fn byte_len(self) -> u32 {
        self.buffer.byte_len() as u32
    }
    pub const fn base_alignment(self) -> u32 {
        self.buffer.base_alignment() as u32
    }
    pub fn plane_count(self) -> u8 {
        match self.planes {
            GroupPlanes::Joint(mask) => mask.count_ones() as u8,
            GroupPlanes::Plane(_) => 1,
        }
    }
    /// Looks up the original surface plane index, not a renumbered output slot.
    pub fn plane(self, index: u8) -> Option<UnitPlane> {
        if index >= self.surface.plane_count() || !self.planes.contains(index) {
            return None;
        }
        let mut end = 0;
        for current in 0..=index {
            if let Some(plane) = self
                .planned_plane(current, end)
                .expect("validated unit memory plan")
            {
                if current == index {
                    return Some(plane);
                }
                end = plane.memory.data_end();
            }
        }
        None
    }

    pub fn planes(self) -> UnitPlanes {
        UnitPlanes {
            plan: self,
            front: 0,
            back: self.surface.plane_count(),
            remaining: self.plane_count(),
        }
    }

    fn planned_plane(
        self,
        index: u8,
        previous_end: u32,
    ) -> Result<Option<UnitPlane>, SurfacePlanError> {
        let Some(source_region) = self.planes.plane_region(self.surface, self.region, index) else {
            return Ok(None);
        };
        let geometry = self
            .surface
            .plane(index)
            .expect("selected surface plane")
            .with_extent(source_region.width(), source_region.height());
        let memory = self
            .requirements
            .plan_plane(geometry, index, previous_end)?;
        Ok(Some(UnitPlane {
            index,
            source_region,
            geometry,
            memory,
        }))
    }
}

/// Iterates selected output planes, retaining their original surface indices.
#[derive(Clone, Debug)]
pub struct UnitPlanes {
    plan: UnitMemoryPlan,
    front: u8,
    back: u8,
    remaining: u8,
}
impl Iterator for UnitPlanes {
    type Item = UnitPlane;
    fn next(&mut self) -> Option<Self::Item> {
        while self.front < self.back {
            let index = self.front;
            self.front += 1;
            if let Some(plane) = self.plan.plane(index) {
                self.remaining -= 1;
                return Some(plane);
            }
        }
        None
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.remaining as usize;
        (len, Some(len))
    }
}
impl DoubleEndedIterator for UnitPlanes {
    fn next_back(&mut self) -> Option<Self::Item> {
        while self.front < self.back {
            self.back -= 1;
            if let Some(plane) = self.plan.plane(self.back) {
                self.remaining -= 1;
                return Some(plane);
            }
        }
        None
    }
}
impl ExactSizeIterator for UnitPlanes {
    fn len(&self) -> usize {
        self.remaining as usize
    }
}
impl FusedIterator for UnitPlanes {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        image::{ColorDescription, SampleLayout, UnitGroup},
        media::{CodingId, CodingRecord},
    };

    fn coding() -> CodingRecord<'static> {
        CodingRecord::new(CodingId::new(19), 1, &[])
    }

    #[test]
    fn odd_joint_edges_keep_source_regions_and_independent_output_alignment() {
        let surface = SurfaceDescriptor::new(
            5,
            3,
            SampleLayout::NV12,
            ColorDescription::BT709_YUV_LIMITED,
        )
        .unwrap();
        let group = UnitGroup::builder(surface, coding(), &[1; 6])
            .with_tiles(2, 2)
            .build()
            .unwrap();
        let plan = group
            .get(5)
            .unwrap()
            .memory_plan(
                SurfaceRequirements::new()
                    .with_base_alignment(64)
                    .with_plane_alignment(64)
                    .with_stride_multiple(64),
            )
            .unwrap();
        let y = plan.plane(0).unwrap();
        let uv = plan.plane(1).unwrap();
        assert_eq!(y.source_region(), Region::new(4, 2, 1, 1).unwrap());
        assert_eq!(uv.source_region(), Region::new(2, 1, 1, 1).unwrap());
        assert_eq!(y.memory().stride(), 64);
        assert_eq!(uv.memory().stride(), 64);
        assert_eq!(uv.geometry().bits_per_element(), 16);
        assert_eq!(
            (y.memory().data_offset(), uv.memory().data_offset()),
            (0, 64)
        );
        assert_eq!(plan.byte_len(), 128);
        assert_eq!(plan.base_alignment(), 64);
        #[repr(align(64))]
        struct Buffer([u8; 256]);
        let bytes = Buffer([0; 256]);
        assert_eq!(plan.buffer_requirements().validate(&bytes.0), Ok(()));
        assert!(plan.buffer_requirements().validate(&bytes.0[1..]).is_err());
        assert!(
            plan.buffer_requirements()
                .validate(&bytes.0[..127])
                .is_err()
        );
    }

    #[test]
    fn planar_units_keep_original_indices_without_allocating_absent_planes() {
        let surface = SurfaceDescriptor::new(
            5,
            3,
            SampleLayout::NV12,
            ColorDescription::BT709_YUV_LIMITED,
        )
        .unwrap();
        for (planes, width, height) in
            [(GroupPlanes::Joint(2), 2, 2), (GroupPlanes::Plane(1), 1, 1)]
        {
            let group = UnitGroup::builder(surface, coding(), &[1; 6])
                .with_planes(planes)
                .with_tiles(width, height)
                .build()
                .unwrap();
            let plan = group
                .get(5)
                .unwrap()
                .memory_plan(SurfaceRequirements::new())
                .unwrap();
            assert_eq!(plan.plane_count(), 1);
            assert_eq!(plan.byte_len(), 2);
            assert_eq!(plan.plane(0), None);
            let plane = plan.plane(1).unwrap();
            assert_eq!(plane.source_region(), Region::new(2, 1, 1, 1).unwrap());
            assert_eq!(plane.memory().data_offset(), 0);
            let mut planes = plan.planes();
            assert_eq!(planes.len(), 1);
            assert_eq!(planes.next_back(), Some(plane));
            assert_eq!(planes.len(), 0);
            assert_eq!(planes.next(), None);
        }
    }

    #[test]
    fn input_alignment_and_sub_byte_origins_do_not_change_output_rules() {
        let rgba =
            SurfaceDescriptor::new(20, 12, SampleLayout::RGBA8888, ColorDescription::SRGB).unwrap();
        let unit = UnitGroup::builder(rgba, coding(), &[1])
            .with_input_alignment(64)
            .build()
            .unwrap()
            .get(0)
            .unwrap();
        let tight = unit.memory_plan(SurfaceRequirements::new()).unwrap();
        assert_eq!(tight.base_alignment(), 1);
        assert_eq!(tight.plane(0).unwrap().memory().stride(), 80);
        let padded = unit
            .memory_plan(
                SurfaceRequirements::new()
                    .with_base_alignment(64)
                    .with_stride_multiple(64),
            )
            .unwrap();
        assert_eq!(padded.plane(0).unwrap().memory().allocation_width(), 20);
        assert_eq!(padded.plane(0).unwrap().memory().stride(), 128);
        let bits = SurfaceDescriptor::new(9, 2, SampleLayout::A1, ColorDescription::NONE).unwrap();
        let bit_unit = UnitGroup::builder(bits, coding(), &[1; 6])
            .with_tiles(3, 1)
            .build()
            .unwrap()
            .get(1)
            .unwrap();
        let plane = bit_unit
            .memory_plan(SurfaceRequirements::new())
            .unwrap()
            .plane(0)
            .unwrap();
        assert_eq!(plane.source_region().x(), 3);
        assert_eq!(plane.geometry().width(), 3);
        assert_eq!(plane.memory().stride(), 1);
        assert_eq!(
            unit.memory_plan(SurfaceRequirements::new().with_stride_multiple(0)),
            Err(SurfacePlanError::InvalidStrideMultiple)
        );
        let huge =
            SurfaceDescriptor::new(u32::MAX, 1, SampleLayout::RGBA8888, ColorDescription::SRGB)
                .unwrap();
        assert_eq!(
            UnitGroup::builder(huge, coding(), &[1])
                .build()
                .unwrap()
                .get(0)
                .unwrap()
                .memory_plan(SurfaceRequirements::new()),
            Err(SurfacePlanError::SizeOverflow)
        );
    }

    #[test]
    fn whole_unit_and_surface_plans_share_all_layout_rules() {
        let layouts = (0..=255)
            .filter_map(crate::ColorFormat::from_u8)
            .map(SampleLayout::from_color_format)
            .chain([
                SampleLayout::I420,
                SampleLayout::YV12,
                SampleLayout::NV12,
                SampleLayout::NV21,
                SampleLayout::P010,
                SampleLayout::P016,
            ]);
        for layout in layouts {
            let color = if layout.is_alpha() {
                ColorDescription::NONE
            } else if layout.is_yuv() {
                ColorDescription::BT709_YUV_LIMITED
            } else {
                ColorDescription::SRGB
            };
            let surface = SurfaceDescriptor::new(5, 3, layout, color).unwrap();
            let unit = UnitGroup::builder(surface, coding(), &[1])
                .build()
                .unwrap()
                .get(0)
                .unwrap();
            for requirements in [
                SurfaceRequirements::new(),
                SurfaceRequirements::new()
                    .with_plane_alignment(64)
                    .with_stride_multiple(64)
                    .with_width_multiple(8)
                    .with_height_multiple(2),
            ] {
                let complete = surface.memory_plan(requirements).unwrap();
                let unit = unit.memory_plan(requirements).unwrap();
                assert_eq!(unit.buffer_requirements(), complete.buffer_requirements());
                for plane in 0..surface.plane_count() {
                    assert_eq!(
                        unit.plane(plane).unwrap().memory(),
                        complete.plane(plane).unwrap()
                    );
                    assert_eq!(
                        unit.plane(plane).unwrap().geometry(),
                        surface.plane(plane).unwrap()
                    );
                }
                assert_eq!(unit.planes().count(), surface.plane_count() as usize);
            }
        }
    }
}
