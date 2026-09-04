use alloc::vec::Vec;

use crate::{
    image::{
        ImageEncodeError, SURFACE_RECORD_LEN, SurfaceDescriptor, UNIT_GROUP_RECORD_LEN, UnitGroup,
        UnitGroupRecord, output::PayloadOutput,
    },
    media::{
        CodingId, CodingRecord, CodingTable, MEDIA_CRC_LEN, MEDIA_HEADER_LEN, MEDIA_SECTION_LEN,
        MEDIA_VERSION, MediaSectionKind, UnitIndex,
    },
    wire::write_u16_le,
};

#[cfg(test)]
mod tests;

/// Borrowed single-stream IMAGE authoring input.
///
/// Metadata validation does not decode supplied bytes or imply codec support.
/// Unknown nonzero coding identifiers remain representable. Profile syntax is
/// checked separately through a resolved unit's decode plan.
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
    coding: CodingRecord<'a>,
    data: &'a [u8],
    color_table: Option<&'a [u8]>,
    input_alignment: u32,
}
impl<'a> EncodedImageAsset<'a> {
    pub const fn new(surface: SurfaceDescriptor, coding: CodingRecord<'a>, data: &'a [u8]) -> Self {
        Self {
            surface,
            coding,
            data,
            color_table: None,
            input_alignment: 1,
        }
    }
    pub const fn with_color_table(mut self, rgba: &'a [u8]) -> Self {
        self.color_table = Some(rgba);
        self
    }
    pub const fn with_input_alignment(mut self, alignment: u32) -> Self {
        self.input_alignment = alignment;
        self
    }
    pub const fn surface(self) -> SurfaceDescriptor {
        self.surface
    }
    pub const fn coding(self) -> CodingRecord<'a> {
        self.coding
    }
    pub const fn data(self) -> &'a [u8] {
        self.data
    }
    pub const fn color_table(self) -> Option<&'a [u8]> {
        self.color_table
    }
    pub const fn input_alignment(self) -> u32 {
        self.input_alignment
    }

    /// Exact payload size after metadata validation, without decoding samples.
    pub fn encoded_len(self) -> Result<usize, ImageEncodeError> {
        Ok(Plan::new(self)?.payload_len)
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
    section_count: u16,
    group: Option<UnitGroupRecord>,
    data_offset: usize,
    payload_len: usize,
}
impl<'a> Plan<'a> {
    fn new(asset: EncodedImageAsset<'a>) -> Result<Self, ImageEncodeError> {
        if asset.coding.id() == CodingId::RAW {
            return Err(ImageEncodeError::UnexpectedCoding(CodingId::RAW));
        }
        asset
            .surface
            .read_color_table(asset.color_table)
            .map_err(ImageEncodeError::from)?;
        let group = UnitGroup::builder(asset.surface, asset.coding, asset.data)
            .with_input_alignment(asset.input_alignment)
            .build()
            .map_err(ImageEncodeError::Group)?;
        let has_group = !group.is_empty() && asset.input_alignment != 1;
        let alignment = if has_group { asset.input_alignment } else { 1 };
        let group = has_group.then(|| {
            UnitGroupRecord::new(0, 0..asset.data.len() as u32)
                .expect("validated group range")
                .with_input_alignment(alignment)
        });
        let coding_len =
            CodingTable::encoded_len(&[asset.coding]).map_err(ImageEncodeError::Codings)?;
        let section_count = 3 + u16::from(has_group) + u16::from(asset.color_table.is_some());
        let metadata_len = (MEDIA_HEADER_LEN
            + usize::from(section_count) * MEDIA_SECTION_LEN
            + SURFACE_RECORD_LEN)
            .checked_add(coding_len)
            .and_then(|len| len.checked_add(usize::from(has_group) * UNIT_GROUP_RECORD_LEN))
            .and_then(|len| len.checked_add(asset.color_table.map_or(0, <[u8]>::len)))
            .ok_or(ImageEncodeError::SizeOverflow)?;
        let data_offset = UnitIndex::aligned(
            u32::try_from(metadata_len).map_err(|_| ImageEncodeError::SizeOverflow)?,
            alignment,
        )
        .map_err(|_| ImageEncodeError::SizeOverflow)? as usize;
        let payload_len = data_offset
            .checked_add(asset.data.len())
            .and_then(|len| len.checked_add(MEDIA_CRC_LEN))
            .ok_or(ImageEncodeError::SizeOverflow)?;
        u32::try_from(payload_len).map_err(|_| ImageEncodeError::SizeOverflow)?;
        Ok(Self {
            asset,
            coding_len,
            section_count,
            group,
            data_offset,
            payload_len,
        })
    }
    fn emit(self, mut output: PayloadOutput<'_>) -> bool {
        let mut header = [0; MEDIA_HEADER_LEN];
        header[0] = MEDIA_VERSION;
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
        if self.group.is_some() {
            output.section(MediaSectionKind::UNIT_GROUPS, offset, UNIT_GROUP_RECORD_LEN);
            offset += UNIT_GROUP_RECORD_LEN;
        }
        if let Some(table) = self.asset.color_table {
            output.section(MediaSectionKind::COLOR_TABLE, offset, table.len());
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
        output.write(&1u32.to_le_bytes());
        output.write(
            &self
                .asset
                .coding
                .encode_entry(self.asset.coding.params().len() as u32),
        );
        output.write(self.asset.coding.params());
        if let Some(group) = self.group {
            let mut bytes = [0; UNIT_GROUP_RECORD_LEN];
            group
                .encode_into(&mut bytes)
                .expect("validated group record");
            output.write(&bytes);
        }
        if let Some(table) = self.asset.color_table {
            output.write(table);
        }
        output.pad_to(self.data_offset);
        output.begin_data();
        output.write(self.asset.data);
        debug_assert_eq!(output.position() + MEDIA_CRC_LEN, self.payload_len);
        output.finish()
    }
}
