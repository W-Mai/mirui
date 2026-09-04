use core::{iter::FusedIterator, ops::Range};

use super::{Region, RegionError, SurfaceDescriptor, TileGrid, TileGridError};
use crate::media::{CodingRecord, UnitIndex, UnitIndexError, UnitSelection, UnitSelectionError};

mod wire;
pub use wire::{GroupSelection, UNIT_GROUP_RECORD_LEN, UnitGroupRecord, UnitGroupRecordError};

/// Included planes and the coordinate space of a group's regions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GroupPlanes {
    /// A nonzero plane bitmask; regions use logical surface coordinates.
    Joint(u8),
    /// One plane index; regions use that plane's element coordinates.
    Plane(u8),
}

impl GroupPlanes {
    pub fn contains(self, index: u8) -> bool {
        match self {
            Self::Joint(mask) => index < 8 && mask & (1 << index) != 0,
            Self::Plane(plane) => plane == index,
        }
    }

    fn dimensions(self, surface: SurfaceDescriptor) -> Result<(u32, u32), UnitGroupError> {
        match self {
            Self::Joint(mask) => {
                let valid = (1u8 << surface.plane_count()) - 1;
                if mask == 0 || mask & !valid != 0 {
                    return Err(UnitGroupError::InvalidPlanes(self));
                }
                Ok((surface.width(), surface.height()))
            }
            Self::Plane(index) => {
                let plane = surface
                    .plane(index)
                    .ok_or(UnitGroupError::InvalidPlanes(self))?;
                Ok((plane.width(), plane.height()))
            }
        }
    }
}

/// Shared reference rule; a frame session resolves the actual reference state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ReferenceMode {
    #[default]
    Independent,
    Previous,
}

/// One synthesized unit; no corresponding per-unit struct is stored on disk.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecodeUnitRef<'a> {
    surface: SurfaceDescriptor,
    region: Region,
    cell: u32,
    planes: GroupPlanes,
    coding: CodingRecord<'a>,
    data: &'a [u8],
    offset: u32,
    reference: ReferenceMode,
    input_alignment: u32,
}

impl<'a> DecodeUnitRef<'a> {
    pub const fn cell(self) -> u32 {
        self.cell
    }
    /// Uses surface or plane coordinates as specified by `planes()`.
    pub const fn region(self) -> Region {
        self.region
    }
    pub const fn planes(self) -> GroupPlanes {
        self.planes
    }
    pub const fn coding(self) -> CodingRecord<'a> {
        self.coding
    }
    pub const fn data(self) -> &'a [u8] {
        self.data
    }
    /// Encoded input range relative to the group's DATA slice.
    pub fn data_range(self) -> Range<u32> {
        self.offset..self.offset + self.data.len() as u32
    }
    pub const fn reference(self) -> ReferenceMode {
        self.reference
    }
    pub const fn input_alignment(self) -> u32 {
        self.input_alignment
    }
    pub fn data_address_is_aligned(self) -> bool {
        self.data.as_ptr() as usize % self.input_alignment as usize == 0
    }
    /// Returns element coordinates for an included plane; absent planes return None.
    pub fn plane_region(self, index: u8) -> Option<Region> {
        if !self.planes.contains(index) {
            return None;
        }
        match self.planes {
            GroupPlanes::Joint(_) => self.region.for_plane(self.surface, index).ok(),
            GroupPlanes::Plane(_) => Some(self.region),
        }
    }
}

/// Shared rules that resolve selected image units without allocating a table.
///
/// Construction validates geometry and input storage, not profile syntax,
/// checksums, supported acceleration, or availability of reference frames.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnitGroup<'a> {
    surface: SurfaceDescriptor,
    grid: TileGrid,
    planes: GroupPlanes,
    coding: CodingRecord<'a>,
    data: &'a [u8],
    selection: UnitSelection<'a>,
    index: UnitIndex<'a>,
    reference: ReferenceMode,
    input_alignment: u32,
}

impl<'a> UnitGroup<'a> {
    pub fn builder(
        surface: SurfaceDescriptor,
        coding: CodingRecord<'a>,
        data: &'a [u8],
    ) -> UnitGroupBuilder<'a> {
        UnitGroupBuilder {
            surface,
            coding,
            data,
            tiles: None,
            planes: GroupPlanes::Joint((1 << surface.plane_count()) - 1),
            selection: None,
            index: None,
            reference: ReferenceMode::Independent,
            input_alignment: 1,
        }
    }
    pub const fn surface(self) -> SurfaceDescriptor {
        self.surface
    }
    pub const fn grid(self) -> TileGrid {
        self.grid
    }
    pub const fn planes(self) -> GroupPlanes {
        self.planes
    }
    pub const fn coding(self) -> CodingRecord<'a> {
        self.coding
    }
    pub const fn data(self) -> &'a [u8] {
        self.data
    }
    pub const fn selection(self) -> UnitSelection<'a> {
        self.selection
    }
    pub const fn index(self) -> UnitIndex<'a> {
        self.index
    }
    pub const fn reference(self) -> ReferenceMode {
        self.reference
    }
    pub const fn input_alignment(self) -> u32 {
        self.input_alignment
    }
    pub const fn len(self) -> usize {
        self.selection.len()
    }
    pub const fn is_empty(self) -> bool {
        self.selection.is_empty()
    }

    /// Checks the actual group address; relative unit starts were checked at build time.
    pub fn data_addresses_are_aligned(self) -> bool {
        self.is_empty() || self.data.as_ptr() as usize % self.input_alignment as usize == 0
    }

    pub fn get(self, ordinal: usize) -> Option<DecodeUnitRef<'a>> {
        let cell = self.selection.get(ordinal)?;
        let range = self.index.get(ordinal)?;
        Some(DecodeUnitRef {
            surface: self.surface,
            region: self.grid.get(cell as usize)?,
            cell,
            planes: self.planes,
            coding: self.coding,
            data: &self.data[range.start as usize..range.end as usize],
            offset: range.start,
            reference: self.reference,
            input_alignment: self.input_alignment,
        })
    }

    /// Resolves a grid cell only when it is present in the group's selection.
    pub fn cell(self, cell: u32) -> Option<DecodeUnitRef<'a>> {
        self.get(self.selection.position(cell)?)
    }

    pub fn iter(self) -> DecodeUnits<'a> {
        DecodeUnits {
            group: self,
            front: 0,
            back: self.len(),
        }
    }
}

/// Checked group construction from borrowed coding, selection, ranges and DATA.
#[derive(Clone, Copy, Debug)]
pub struct UnitGroupBuilder<'a> {
    surface: SurfaceDescriptor,
    coding: CodingRecord<'a>,
    data: &'a [u8],
    tiles: Option<(u32, u32)>,
    planes: GroupPlanes,
    selection: Option<UnitSelection<'a>>,
    index: Option<UnitIndex<'a>>,
    reference: ReferenceMode,
    input_alignment: u32,
}

impl<'a> UnitGroupBuilder<'a> {
    fn grid(&self) -> Result<TileGrid, UnitGroupError> {
        let (width, height) = self.planes.dimensions(self.surface)?;
        let (tile_width, tile_height) = self.tiles.unwrap_or((width.max(1), height.max(1)));
        TileGrid::new(width, height, tile_width, tile_height).map_err(UnitGroupError::Grid)
    }

    pub const fn with_tiles(mut self, width: u32, height: u32) -> Self {
        self.tiles = Some((width, height));
        self
    }
    pub const fn with_planes(mut self, planes: GroupPlanes) -> Self {
        self.planes = planes;
        self
    }
    pub const fn with_selection(mut self, selection: UnitSelection<'a>) -> Self {
        self.selection = Some(selection);
        self
    }
    pub const fn with_index(mut self, index: UnitIndex<'a>) -> Self {
        self.index = Some(index);
        self
    }
    pub const fn with_reference(mut self, reference: ReferenceMode) -> Self {
        self.reference = reference;
        self
    }
    /// Requires every unit start offset to align; actual pointer alignment remains separate.
    pub const fn with_input_alignment(mut self, alignment: u32) -> Self {
        self.input_alignment = alignment;
        self
    }

    pub fn build(self) -> Result<UnitGroup<'a>, UnitGroupError> {
        let data_len = u32::try_from(self.data.len()).map_err(|_| UnitGroupError::SizeOverflow)?;
        if !self.input_alignment.is_power_of_two() {
            return Err(UnitGroupError::InvalidAlignment(self.input_alignment));
        }
        let grid = self.grid()?;
        if let (GroupPlanes::Joint(_), Some(first)) = (self.planes, grid.get(0)) {
            for plane in 0..self.surface.plane_count() {
                if self.planes.contains(plane) {
                    first
                        .for_plane(self.surface, plane)
                        .map_err(UnitGroupError::Region)?;
                }
            }
        }
        let selection = match self.selection {
            Some(selection) => selection,
            None => UnitSelection::all(grid.len() as u32).map_err(UnitGroupError::Selection)?,
        };
        if selection.cell_count() as usize != grid.len() {
            return Err(UnitGroupError::CellCountMismatch {
                expected: grid.len(),
                actual: selection.cell_count(),
            });
        }
        let count = selection.len() as u32;
        let index = match self.index {
            Some(index) => index,
            None => {
                let unit_bytes = if count == 0 {
                    if data_len != 0 {
                        return Err(UnitGroupError::UnreferencedData);
                    }
                    0
                } else {
                    if data_len % count != 0 {
                        return Err(UnitGroupError::UnequalFixedUnits);
                    }
                    data_len / count
                };
                UnitIndex::fixed(count, unit_bytes).map_err(UnitGroupError::Index)?
            }
        };
        if index.len() != selection.len() {
            return Err(UnitGroupError::UnitCountMismatch {
                expected: selection.len(),
                actual: index.len(),
            });
        }
        if index.byte_len() != data_len {
            return Err(UnitGroupError::DataLengthMismatch {
                expected: index.byte_len(),
                actual: data_len,
            });
        }
        for (ordinal, range) in index.iter().enumerate() {
            if range.is_empty() {
                return Err(UnitGroupError::EmptyUnit {
                    ordinal: ordinal as u32,
                });
            }
            if range.start % self.input_alignment != 0 {
                return Err(UnitGroupError::UnalignedUnit {
                    ordinal: ordinal as u32,
                    offset: range.start,
                    alignment: self.input_alignment,
                });
            }
        }
        Ok(UnitGroup {
            surface: self.surface,
            grid,
            planes: self.planes,
            coding: self.coding,
            data: self.data,
            selection,
            index,
            reference: self.reference,
            input_alignment: self.input_alignment,
        })
    }
}

#[derive(Clone, Debug)]
pub struct DecodeUnits<'a> {
    group: UnitGroup<'a>,
    front: usize,
    back: usize,
}
impl<'a> Iterator for DecodeUnits<'a> {
    type Item = DecodeUnitRef<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        let index = self.front;
        self.front += 1;
        self.group.get(index)
    }
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        if n >= self.len() {
            self.front = self.back;
            return None;
        }
        self.front += n;
        self.next()
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len(), Some(self.len()))
    }
    fn count(self) -> usize {
        self.len()
    }
    fn last(mut self) -> Option<Self::Item> {
        self.next_back()
    }
}
impl DoubleEndedIterator for DecodeUnits<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        self.back -= 1;
        self.group.get(self.back)
    }
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        if n >= self.len() {
            self.front = self.back;
            return None;
        }
        self.back -= n;
        self.next_back()
    }
}
impl ExactSizeIterator for DecodeUnits<'_> {
    fn len(&self) -> usize {
        self.back - self.front
    }
}
impl FusedIterator for DecodeUnits<'_> {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum UnitGroupError {
    Grid(TileGridError),
    Region(RegionError),
    Selection(UnitSelectionError),
    Index(UnitIndexError),
    InvalidPlanes(GroupPlanes),
    InvalidAlignment(u32),
    SizeOverflow,
    CellCountMismatch {
        expected: usize,
        actual: u32,
    },
    UnitCountMismatch {
        expected: usize,
        actual: usize,
    },
    DataLengthMismatch {
        expected: u32,
        actual: u32,
    },
    UnequalFixedUnits,
    UnreferencedData,
    EmptyUnit {
        ordinal: u32,
    },
    UnalignedUnit {
        ordinal: u32,
        offset: u32,
        alignment: u32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        image::{ColorDescription, SampleLayout},
        media::{CodingId, UnitIndexEncoding, UnitSelectionEncoding},
    };

    fn surface() -> SurfaceDescriptor {
        SurfaceDescriptor::new(
            5,
            3,
            SampleLayout::NV12,
            ColorDescription::BT709_YUV_LIMITED,
        )
        .unwrap()
    }
    fn coding() -> CodingRecord<'static> {
        CodingRecord::new(CodingId::new(0x1234), 7, &[8, 9])
    }

    #[test]
    fn sparse_joint_units_share_coding_and_derive_chroma_edges() {
        let mut selected = [0; 12];
        UnitSelectionEncoding::List
            .encode_into(6, &[0, 2, 5], &mut selected)
            .unwrap();
        let mut ranges = [0; 16];
        UnitIndexEncoding::Offsets
            .encode_into(&[3, 2, 3], &mut ranges)
            .unwrap();
        let data = [1, 2, 3, 4, 5, 6, 7, 8];
        let group = UnitGroup::builder(surface(), coding(), &data)
            .with_tiles(2, 2)
            .with_selection(UnitSelection::list(6, &selected).unwrap())
            .with_index(UnitIndex::offsets(&ranges).unwrap())
            .with_reference(ReferenceMode::Previous)
            .build()
            .unwrap();
        assert_eq!(group.len(), 3);
        assert_eq!(group.grid().len(), 6);
        assert!(group.cell(1).is_none());
        assert_eq!(group.get(2), group.cell(5));
        let edge = group.get(2).unwrap();
        assert_eq!(edge.region(), Region::new(4, 2, 1, 1).unwrap());
        assert_eq!(edge.plane_region(1), Some(Region::new(2, 1, 1, 1).unwrap()));
        assert_eq!(edge.plane_region(2), None);
        assert_eq!(edge.data_range(), 5..8);
        assert_eq!(edge.data(), &[6, 7, 8]);
        assert_eq!(edge.data().as_ptr(), data[5..].as_ptr());
        assert_eq!(edge.coding(), coding());
        assert_eq!(edge.reference(), ReferenceMode::Previous);
        assert!(group.iter().map(|unit| unit.cell()).eq([0, 2, 5]));
        assert_eq!(group.iter().nth_back(1), group.get(1));
        assert_eq!(group.iter().nth(2), group.get(2));
        assert_eq!(group.iter().count(), 3);
        assert_eq!(group.iter().last(), group.get(2));
        let mut exhausted = group.iter();
        assert_eq!(exhausted.nth(usize::MAX), None);
        assert_eq!(exhausted.next_back(), None);
    }

    #[test]
    fn only_selected_planes_constrain_joint_boundaries() {
        assert!(matches!(
            UnitGroup::builder(surface(), coding(), &[1; 6])
                .with_tiles(3, 1)
                .build(),
            Err(UnitGroupError::Region(_))
        ));
        let luma = UnitGroup::builder(surface(), coding(), &[1; 6])
            .with_tiles(3, 1)
            .with_planes(GroupPlanes::Joint(1))
            .build()
            .unwrap();
        assert_eq!(luma.len(), 6);
        assert_eq!(
            luma.get(5).unwrap().region(),
            Region::new(3, 2, 2, 1).unwrap()
        );
        assert_eq!(luma.get(5).unwrap().plane_region(1), None);
        let chroma = UnitGroup::builder(surface(), coding(), &[1; 6])
            .with_tiles(1, 1)
            .with_planes(GroupPlanes::Plane(1))
            .build()
            .unwrap();
        assert_eq!((chroma.grid().width(), chroma.grid().height()), (3, 2));
        assert_eq!(
            chroma.get(5).unwrap().region(),
            Region::new(2, 1, 1, 1).unwrap()
        );
        assert_eq!(
            chroma.get(5).unwrap().plane_region(1),
            Some(Region::new(2, 1, 1, 1).unwrap())
        );
        assert_eq!(chroma.get(5).unwrap().plane_region(0), None);
        for planes in [
            GroupPlanes::Joint(0),
            GroupPlanes::Joint(4),
            GroupPlanes::Plane(2),
        ] {
            assert_eq!(
                UnitGroup::builder(surface(), coding(), &[1])
                    .with_planes(planes)
                    .build(),
                Err(UnitGroupError::InvalidPlanes(planes))
            );
        }
    }

    #[test]
    fn omitted_maps_and_empty_groups_have_unambiguous_ranges() {
        let whole = UnitGroup::builder(surface(), coding(), &[1, 2, 3])
            .build()
            .unwrap();
        assert_eq!(whole.len(), 1);
        assert_eq!(
            whole.get(0).unwrap().region(),
            Region::new(0, 0, 5, 3).unwrap()
        );
        assert_eq!(whole.get(0).unwrap().data_range(), 0..3);
        assert_eq!(
            whole.get(0).unwrap().reference(),
            ReferenceMode::Independent
        );
        let empty = UnitGroup::builder(surface(), coding(), &[])
            .with_selection(UnitSelection::list(1, &[]).unwrap())
            .build()
            .unwrap();
        assert!(empty.is_empty());
        assert_eq!(empty.get(0), None);
        let empty_surface =
            SurfaceDescriptor::new(0, u32::MAX, SampleLayout::A8, ColorDescription::NONE).unwrap();
        assert!(
            UnitGroup::builder(empty_surface, coding(), &[])
                .build()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            UnitGroup::builder(empty_surface, coding(), &[1]).build(),
            Err(UnitGroupError::UnreferencedData)
        );
        assert_eq!(
            UnitGroup::builder(surface(), coding(), &[]).build(),
            Err(UnitGroupError::EmptyUnit { ordinal: 0 })
        );
        assert!(matches!(
            UnitGroup::builder(surface(), coding(), &[1])
                .with_selection(UnitSelection::all(2).unwrap())
                .build(),
            Err(UnitGroupError::CellCountMismatch { .. })
        ));
        assert!(matches!(
            UnitGroup::builder(surface(), coding(), &[1])
                .with_index(UnitIndex::fixed(2, 1).unwrap())
                .build(),
            Err(UnitGroupError::UnitCountMismatch { .. })
        ));
        assert!(matches!(
            UnitGroup::builder(surface(), coding(), &[1])
                .with_index(UnitIndex::fixed(1, 2).unwrap())
                .build(),
            Err(UnitGroupError::DataLengthMismatch { .. })
        ));
        assert_eq!(
            UnitGroup::builder(surface(), coding(), &[1; 7])
                .with_tiles(2, 2)
                .build(),
            Err(UnitGroupError::UnequalFixedUnits)
        );
    }

    #[test]
    fn aligned_offsets_never_imply_aligned_input_addresses() {
        #[repr(align(64))]
        struct Buffer([u8; 130]);
        let data = Buffer([1; 130]);
        let surface =
            SurfaceDescriptor::new(2, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let aligned = UnitGroup::builder(surface, coding(), &data.0[..128])
            .with_tiles(1, 1)
            .with_input_alignment(64)
            .build()
            .unwrap();
        assert!(aligned.data_addresses_are_aligned());
        assert!(aligned.iter().all(DecodeUnitRef::data_address_is_aligned));
        let unaligned = UnitGroup::builder(surface, coding(), &data.0[1..129])
            .with_tiles(1, 1)
            .with_input_alignment(64)
            .build()
            .unwrap();
        assert!(!unaligned.data_addresses_are_aligned());
        assert!(!unaligned.get(1).unwrap().data_address_is_aligned());
        assert_eq!(unaligned.get(1).unwrap().data_range(), 64..128);
        assert!(matches!(
            UnitGroup::builder(surface, coding(), &data.0[..126])
                .with_tiles(1, 1)
                .with_input_alignment(64)
                .build(),
            Err(UnitGroupError::UnalignedUnit {
                ordinal: 1,
                offset: 63,
                alignment: 64
            })
        ));
        assert_eq!(
            UnitGroup::builder(surface, coding(), &[1])
                .with_input_alignment(3)
                .build(),
            Err(UnitGroupError::InvalidAlignment(3))
        );
    }
}
