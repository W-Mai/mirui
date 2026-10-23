use core::ops::Range;

use super::{
    EncodedImageError,
    groups::{CodingRecords, GroupMetadata, GroupRecords},
};
use crate::{
    ByteAlignment,
    image::{
        CoverageBudget, DecodeUnitRef, EncodedGroupError, GroupPlanes, Region, RegionError,
        SurfaceDescriptor,
        units::{GroupLayout, RegionCells},
    },
    media::{CodingRecord, CodingTable},
};

#[cfg(test)]
mod tests;

/// Encoded IMAGE addressing from separate metadata windows, without DATA storage.
///
/// Directory, palette, metadata checksum and DATA integrity checks remain the caller's responsibility. Group coverage is checked by `validate_groups` with an explicit work budget; binding a unit checks its byte length, not its coding syntax or checksum. Decoding uses the existing `DecodeUnitRef` API after the caller reads that unit's complete input range.
///
/// ```
/// use mirx::{coding::Rle, image::{
///     ColorDescription, CoverageBudget, EncodedImageAsset, EncodedImageView, SampleLayout,
///     SurfaceDescriptor, SurfaceRequirements,
/// }};
/// let surface = SurfaceDescriptor::new(2, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
/// let payload = EncodedImageAsset::new(surface, Rle::new().record(), &[0x81, 42]).encode().unwrap();
/// let image = EncodedImageView::open(&payload).unwrap().layout();
/// image.validate_groups(&mut CoverageBudget::new(100)).unwrap();
/// let group = image.group(0).unwrap();
/// assert_eq!(group.unit_range(0), Some(0..2));
/// // The caller reads and verifies this range before binding its input.
/// let input = [0x81, 42];
/// let unit = group.unit(0, &input).unwrap();
/// let mut output = [0; 2];
/// unit.decode_plan(SurfaceRequirements::new()).unwrap().decode_into(&mut output).unwrap();
/// assert_eq!(output, [42, 42]);
/// ```
#[derive(Clone, Copy, Debug)]
pub struct EncodedImageLayout<'a> {
    pub(super) metadata: GroupMetadata<'a>,
}

impl<'a> EncodedImageLayout<'a> {
    /// Opens the coding, optional group and optional index bodies against a declared DATA length. `data_file_offset` is the absolute DATA start when file alignment must be checked. No DATA bytes are allocated, read or borrowed.
    pub fn new(
        surface: SurfaceDescriptor,
        codings: &'a [u8],
        groups: Option<&'a [u8]>,
        indexes: Option<&'a [u8]>,
        data_len: u32,
        data_file_offset: Option<u32>,
    ) -> Result<Self, EncodedImageError> {
        if indexes.is_some_and(|bytes| bytes.is_empty()) {
            return Err(EncodedImageError::EmptyIndexSection);
        }
        if groups.is_none() && indexes.is_some() {
            return Err(EncodedImageError::AmbiguousImplicitGroup);
        }
        if let Some(offset) = data_file_offset {
            offset
                .checked_add(data_len)
                .ok_or(EncodedImageError::SizeOverflow)?;
        }
        let metadata = GroupMetadata {
            surface,
            codings: CodingRecords::Wire(
                CodingTable::open(codings).map_err(EncodedImageError::Codings)?,
            ),
            records: groups.map_or(GroupRecords::Implicit, GroupRecords::Wire),
            indexes: indexes.unwrap_or(&[]),
            data_len,
            file_offset: data_file_offset,
            data_offset: 0,
        };
        metadata.validate_tables()?;
        Ok(Self { metadata })
    }

    pub(super) const fn from_metadata(metadata: GroupMetadata<'a>) -> Self {
        Self { metadata }
    }

    pub const fn surface(self) -> SurfaceDescriptor {
        self.metadata.surface
    }
    pub fn group_count(self) -> usize {
        self.metadata.group_count()
    }

    /// Checks canonical group/index ranges, file alignment and exact static coverage using the same rules as resident images. Coding support, syntax and DATA integrity remain separate checks.
    pub fn validate_groups(self, budget: &mut CoverageBudget) -> Result<(), EncodedImageError> {
        self.metadata.validate_groups(budget)
    }

    pub fn input_alignment(self) -> Result<ByteAlignment, EncodedImageError> {
        self.metadata.input_alignment()
    }

    /// Resolves one group's geometry and ranges without reading its encoded input.
    pub fn group(self, index: usize) -> Result<ImageGroupLayout<'a>, EncodedImageError> {
        let record = self.metadata.record(index)?;
        let layout = self.metadata.resolve_record(index, record)?.0;
        Ok(ImageGroupLayout {
            index,
            base: record.data_range().start,
            layout,
        })
    }
}

/// One checked group's addressing metadata. A borrowed unit input is bound only for its decode call.
#[derive(Clone, Copy, Debug)]
pub struct ImageGroupLayout<'a> {
    index: usize,
    base: u32,
    layout: GroupLayout<'a>,
}

impl<'a> ImageGroupLayout<'a> {
    pub const fn surface(self) -> SurfaceDescriptor {
        self.layout.surface
    }
    pub const fn planes(self) -> GroupPlanes {
        self.layout.planes
    }
    pub const fn coding(self) -> CodingRecord<'a> {
        self.layout.coding
    }
    pub const fn len(self) -> usize {
        self.layout.selection.len()
    }
    pub const fn is_empty(self) -> bool {
        self.layout.selection.is_empty()
    }
    pub const fn input_alignment(self) -> ByteAlignment {
        self.layout.input_alignment
    }

    /// Complete encoded input range, relative to the DATA section, including neither adjacent units nor alignment padding.
    pub fn unit_range(self, ordinal: usize) -> Option<Range<u32>> {
        let range = self.layout.index.get(ordinal)?;
        Some(self.base + range.start..self.base + range.end)
    }

    /// Unit rectangle in surface coordinates for joint groups, or element coordinates for a planar group.
    pub fn unit_region(self, ordinal: usize) -> Option<Region> {
        self.layout
            .grid
            .get(self.layout.selection.get(ordinal)? as usize)
    }

    /// Stored ordinals intersecting the requested grid-space rectangle. Reuses sparse spatial selection without allocating or scanning DATA.
    pub fn units_in(self, region: Region) -> Result<impl Iterator<Item = usize> + 'a, RegionError> {
        Ok(
            RegionCells::new(self.layout.grid, self.layout.selection, region)?
                .map(move |cell| self.layout.selection.position(cell).expect("selected cell")),
        )
    }

    /// Binds exactly one unit's input to the existing decode interface. The returned unit borrows both metadata and input; source I/O and checksum validation must finish before this call.
    ///
    /// ```compile_fail
    /// use mirx::image::{DecodeUnitRef, ImageGroupLayout};
    /// fn escape<'a>(group: ImageGroupLayout<'a>) -> DecodeUnitRef<'a> {
    ///     let input = [0x81, 42];
    ///     group.unit(0, &input).unwrap()
    /// }
    /// ```
    pub fn unit(
        self,
        ordinal: usize,
        bytes: &'a [u8],
    ) -> Result<DecodeUnitRef<'a>, EncodedImageError> {
        self.layout
            .bind_unit(ordinal, bytes)
            .ok_or(EncodedImageError::UnitOutOfBounds {
                group: self.index,
                ordinal,
            })?
            .map_err(|error| EncodedImageError::Group {
                index: self.index,
                error: EncodedGroupError::Group(error),
            })
    }
}
