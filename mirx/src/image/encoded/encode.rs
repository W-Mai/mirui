use super::groups::{CodingRecords, GroupRecords, GroupSource};
use super::preflight::Preflight;
use alloc::vec::Vec;

use crate::{
    image::{
        ImageEncodeError, SURFACE_RECORD_LEN, SurfaceDescriptor, UNIT_GROUP_RECORD_LEN, UnitGroup,
        UnitGroupRecord, output::PayloadOutput,
    },
    media::{
        CODING_RECORD_LEN, CodingId, CodingRecord, CodingTable, DataIntegrity,
        INTEGRITY_RECORD_LEN, IntegrityRange, MEDIA_HEADER_LEN, MEDIA_SECTION_LEN, MEDIA_VERSION,
        MediaFlags, MediaSectionKind, UnitIndex,
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
    groups: Option<&'a [UnitGroupRecord]>,
    indexes: &'a [u8],
    integrity: DataIntegrity<'a>,
    data: &'a [u8],
    color_table: Option<&'a [u8]>,
    input_alignment: u32,
}

#[derive(Clone, Copy, Debug)]
enum AssetCodings<'a> {
    Single(CodingRecord<'a>),
    Shared(&'a [CodingRecord<'a>]),
}

impl<'a> AssetCodings<'a> {
    fn as_slice(&self) -> &[CodingRecord<'a>] {
        match self {
            Self::Single(record) => core::slice::from_ref(record),
            Self::Shared(records) => records,
        }
    }
}

impl<'a> EncodedImageAsset<'a> {
    /// Defines one whole-surface stream with an omitted group table by default.
    pub const fn new(surface: SurfaceDescriptor, coding: CodingRecord<'a>, data: &'a [u8]) -> Self {
        Self {
            surface,
            codings: AssetCodings::Single(coding),
            groups: None,
            indexes: &[],
            integrity: DataIntegrity::Whole,
            data,
            color_table: None,
            input_alignment: 1,
        }
    }

    /// Borrows shared profiles and explicit groups whose ranges address DATA.
    /// Group records own alignment, topology, coding ordinals and index offsets.
    pub const fn from_groups(
        surface: SurfaceDescriptor,
        codings: &'a [CodingRecord<'a>],
        groups: &'a [UnitGroupRecord],
        data: &'a [u8],
    ) -> Self {
        Self {
            surface,
            codings: AssetCodings::Shared(codings),
            groups: Some(groups),
            indexes: &[],
            integrity: DataIntegrity::Whole,
            data,
            color_table: None,
            input_alignment: 1,
        }
    }

    /// Attaches combined selection/range index bytes without changing their encoding.
    pub const fn with_index(mut self, bytes: &'a [u8]) -> Self {
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
    pub const fn with_input_alignment(mut self, alignment: u32) -> Self {
        self.input_alignment = alignment;
        self
    }
    pub const fn surface(self) -> SurfaceDescriptor {
        self.surface
    }
    pub fn codings(&self) -> &[CodingRecord<'a>] {
        self.codings.as_slice()
    }
    pub const fn groups(self) -> Option<&'a [UnitGroupRecord]> {
        self.groups
    }
    pub const fn index(self) -> &'a [u8] {
        self.indexes
    }
    pub const fn data(self) -> &'a [u8] {
        self.data
    }
    pub const fn color_table(self) -> Option<&'a [u8]> {
        self.color_table
    }
    /// Checked DATA alignment, derived from all groups; empty implicit streams need one.
    pub fn input_alignment(self) -> Result<u32, ImageEncodeError> {
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
        let groups = self.groups.map_or(1, <[UnitGroupRecord]>::len);
        let mut preflight = Preflight::new(limits, groups).map_err(ImageEncodeError::Preflight)?;
        let minimum = (self.codings().len() as u64)
            .checked_mul(CODING_RECORD_LEN as u64)
            .and_then(|len| {
                len.checked_add(
                    self.groups.map_or(0, <[UnitGroupRecord]>::len) as u64
                        * UNIT_GROUP_RECORD_LEN as u64,
                )
            })
            .and_then(|len| {
                (self.integrity.partitions().map_or(0, <[u32]>::len) as u64)
                    .checked_mul(INTEGRITY_RECORD_LEN as u64)
                    .and_then(|size| len.checked_add(size))
            })
            .ok_or(ImageEncodeError::SizeOverflow)?;
        // Bound native table scans before sizing; the output span charges these bytes once.
        if minimum > limits.max_image_work() {
            return Err(ImageEncodeError::Preflight(
                super::EncodedImageError::Coverage(crate::image::CoverageError::BudgetExceeded),
            ));
        }
        let plan = Plan::metadata(self)?;
        preflight
            .spend(plan.payload_len as u64)
            .map_err(ImageEncodeError::Preflight)?;
        preflight
            .groups(plan.source())
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
    alignment: u32,
    data_offset: usize,
    payload_len: usize,
}
impl<'a> Plan<'a> {
    fn new(asset: EncodedImageAsset<'a>) -> Result<Self, ImageEncodeError> {
        let plan = Self::metadata(asset)?;
        plan.source()
            .visit_groups(None, |_, _| {})
            .map_err(ImageEncodeError::Preflight)?;
        Ok(plan)
    }

    fn metadata(asset: EncodedImageAsset<'a>) -> Result<Self, ImageEncodeError> {
        asset
            .surface
            .read_color_table(asset.color_table)
            .map_err(ImageEncodeError::from)?;
        let group = if asset.groups.is_some() {
            if asset.input_alignment != 1 {
                return Err(ImageEncodeError::ConflictingAlignment);
            }
            None
        } else {
            let coding = asset.codings()[0];
            if coding.id() == CodingId::RAW {
                return Err(ImageEncodeError::UnexpectedCoding(CodingId::RAW));
            }
            let group = UnitGroup::builder(asset.surface, coding, asset.data)
                .with_input_alignment(asset.input_alignment)
                .build()
                .map_err(ImageEncodeError::Group)?;
            (!group.is_empty() && asset.input_alignment != 1).then(|| {
                UnitGroupRecord::new(0, 0..asset.data.len() as u32)
                    .expect("validated group range")
                    .with_input_alignment(asset.input_alignment)
            })
        };
        let records = asset
            .groups
            .or_else(|| group.as_ref().map(core::slice::from_ref));
        let source = GroupSource {
            surface: asset.surface,
            codings: CodingRecords::Native(asset.codings()),
            records: records.map_or(GroupRecords::Implicit, GroupRecords::Native),
            data: asset.data,
            indexes: asset.indexes,
            file_offset: None,
            data_offset: 0,
        };
        let alignment = source
            .input_alignment()
            .map_err(ImageEncodeError::Preflight)?;
        let coding_len =
            CodingTable::encoded_len(asset.codings()).map_err(ImageEncodeError::Codings)?;
        let integrity_len = asset
            .integrity
            .section_len(asset.data.len())
            .map_err(ImageEncodeError::Integrity)?;
        let group_len = records
            .map_or(0, <[UnitGroupRecord]>::len)
            .checked_mul(UNIT_GROUP_RECORD_LEN)
            .ok_or(ImageEncodeError::SizeOverflow)?;
        let section_count = 3
            + u16::from(records.is_some())
            + u16::from(!asset.indexes.is_empty())
            + u16::from(asset.integrity.partitions().is_some())
            + u16::from(asset.color_table.is_some());
        let metadata_len = (MEDIA_HEADER_LEN
            + usize::from(section_count) * MEDIA_SECTION_LEN
            + SURFACE_RECORD_LEN)
            .checked_add(coding_len)
            .and_then(|len| len.checked_add(group_len))
            .and_then(|len| len.checked_add(asset.indexes.len()))
            .and_then(|len| len.checked_add(asset.color_table.map_or(0, <[u8]>::len)))
            .and_then(|len| len.checked_add(integrity_len))
            .ok_or(ImageEncodeError::SizeOverflow)?;
        let data_offset = UnitIndex::aligned(
            u32::try_from(metadata_len).map_err(|_| ImageEncodeError::SizeOverflow)?,
            alignment,
        )
        .map_err(|_| ImageEncodeError::SizeOverflow)? as usize;
        let payload_len = data_offset
            .checked_add(asset.data.len())
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
            data_offset,
            payload_len,
        })
    }

    fn records(&self) -> Option<&[UnitGroupRecord]> {
        self.asset
            .groups
            .or_else(|| self.group.as_ref().map(core::slice::from_ref))
    }

    fn source(&self) -> GroupSource<'_> {
        GroupSource {
            surface: self.asset.surface,
            codings: CodingRecords::Native(self.asset.codings()),
            records: self
                .records()
                .map_or(GroupRecords::Implicit, GroupRecords::Native),
            data: self.asset.data,
            indexes: self.asset.indexes,
            file_offset: None,
            data_offset: self.data_offset as u32,
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
        if let Some(records) = self.records() {
            let size = records.len() * UNIT_GROUP_RECORD_LEN;
            output.section(MediaSectionKind::UNIT_GROUPS, offset, size);
            offset += size;
        }
        if !self.asset.indexes.is_empty() {
            output.section(
                MediaSectionKind::UNIT_INDEX,
                offset,
                self.asset.indexes.len(),
            );
            offset += self.asset.indexes.len();
        }
        if let Some(table) = self.asset.color_table {
            output.section(MediaSectionKind::COLOR_TABLE, offset, table.len());
            offset += table.len();
        }
        if self.asset.integrity.partitions().is_some() {
            output.section(MediaSectionKind::INTEGRITY, offset, self.integrity_len);
        }
        output.section(
            MediaSectionKind::DATA,
            self.data_offset,
            self.asset.data.len(),
        );
        let mut surface = [0; SURFACE_RECORD_LEN];
        self.asset
            .surface
            .encode_record_into(&mut surface)
            .expect("surface record size");
        output.write(&surface);
        output.write(&(self.asset.codings().len() as u32).to_le_bytes());
        let mut params_end = 0;
        for record in self.asset.codings() {
            params_end += record.params().len() as u32;
            output.write(&record.encode_entry(params_end));
        }
        for record in self.asset.codings() {
            output.write(record.params());
        }
        for group in self.records().into_iter().flatten() {
            let mut bytes = [0; UNIT_GROUP_RECORD_LEN];
            group
                .encode_into(&mut bytes)
                .expect("validated group record");
            output.write(&bytes);
        }
        output.write(self.asset.indexes);
        if let Some(table) = self.asset.color_table {
            output.write(table);
        }
        let mut start = 0;
        for &end in self.asset.integrity.partitions().unwrap_or(&[]) {
            let checksum = crate::crc32(&self.asset.data[start as usize..end as usize]);
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
        output.write(self.asset.data);
        debug_assert_eq!(
            output.position() + self.asset.integrity.trailer_len(),
            self.payload_len
        );
        output.finish()
    }
}
