use super::groups::{CodingRecords, GroupRecords, GroupSource};
use super::preflight::Preflight;
use alloc::vec::Vec;

use crate::{
    ByteAlignment,
    coding::CodingId,
    image::{
        GroupPlanes, GroupSelection, ImageEncodeError, ReferenceMode, SURFACE_RECORD_LEN,
        SurfaceDescriptor, UNIT_GROUP_RECORD_LEN, UnitGroup, UnitGroupRecord,
    },
    media::{
        CODING_RECORD_LEN, CodingRecord, CodingTable, DataIntegrity, INTEGRITY_RECORD_LEN,
        IntegrityRange, MEDIA_HEADER_LEN, MEDIA_SECTION_LEN, MEDIA_VERSION, MediaFlags,
        MediaSectionKind, UnitIndex, UnitIndexEncoding, UnitSelectionEncoding,
        index::UNIT_CHECKPOINT_INTERVAL, output::PayloadOutput,
        selection::SELECTION_CHECKPOINT_INTERVAL,
    },
    wire::write_u16_le,
};

#[cfg(test)]
mod tests;

/// Borrowed single-stream or grouped IMAGE authoring input.
///
/// Metadata validation does not decode supplied bytes or imply codec support.
/// Unknown nonzero coding identifiers remain representable. Profile syntax is
/// checked separately by [`Self::preflight`] or a resolved unit's decode plan.
///
/// ```
/// use mirx::{coding::Rle, image::{
///     ColorDescription, EncodedImageAsset, EncodedImageView, SampleLayout, SurfaceDescriptor,
/// }};
///
/// let surface = SurfaceDescriptor::new(4, 2, SampleLayout::A8, ColorDescription::NONE).unwrap();
/// let codec = Rle::new();
/// let mut stream = [0; 16];
/// let len = codec.encode_into(&[42; 8], &mut stream).unwrap();
/// let asset = EncodedImageAsset::new(surface, codec.record(), &stream[..len]);
/// let mut payload = [0; 128];
/// let size = asset.encode_into(&mut payload).unwrap();
/// let image = EncodedImageView::open(&payload[..size]).unwrap();
/// assert_eq!(image.surface(), surface);
/// image.validate_data().unwrap();
/// ```
#[derive(Clone, Copy, Debug)]
pub struct EncodedImageAsset<'a> {
    surface: SurfaceDescriptor,
    codings: AssetCodings<'a>,
    groups: AssetGroups<'a>,
    indexes: &'a [u8],
    integrity: DataIntegrity<'a>,
    data: &'a [u8],
    color_table: Option<&'a [u8]>,
    input_alignment: ByteAlignment,
}

#[derive(Clone, Copy, Debug)]
enum AssetCodings<'a> {
    Single(CodingRecord<'a>),
    Shared(&'a [CodingRecord<'a>]),
    Table(CodingTable<'a>),
    Groups(&'a [UnitGroup<'a>]),
}

#[derive(Clone, Copy, Debug)]
enum AssetGroups<'a> {
    None,
    Authored(&'a [UnitGroup<'a>]),
    Records(&'a [UnitGroupRecord]),
}

#[derive(Clone, Copy)]
struct AuthoredGroupPlan {
    record: UnitGroupRecord,
    selection: Option<UnitSelectionEncoding>,
    ranges: Option<UnitIndexEncoding>,
    selection_len: usize,
    range_len: usize,
}

impl AuthoredGroupPlan {
    fn new(
        surface: SurfaceDescriptor,
        codings: CodingRecords<'_>,
        group_index: usize,
        group: UnitGroup<'_>,
        data_offset: u32,
        index_offset: u32,
    ) -> Result<Self, ImageEncodeError> {
        if group.surface() != surface {
            return Err(ImageEncodeError::Preflight(
                super::EncodedImageError::Coverage(crate::image::CoverageError::SurfaceMismatch),
            ));
        }
        if group.is_empty() {
            return Err(ImageEncodeError::Preflight(
                super::EncodedImageError::EmptyGroup(group_index),
            ));
        }
        let coding_index = codings
            .index_of(group.coding())
            .ok_or(ImageEncodeError::SizeOverflow)?;
        let selection = Self::selection_encoding(group)?;
        let selection_len = selection.map_or(0, |encoding| {
            encoding
                .table_len(group.grid().len() as u32, group.len())
                .expect("validated selection size")
        });
        let ranges = Self::range_encoding(group)?;
        let range_len = ranges.map_or(0, |encoding| {
            encoding
                .table_len(group.len())
                .expect("validated range size")
        });
        let data_end = data_offset
            .checked_add(
                u32::try_from(group.data().len()).map_err(|_| ImageEncodeError::SizeOverflow)?,
            )
            .ok_or(ImageEncodeError::SizeOverflow)?;
        let mut record = UnitGroupRecord::new(coding_index, data_offset..data_end)
            .map_err(|_| ImageEncodeError::SizeOverflow)?
            .with_input_alignment(group.input_alignment())
            .with_reference(group.reference());
        if group.grid().len() != 1 {
            record = record.with_tiles(group.grid().tile_width(), group.grid().tile_height());
        }
        let all_planes = GroupPlanes::Joint((1 << surface.plane_count()) - 1);
        if group.planes() != all_planes {
            record = record.with_planes(group.planes());
        }
        if let Some(encoding) = selection {
            record = record
                .with_selection(match encoding {
                    UnitSelectionEncoding::List => GroupSelection::List(group.len() as u32),
                    UnitSelectionEncoding::Bitmap => GroupSelection::Bitmap,
                })
                .with_index_offset(index_offset);
        }
        if let Some(encoding) = ranges {
            record = record
                .with_index_encoding(encoding)
                .with_index_offset(index_offset);
        }
        Ok(Self {
            record,
            selection,
            ranges,
            selection_len,
            range_len,
        })
    }

    fn selection_encoding(
        group: UnitGroup<'_>,
    ) -> Result<Option<UnitSelectionEncoding>, ImageEncodeError> {
        if group.len() == group.grid().len() {
            return Ok(None);
        }
        let list = UnitSelectionEncoding::List
            .table_len(group.grid().len() as u32, group.len())
            .map_err(|_| ImageEncodeError::SizeOverflow)?;
        let bitmap = UnitSelectionEncoding::Bitmap
            .table_len(group.grid().len() as u32, group.len())
            .map_err(|_| ImageEncodeError::SizeOverflow)?;
        Ok(Some(if bitmap < list {
            UnitSelectionEncoding::Bitmap
        } else {
            UnitSelectionEncoding::List
        }))
    }

    fn range_encoding(group: UnitGroup<'_>) -> Result<Option<UnitIndexEncoding>, ImageEncodeError> {
        let mut ranges = group.index().iter();
        let Some(first) = ranges.next() else {
            return Ok(None);
        };
        let first_len = first.end - first.start;
        let mut fixed = true;
        let mut adjacent = true;
        let mut fits_u16 = first_len <= u32::from(u16::MAX);
        let mut previous_end = first.end;
        for range in ranges {
            let len = range.end - range.start;
            fixed &= len == first_len;
            adjacent &= range.start == previous_end;
            fits_u16 &= len <= u32::from(u16::MAX);
            previous_end = range.end;
        }
        if fixed {
            return Ok(None);
        }
        let mut best = UnitIndexEncoding::Lengths32;
        let mut best_len = best
            .table_len(group.len())
            .map_err(|_| ImageEncodeError::SizeOverflow)?;
        if fits_u16 {
            let len = UnitIndexEncoding::Lengths16
                .table_len(group.len())
                .map_err(|_| ImageEncodeError::SizeOverflow)?;
            if len <= best_len {
                best = UnitIndexEncoding::Lengths16;
                best_len = len;
            }
        }
        if adjacent {
            let len = UnitIndexEncoding::Offsets
                .table_len(group.len())
                .map_err(|_| ImageEncodeError::SizeOverflow)?;
            if len < best_len {
                best = UnitIndexEncoding::Offsets;
            }
        }
        Ok(Some(best))
    }

    fn index_len(self) -> usize {
        self.selection_len + self.range_len
    }
}

struct AuthoredGroupPlans<'a> {
    asset: EncodedImageAsset<'a>,
    position: usize,
    data_end: u32,
    index_end: u32,
}

impl Iterator for AuthoredGroupPlans<'_> {
    type Item = Result<AuthoredGroupPlan, ImageEncodeError>;

    fn next(&mut self) -> Option<Self::Item> {
        let groups = self.asset.authored_groups()?;
        let &group = groups.get(self.position)?;
        let data_start = match UnitIndex::aligned(self.data_end, group.input_alignment()) {
            Ok(start) => start,
            Err(_) => return Some(Err(ImageEncodeError::SizeOverflow)),
        };
        let plan = AuthoredGroupPlan::new(
            self.asset.surface,
            self.asset.codings.source(),
            self.position,
            group,
            data_start,
            self.index_end,
        );
        self.position += 1;
        match plan {
            Ok(plan) => {
                self.data_end = plan.record.data_range().end;
                match u32::try_from(plan.index_len())
                    .ok()
                    .and_then(|len| self.index_end.checked_add(len))
                {
                    Some(end) => self.index_end = end,
                    None => return Some(Err(ImageEncodeError::SizeOverflow)),
                }
                Some(Ok(plan))
            }
            Err(error) => Some(Err(error)),
        }
    }
}

impl<'a> AssetCodings<'a> {
    fn source(&self) -> CodingRecords<'_> {
        match self {
            Self::Single(record) => CodingRecords::Native(core::slice::from_ref(record)),
            Self::Shared(records) => CodingRecords::Native(records),
            Self::Table(table) => CodingRecords::Wire(*table),
            Self::Groups(groups) => CodingRecords::Groups(groups),
        }
    }
}

impl<'a> EncodedImageAsset<'a> {
    /// Defines one whole-surface stream with an omitted group table by default.
    pub const fn new(surface: SurfaceDescriptor, coding: CodingRecord<'a>, data: &'a [u8]) -> Self {
        Self {
            surface,
            codings: AssetCodings::Single(coding),
            groups: AssetGroups::None,
            indexes: &[],
            integrity: DataIntegrity::Whole,
            data,
            color_table: None,
            input_alignment: ByteAlignment::ONE,
        }
    }

    /// Borrows validated semantic groups and chooses their canonical wire tables.
    pub const fn from_groups(surface: SurfaceDescriptor, groups: &'a [UnitGroup<'a>]) -> Self {
        Self {
            surface,
            codings: AssetCodings::Groups(groups),
            groups: AssetGroups::Authored(groups),
            indexes: &[],
            integrity: DataIntegrity::Whole,
            data: &[],
            color_table: None,
            input_alignment: ByteAlignment::ONE,
        }
    }

    pub(crate) const fn from_records(
        surface: SurfaceDescriptor,
        codings: &'a [CodingRecord<'a>],
        groups: &'a [UnitGroupRecord],
        data: &'a [u8],
    ) -> Self {
        Self {
            surface,
            codings: AssetCodings::Shared(codings),
            groups: AssetGroups::Records(groups),
            indexes: &[],
            integrity: DataIntegrity::Whole,
            data,
            color_table: None,
            input_alignment: ByteAlignment::ONE,
        }
    }

    pub(crate) const fn from_codings(
        surface: SurfaceDescriptor,
        codings: CodingTable<'a>,
        data: &'a [u8],
    ) -> Self {
        Self {
            surface,
            codings: AssetCodings::Table(codings),
            groups: AssetGroups::None,
            indexes: &[],
            integrity: DataIntegrity::Whole,
            data,
            color_table: None,
            input_alignment: ByteAlignment::ONE,
        }
    }

    pub(crate) const fn with_groups(mut self, groups: &'a [UnitGroupRecord]) -> Self {
        self.groups = AssetGroups::Records(groups);
        self
    }

    pub(crate) const fn with_unit_index(mut self, bytes: &'a [u8]) -> Self {
        self.indexes = bytes;
        self
    }
    /// Selects whole-DATA or caller-partitioned CRC coverage; no checksum is supplied manually.
    pub const fn with_integrity(mut self, integrity: DataIntegrity<'a>) -> Self {
        self.integrity = integrity;
        self
    }
    pub const fn integrity(self) -> DataIntegrity<'a> {
        self.integrity
    }
    pub const fn with_color_table(mut self, rgba: &'a [u8]) -> Self {
        self.color_table = Some(rgba);
        self
    }
    /// Sets single-stream alignment. Explicit groups declare their own alignment.
    /// A nondefault override combined with explicit groups is rejected.
    pub const fn with_input_alignment(mut self, alignment: ByteAlignment) -> Self {
        self.input_alignment = alignment;
        self
    }
    pub const fn surface(self) -> SurfaceDescriptor {
        self.surface
    }
    pub fn codings(
        &self,
    ) -> impl ExactSizeIterator<Item = CodingRecord<'_>>
    + DoubleEndedIterator
    + core::iter::FusedIterator
    + Clone {
        self.codings.source().iter()
    }
    #[cfg(test)]
    pub(crate) const fn group_records(self) -> Option<&'a [UnitGroupRecord]> {
        match self.groups {
            AssetGroups::Records(groups) => Some(groups),
            _ => None,
        }
    }
    #[cfg(test)]
    pub(crate) const fn unit_index(self) -> &'a [u8] {
        self.indexes
    }
    #[cfg(test)]
    pub(crate) const fn data(self) -> &'a [u8] {
        self.data
    }
    pub const fn color_table(self) -> Option<&'a [u8]> {
        self.color_table
    }

    fn authored_groups(self) -> Option<&'a [UnitGroup<'a>]> {
        match self.groups {
            AssetGroups::Authored(groups) => Some(groups),
            _ => None,
        }
    }

    fn group_plans(self) -> AuthoredGroupPlans<'a> {
        AuthoredGroupPlans {
            asset: self,
            position: 0,
            data_end: 0,
            index_end: 0,
        }
    }

    fn data_len(self) -> Result<usize, ImageEncodeError> {
        let Some(groups) = self.authored_groups() else {
            return Ok(self.data.len());
        };
        let mut end = 0u32;
        for group in groups {
            let start = UnitIndex::aligned(end, group.input_alignment())
                .map_err(|_| ImageEncodeError::SizeOverflow)?;
            end = start
                .checked_add(
                    u32::try_from(group.data().len())
                        .map_err(|_| ImageEncodeError::SizeOverflow)?,
                )
                .ok_or(ImageEncodeError::SizeOverflow)?;
        }
        Ok(end as usize)
    }

    fn index_len(self) -> Result<usize, ImageEncodeError> {
        if self.authored_groups().is_none() {
            return Ok(self.indexes.len());
        }
        self.group_plans().try_fold(0usize, |len, plan| {
            len.checked_add(plan?.index_len())
                .ok_or(ImageEncodeError::SizeOverflow)
        })
    }

    fn write_indexes(self, output: &mut PayloadOutput<'_>) {
        let Some(groups) = self.authored_groups() else {
            output.write(self.indexes);
            return;
        };
        for (&group, plan) in groups.iter().zip(self.group_plans()) {
            let plan = plan.expect("validated authored group");
            if let Some(encoding) = plan.selection {
                match encoding {
                    UnitSelectionEncoding::List => {
                        for cell in group.selection().iter() {
                            output.write(&cell.to_le_bytes());
                        }
                    }
                    UnitSelectionEncoding::Bitmap => {
                        let cells = group.grid().len();
                        for block in 0..cells.div_ceil(SELECTION_CHECKPOINT_INTERVAL) {
                            let before = (block * SELECTION_CHECKPOINT_INTERVAL) as u32;
                            let count = group.selection().range(0..before).unwrap().len() as u32;
                            output.write(&count.to_le_bytes());
                        }
                        for byte_index in 0..cells.div_ceil(8) {
                            let mut bits = 0u8;
                            for bit in 0..8 {
                                let cell = byte_index * 8 + bit;
                                if cell < cells && group.selection().contains(cell as u32) {
                                    bits |= 1 << bit;
                                }
                            }
                            output.write(&[bits]);
                        }
                    }
                }
            }
            if let Some(encoding) = plan.ranges {
                match encoding {
                    UnitIndexEncoding::Offsets => {
                        output.write(&0u32.to_le_bytes());
                        for range in group.index().iter() {
                            output.write(&range.end.to_le_bytes());
                        }
                    }
                    UnitIndexEncoding::Lengths16 | UnitIndexEncoding::Lengths32 => {
                        for (ordinal, range) in group.index().iter().enumerate() {
                            if ordinal % UNIT_CHECKPOINT_INTERVAL == 0 {
                                output.write(&range.start.to_le_bytes());
                            }
                        }
                        for range in group.index().iter() {
                            let len = range.end - range.start;
                            match encoding {
                                UnitIndexEncoding::Lengths16 => {
                                    output.write(&(len as u16).to_le_bytes());
                                }
                                UnitIndexEncoding::Lengths32 => output.write(&len.to_le_bytes()),
                                UnitIndexEncoding::Offsets => unreachable!(),
                            }
                        }
                    }
                }
            }
        }
    }

    fn write_data(self, output: &mut PayloadOutput<'_>) {
        let Some(groups) = self.authored_groups() else {
            output.write(self.data);
            return;
        };
        let base = output.position();
        let mut end = 0u32;
        for group in groups {
            let start = UnitIndex::aligned(end, group.input_alignment())
                .expect("validated authored DATA layout");
            output.pad_to(base + start as usize);
            output.write(group.data());
            end = start + group.data().len() as u32;
        }
    }

    fn data_crc(self, range: core::ops::Range<u32>) -> u32 {
        let Some(groups) = self.authored_groups() else {
            return crate::crc32(&self.data[range.start as usize..range.end as usize]);
        };
        let mut crc = crate::crc32::Crc32::new();
        let mut end = 0u32;
        const ZERO: [u8; 64] = [0; 64];
        for group in groups {
            let start = UnitIndex::aligned(end, group.input_alignment())
                .expect("validated authored DATA layout");
            let gap_start = end.max(range.start);
            let gap_end = start.min(range.end);
            let mut gap = gap_end.saturating_sub(gap_start) as usize;
            while gap != 0 {
                let size = gap.min(ZERO.len());
                crc.update(&ZERO[..size]);
                gap -= size;
            }
            let group_end = start + group.data().len() as u32;
            let data_start = start.max(range.start);
            let data_end = group_end.min(range.end);
            if data_start < data_end {
                crc.update(
                    &group.data()[(data_start - start) as usize..(data_end - start) as usize],
                );
            }
            end = group_end;
            if end >= range.end {
                break;
            }
        }
        crc.finish()
    }
    /// Checked DATA alignment, derived from all groups; empty implicit streams need one.
    pub fn input_alignment(self) -> Result<ByteAlignment, ImageEncodeError> {
        Ok(Plan::metadata(self)?.alignment)
    }

    /// Exact payload size after metadata validation, without decoding samples.
    pub fn encoded_len(self) -> Result<usize, ImageEncodeError> {
        Ok(Plan::new(self)?.payload_len)
    }

    /// Validates metadata, scalar syntax and resource limits before allocation.
    ///
    /// Work includes the equivalent encoded reader's group/unit checks and the
    /// canonical output span, including padding. No payload, decoded samples
    /// or group table is allocated. Low-level encoding alone checks metadata,
    /// not profile support; this explicit gate admits typed document edits.
    pub fn preflight(self, limits: &crate::PayloadLimits) -> Result<(), ImageEncodeError> {
        let mut preflight = Preflight::new(limits, 0).map_err(ImageEncodeError::Preflight)?;
        let group_count = match self.groups {
            AssetGroups::Authored(groups) => groups.len(),
            AssetGroups::Records(groups) => groups.len(),
            AssetGroups::None => 1,
        };
        let minimum = (self.codings().len() as u64)
            .checked_mul(CODING_RECORD_LEN as u64)
            .and_then(|len| len.checked_add(group_count as u64 * UNIT_GROUP_RECORD_LEN as u64))
            .and_then(|len| {
                (self.integrity.partitions().map_or(0, <[u32]>::len) as u64)
                    .checked_mul(INTEGRITY_RECORD_LEN as u64)
                    .and_then(|size| len.checked_add(size))
            })
            .ok_or(ImageEncodeError::SizeOverflow)?;
        if minimum > limits.max_raster_work() {
            return Err(ImageEncodeError::Preflight(
                super::EncodedImageError::Coverage(crate::image::CoverageError::BudgetExceeded),
            ));
        }
        let plan = Plan::metadata(self)?;
        let storage = plan.storage();
        preflight
            .spend(plan.payload_len as u64)
            .map_err(ImageEncodeError::Preflight)?;
        storage.preflight_in(&mut preflight)?;
        preflight
            .spend(plan.data_len as u64)
            .map_err(ImageEncodeError::Preflight)
    }

    /// Writes canonical metadata and supplied DATA after all bounds are checked.
    /// Errors preserve output; success preserves its unused suffix.
    pub fn encode_into(self, output: &mut [u8]) -> Result<usize, ImageEncodeError> {
        let plan = Plan::new(self)?;
        if output.len() < plan.payload_len {
            return Err(ImageEncodeError::BufferTooSmall {
                needed: plan.payload_len,
                available: output.len(),
            });
        }
        let len = plan.payload_len;
        plan.emit(PayloadOutput::buffer(&mut output[..len]));
        Ok(len)
    }
    /// Allocates exactly one payload; its backing pointer has no extra alignment promise.
    pub fn encode(self) -> Result<Vec<u8>, ImageEncodeError> {
        let plan = Plan::new(self)?;
        let mut output = Vec::new();
        output
            .try_reserve_exact(plan.payload_len)
            .map_err(|_| ImageEncodeError::AllocationFailed)?;
        output.resize(plan.payload_len, 0);
        plan.emit(PayloadOutput::buffer(&mut output));
        Ok(output)
    }
    /// Compares complete canonical bytes, including metadata and DATA checksums.
    pub fn matches_payload(self, payload: &[u8]) -> Result<bool, ImageEncodeError> {
        let plan = Plan::new(self)?;
        Ok(payload.len() == plan.payload_len && plan.emit(PayloadOutput::comparison(payload)))
    }
}

struct Plan<'a> {
    asset: EncodedImageAsset<'a>,
    coding_len: usize,
    integrity_len: usize,
    section_count: u16,
    group: Option<UnitGroupRecord>,
    alignment: ByteAlignment,
    index_len: usize,
    data_len: usize,
    data_offset: usize,
    payload_len: usize,
}
impl<'a> Plan<'a> {
    fn new(asset: EncodedImageAsset<'a>) -> Result<Self, ImageEncodeError> {
        let plan = Self::metadata(asset)?;
        plan.storage().validate()?;
        Ok(plan)
    }

    fn metadata(asset: EncodedImageAsset<'a>) -> Result<Self, ImageEncodeError> {
        let storage = StoragePlan::new(asset)?;
        let group = storage.group;
        let alignment = storage.alignment;
        let coding_len = storage.coding_len;
        let index_len = storage.index_len;
        let data_len = storage.data_len;
        let integrity_len = asset
            .integrity
            .section_len(data_len)
            .map_err(ImageEncodeError::Integrity)?;
        let group_len = storage.group_len()?;
        let section_count = 3
            + u16::from(storage.has_group_section())
            + u16::from(index_len != 0)
            + u16::from(asset.integrity.partitions().is_some())
            + u16::from(asset.color_table.is_some());
        let metadata_len = (MEDIA_HEADER_LEN
            + usize::from(section_count) * MEDIA_SECTION_LEN
            + SURFACE_RECORD_LEN)
            .checked_add(coding_len)
            .and_then(|len| len.checked_add(group_len))
            .and_then(|len| len.checked_add(index_len))
            .and_then(|len| len.checked_add(asset.color_table.map_or(0, <[u8]>::len)))
            .and_then(|len| len.checked_add(integrity_len))
            .ok_or(ImageEncodeError::SizeOverflow)?;
        let data_offset = UnitIndex::aligned(
            u32::try_from(metadata_len).map_err(|_| ImageEncodeError::SizeOverflow)?,
            alignment,
        )
        .map_err(|_| ImageEncodeError::SizeOverflow)? as usize;
        let payload_len = data_offset
            .checked_add(data_len)
            .and_then(|len| len.checked_add(asset.integrity.trailer_len()))
            .ok_or(ImageEncodeError::SizeOverflow)?;
        u32::try_from(payload_len).map_err(|_| ImageEncodeError::SizeOverflow)?;
        Ok(Self {
            asset,
            coding_len,
            integrity_len,
            section_count,
            group,
            alignment,
            index_len,
            data_len,
            data_offset,
            payload_len,
        })
    }

    fn storage(&self) -> StoragePlan<'a> {
        StoragePlan {
            asset: self.asset,
            coding_len: self.coding_len,
            group: self.group,
            alignment: self.alignment,
            index_len: self.index_len,
            data_len: self.data_len,
        }
    }
    fn emit(self, mut output: PayloadOutput<'_>) -> bool {
        let mut header = [0; MEDIA_HEADER_LEN];
        header[0] = MEDIA_VERSION;
        if self.asset.integrity.partitions().is_some() {
            header[1] = MediaFlags::INDEXED_INTEGRITY.bits();
        }
        write_u16_le(&mut header, 2, self.section_count);
        output.header(&header);
        let surface_offset = MEDIA_HEADER_LEN + usize::from(self.section_count) * MEDIA_SECTION_LEN;
        output.section(
            MediaSectionKind::SURFACE,
            surface_offset,
            SURFACE_RECORD_LEN,
        );
        let mut offset = surface_offset + SURFACE_RECORD_LEN;
        output.section(MediaSectionKind::CODINGS, offset, self.coding_len);
        offset += self.coding_len;
        let storage = self.storage();
        if storage.has_group_section() {
            let size = storage.group_len().expect("validated group table size");
            output.section(MediaSectionKind::UNIT_GROUPS, offset, size);
            offset += size;
        }
        if self.index_len != 0 {
            output.section(MediaSectionKind::UNIT_INDEX, offset, self.index_len);
            offset += self.index_len;
        }
        if let Some(table) = self.asset.color_table {
            output.section(MediaSectionKind::COLOR_TABLE, offset, table.len());
            offset += table.len();
        }
        if self.asset.integrity.partitions().is_some() {
            output.section(MediaSectionKind::INTEGRITY, offset, self.integrity_len);
        }
        output.section(MediaSectionKind::DATA, self.data_offset, self.data_len);
        output.write(&self.asset.surface.encode_record());
        storage.emit_metadata(&mut output);
        if let Some(table) = self.asset.color_table {
            output.write(table);
        }
        let mut start = 0;
        for &end in self.asset.integrity.partitions().unwrap_or(&[]) {
            let checksum = self.asset.data_crc(start..end);
            let range = IntegrityRange::new(
                self.data_offset as u32 + start..self.data_offset as u32 + end,
                checksum,
            )
            .expect("validated integrity partition");
            output.write(&range.encode_record());
            start = end;
        }
        output.pad_to(self.data_offset);
        output.begin_data(self.asset.integrity);
        self.asset.write_data(&mut output);
        debug_assert_eq!(
            output.position() + self.asset.integrity.trailer_len(),
            self.payload_len
        );
        output.finish()
    }
}

/// Resource-independent coded storage, without an IMAGE header or SURFACE body.
#[derive(Clone, Copy)]
pub(crate) struct StoragePlan<'a> {
    asset: EncodedImageAsset<'a>,
    coding_len: usize,
    group: Option<UnitGroupRecord>,
    alignment: ByteAlignment,
    index_len: usize,
    data_len: usize,
}

impl<'a> StoragePlan<'a> {
    pub(crate) fn new(asset: EncodedImageAsset<'a>) -> Result<Self, ImageEncodeError> {
        asset
            .surface
            .read_color_table(asset.color_table)
            .map_err(ImageEncodeError::from)?;
        if matches!(asset.groups, AssetGroups::Authored([])) {
            return Err(ImageEncodeError::Preflight(
                super::EncodedImageError::InvalidGroupTableLength(0),
            ));
        }
        let group = if !matches!(asset.groups, AssetGroups::None) {
            if asset.input_alignment != ByteAlignment::ONE {
                return Err(ImageEncodeError::ConflictingAlignment);
            }
            None
        } else {
            let source = asset.codings.source();
            if source.len() != 1 {
                return Err(ImageEncodeError::Preflight(
                    super::EncodedImageError::AmbiguousImplicitGroup,
                ));
            }
            let coding = source.get(0).expect("single coding");
            if coding.id() == CodingId::RAW {
                return Err(ImageEncodeError::UnexpectedCoding(CodingId::RAW));
            }
            let group = UnitGroup::builder(asset.surface, coding, asset.data)
                .with_input_alignment(asset.input_alignment)
                .build()
                .map_err(ImageEncodeError::Group)?;
            (!group.is_empty() && asset.input_alignment != ByteAlignment::ONE).then(|| {
                UnitGroupRecord::new(0, 0..asset.data.len() as u32)
                    .expect("validated group range")
                    .with_input_alignment(asset.input_alignment)
            })
        };
        let coding_len = asset
            .codings
            .source()
            .encoded_len()
            .map_err(ImageEncodeError::Codings)?;
        let index_len = asset.index_len()?;
        let data_len = asset.data_len()?;
        asset
            .integrity
            .section_len(data_len)
            .map_err(ImageEncodeError::Integrity)?;
        let mut plan = Self {
            asset,
            coding_len,
            group,
            alignment: ByteAlignment::ONE,
            index_len,
            data_len,
        };
        plan.alignment = match plan.asset.groups {
            AssetGroups::Authored(groups) => {
                groups.iter().fold(ByteAlignment::ONE, |alignment, group| {
                    alignment.max(group.input_alignment())
                })
            }
            _ => plan
                .source()
                .input_alignment()
                .map_err(ImageEncodeError::Preflight)?,
        };
        Ok(plan)
    }

    fn records(&self) -> Option<&[UnitGroupRecord]> {
        match self.asset.groups {
            AssetGroups::Records(groups) => Some(groups),
            AssetGroups::None => self.group.as_ref().map(core::slice::from_ref),
            AssetGroups::Authored(_) => None,
        }
    }

    pub(crate) fn group_record_count(&self) -> usize {
        match self.asset.groups {
            AssetGroups::Authored(groups) => groups.len(),
            AssetGroups::Records(groups) => groups.len(),
            AssetGroups::None => usize::from(self.group.is_some()),
        }
    }

    fn has_group_section(&self) -> bool {
        match self.asset.groups {
            AssetGroups::Authored(_) | AssetGroups::Records(_) => true,
            AssetGroups::None => self.group.is_some(),
        }
    }

    fn group_len(&self) -> Result<usize, ImageEncodeError> {
        self.group_record_count()
            .checked_mul(UNIT_GROUP_RECORD_LEN)
            .ok_or(ImageEncodeError::SizeOverflow)
    }

    pub(crate) fn source(&self) -> GroupSource<'_> {
        debug_assert!(!matches!(self.asset.groups, AssetGroups::Authored(_)));
        GroupSource {
            surface: self.asset.surface,
            codings: self.asset.codings.source(),
            records: self
                .records()
                .map_or(GroupRecords::Implicit, GroupRecords::Native),
            data: self.asset.data,
            indexes: self.asset.indexes,
            file_offset: None,
            data_offset: 0,
        }
    }

    pub(crate) fn validate(&self) -> Result<(), ImageEncodeError> {
        if let Some(groups) = self.asset.authored_groups() {
            if groups.is_empty() {
                return Err(ImageEncodeError::Preflight(
                    super::EncodedImageError::InvalidGroupTableLength(0),
                ));
            }
            for (index, plan) in self.asset.group_plans().enumerate() {
                plan?;
                if groups[index].reference() != ReferenceMode::Independent {
                    return Err(ImageEncodeError::Preflight(
                        super::EncodedImageError::ReferenceInStaticImage(index),
                    ));
                }
            }
            Ok(())
        } else {
            self.source()
                .visit_groups(None, |_, _| {})
                .map_err(ImageEncodeError::Preflight)
        }
    }

    pub(crate) fn validate_with_references(&self) -> Result<(), ImageEncodeError> {
        if let Some(groups) = self.asset.authored_groups() {
            if groups.is_empty() {
                return Err(ImageEncodeError::Preflight(
                    super::EncodedImageError::InvalidGroupTableLength(0),
                ));
            }
            for plan in self.asset.group_plans() {
                plan?;
            }
            Ok(())
        } else {
            self.source()
                .visit_groups_with_references(None, |_, _| {})
                .map_err(ImageEncodeError::Preflight)
        }
    }

    pub(crate) fn preflight_in(
        &self,
        preflight: &mut Preflight<'_>,
    ) -> Result<(), ImageEncodeError> {
        if let Some(groups) = self.asset.authored_groups() {
            preflight
                .add_groups(groups.len())
                .map_err(ImageEncodeError::Preflight)?;
            preflight
                .native_groups(self.asset.surface, groups)
                .map_err(ImageEncodeError::Preflight)
        } else {
            let source = self.source();
            preflight
                .add_groups(source.group_count())
                .map_err(ImageEncodeError::Preflight)?;
            preflight
                .groups(source)
                .map_err(ImageEncodeError::Preflight)
        }
    }

    pub(crate) const fn alignment(self) -> ByteAlignment {
        self.alignment
    }

    pub(crate) const fn data_len(self) -> usize {
        self.data_len
    }

    pub(crate) const fn coding_len(self) -> usize {
        self.coding_len
    }

    pub(crate) const fn index_len(self) -> usize {
        self.index_len
    }

    pub(crate) fn visit_group_records(
        &self,
        mut visit: impl FnMut(UnitGroupRecord),
    ) -> Result<(), ImageEncodeError> {
        match self.asset.groups {
            AssetGroups::Authored(_) => {
                for plan in self.asset.group_plans() {
                    visit(plan?.record);
                }
            }
            _ => {
                for &record in self.records().into_iter().flatten() {
                    visit(record);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn write_index_into(&self, output: &mut [u8]) {
        debug_assert_eq!(output.len(), self.index_len);
        self.asset.write_indexes(&mut PayloadOutput::buffer(output));
    }

    pub(crate) fn write_data_into(&self, output: &mut [u8]) {
        debug_assert_eq!(output.len(), self.data_len);
        self.asset.write_data(&mut PayloadOutput::buffer(output));
    }

    pub(crate) fn emit_data(&self, output: &mut PayloadOutput<'_>) {
        self.asset.write_data(output);
    }

    pub(crate) fn data_crc(&self, range: core::ops::Range<u32>) -> u32 {
        self.asset.data_crc(range)
    }

    pub(crate) fn sections(&self) -> [Option<(MediaSectionKind, usize)>; 3] {
        [
            Some((MediaSectionKind::CODINGS, self.coding_len)),
            self.has_group_section()
                .then(|| {
                    self.group_len()
                        .map(|len| (MediaSectionKind::UNIT_GROUPS, len))
                })
                .transpose()
                .expect("validated group table size"),
            (self.index_len != 0).then_some((MediaSectionKind::UNIT_INDEX, self.index_len)),
        ]
    }

    pub(crate) fn emit_metadata(&self, output: &mut PayloadOutput<'_>) {
        output.write(&(self.asset.codings().len() as u32).to_le_bytes());
        let mut params_end = 0;
        for record in self.asset.codings() {
            params_end += record.params().len() as u32;
            output.write(&record.encode_entry(params_end));
        }
        for record in self.asset.codings() {
            output.write(record.params());
        }
        self.visit_group_records(|group| {
            output.write(&group.encode_record().expect("validated group record"));
        })
        .expect("validated group records");
        self.asset.write_indexes(output);
    }
}
