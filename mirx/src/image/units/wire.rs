use core::ops::Range;

use super::{GroupPlanes, ReferenceMode, SurfaceDescriptor, UnitGroup, UnitGroupError};
use crate::ByteAlignment;
#[cfg(test)]
use crate::media::CodingTable;
use crate::media::{
    UnitIndex, UnitIndexEncoding, UnitIndexError, UnitSelection, UnitSelectionEncoding,
    UnitSelectionError,
};
use crate::wire::{read_u32_le, write_u32_le};

pub(crate) const UNIT_GROUP_RECORD_LEN: usize = 36;

/// Selected-cell representation in a group's combined UNIT_INDEX body.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum GroupSelection {
    #[default]
    All,
    /// Number of strictly ordered u32 grid-cell ordinals.
    List(u32),
    Bitmap,
}

/// One fixed-width shared group record, independent of the number of tiles.
///
/// DATA and UNIT_INDEX offsets are relative to their respective section bodies.
/// Selection bytes precede byte-range index bytes; their lengths are derived.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct UnitGroupRecord {
    coding_index: u32,
    data_offset: u32,
    data_size: u32,
    index_offset: u32,
    tiles: Option<(u32, u32)>,
    planes: Option<GroupPlanes>,
    index_encoding: Option<UnitIndexEncoding>,
    selection: GroupSelection,
    reference: ReferenceMode,
    input_alignment: ByteAlignment,
}

impl UnitGroupRecord {
    /// Describes a whole-surface, fixed-size, independent group by default.
    pub fn new(coding_index: u32, data: Range<u32>) -> Result<Self, UnitGroupRecordError> {
        let data_size = data
            .end
            .checked_sub(data.start)
            .ok_or(UnitGroupRecordError::InvalidDataRange)?;
        Ok(Self {
            coding_index,
            data_offset: data.start,
            data_size,
            index_offset: 0,
            tiles: None,
            planes: None,
            index_encoding: None,
            selection: GroupSelection::All,
            reference: ReferenceMode::Independent,
            input_alignment: ByteAlignment::ONE,
        })
    }

    pub fn data_range(self) -> Range<u32> {
        self.data_offset..self.data_offset + self.data_size
    }
    #[cfg(test)]
    pub(crate) const fn index_offset(self) -> u32 {
        self.index_offset
    }
    #[cfg(test)]
    pub(crate) const fn index_encoding(self) -> Option<UnitIndexEncoding> {
        self.index_encoding
    }
    #[cfg(test)]
    pub(crate) const fn selection(self) -> GroupSelection {
        self.selection
    }
    pub const fn reference(self) -> ReferenceMode {
        self.reference
    }
    pub const fn input_alignment(self) -> ByteAlignment {
        self.input_alignment
    }

    /// Conservative resolution cost for a validated media payload's index span.
    pub(crate) fn resolution_work(self, index_bytes: usize) -> u64 {
        1 + u64::from(self.data_size) + index_bytes as u64
    }

    pub const fn with_tiles(mut self, width: u32, height: u32) -> Self {
        self.tiles = Some((width, height));
        self
    }
    pub const fn with_planes(mut self, planes: GroupPlanes) -> Self {
        self.planes = Some(planes);
        self
    }
    pub const fn with_index_offset(mut self, offset: u32) -> Self {
        self.index_offset = offset;
        self
    }
    pub const fn with_index_encoding(mut self, encoding: UnitIndexEncoding) -> Self {
        self.index_encoding = Some(encoding);
        self
    }
    pub const fn with_selection(mut self, selection: GroupSelection) -> Self {
        self.selection = selection;
        self
    }
    pub const fn with_reference(mut self, reference: ReferenceMode) -> Self {
        self.reference = reference;
        self
    }
    pub const fn with_input_alignment(mut self, alignment: ByteAlignment) -> Self {
        self.input_alignment = alignment;
        self
    }

    /// Reads the first complete record without aligned casts or DATA access.
    pub fn open(bytes: &[u8]) -> Result<Self, UnitGroupRecordError> {
        let bytes = bytes
            .get(..UNIT_GROUP_RECORD_LEN)
            .ok_or(UnitGroupRecordError::Truncated)?;
        let mut record = Self::new(read_u32_le(bytes, 0).unwrap(), {
            let offset = read_u32_le(bytes, 4).unwrap();
            offset
                ..offset
                    .checked_add(read_u32_le(bytes, 8).unwrap())
                    .ok_or(UnitGroupRecordError::SizeOverflow)?
        })?;
        record.index_offset = read_u32_le(bytes, 12).unwrap();
        let width = read_u32_le(bytes, 16).unwrap();
        let height = read_u32_le(bytes, 20).unwrap();
        record.tiles = match bytes[28] {
            0 if width == 0 && height == 0 => None,
            0 => return Err(UnitGroupRecordError::NonCanonical { offset: 16 }),
            1 => Some((width, height)),
            value => return Err(UnitGroupRecordError::UnknownValue { offset: 28, value }),
        };
        record.planes = match bytes[34] {
            0 if bytes[29] == 0 => None,
            0 => Some(GroupPlanes::Joint(bytes[29])),
            1 => Some(GroupPlanes::Plane(bytes[29])),
            value => return Err(UnitGroupRecordError::UnknownValue { offset: 34, value }),
        };
        record.index_encoding = match bytes[30] {
            0 => None,
            1 => Some(UnitIndexEncoding::Offsets),
            2 => Some(UnitIndexEncoding::Lengths16),
            3 => Some(UnitIndexEncoding::Lengths32),
            value => return Err(UnitGroupRecordError::UnknownValue { offset: 30, value }),
        };
        let listed_count = read_u32_le(bytes, 24).unwrap();
        if bytes[31] != 1 && listed_count != 0 {
            return Err(UnitGroupRecordError::NonCanonical { offset: 24 });
        }
        record.selection = match bytes[31] {
            0 => GroupSelection::All,
            1 => GroupSelection::List(listed_count),
            2 => GroupSelection::Bitmap,
            value => return Err(UnitGroupRecordError::UnknownValue { offset: 31, value }),
        };
        record.reference = match bytes[32] {
            0 => ReferenceMode::Independent,
            1 => ReferenceMode::Previous,
            value => return Err(UnitGroupRecordError::UnknownValue { offset: 32, value }),
        };
        if bytes[33] > 31 {
            return Err(UnitGroupRecordError::UnknownValue {
                offset: 33,
                value: bytes[33],
            });
        }
        record.input_alignment =
            ByteAlignment::new(1 << bytes[33]).expect("validated unit alignment exponent");
        if bytes[35] != 0 {
            return Err(UnitGroupRecordError::NonCanonical { offset: 35 });
        }
        record.validate()?;
        Ok(record)
    }

    pub fn encode_record(self) -> Result<[u8; UNIT_GROUP_RECORD_LEN], UnitGroupRecordError> {
        self.validate()?;
        let mut bytes = [0; UNIT_GROUP_RECORD_LEN];
        write_u32_le(&mut bytes, 0, self.coding_index);
        write_u32_le(&mut bytes, 4, self.data_offset);
        write_u32_le(&mut bytes, 8, self.data_size);
        write_u32_le(&mut bytes, 12, self.index_offset);
        if let Some((width, height)) = self.tiles {
            write_u32_le(&mut bytes, 16, width);
            write_u32_le(&mut bytes, 20, height);
            bytes[28] = 1;
        }
        if let Some(planes) = self.planes {
            match planes {
                GroupPlanes::Joint(mask) => bytes[29] = mask,
                GroupPlanes::Plane(index) => {
                    bytes[29] = index;
                    bytes[34] = 1;
                }
            }
        }
        bytes[30] = match self.index_encoding {
            None => 0,
            Some(UnitIndexEncoding::Offsets) => 1,
            Some(UnitIndexEncoding::Lengths16) => 2,
            Some(UnitIndexEncoding::Lengths32) => 3,
        };
        match self.selection {
            GroupSelection::All => {}
            GroupSelection::List(count) => {
                bytes[31] = 1;
                write_u32_le(&mut bytes, 24, count);
            }
            GroupSelection::Bitmap => bytes[31] = 2,
        }
        bytes[32] = match self.reference {
            ReferenceMode::Independent => 0,
            ReferenceMode::Previous => 1,
        };
        bytes[33] = self.input_alignment.log2();
        Ok(bytes)
    }

    #[cfg(test)]
    pub(crate) fn resolve<'a>(
        self,
        surface: SurfaceDescriptor,
        codings: CodingTable<'a>,
        data: &'a [u8],
        indexes: &'a [u8],
    ) -> Result<UnitGroup<'a>, UnitGroupRecordError> {
        self.resolve_with_index_range(surface, codings, data, indexes)
            .map(|(group, _)| group)
    }

    #[cfg(test)]
    pub(crate) fn resolve_with_index_range<'a>(
        self,
        surface: SurfaceDescriptor,
        codings: CodingTable<'a>,
        data: &'a [u8],
        indexes: &'a [u8],
    ) -> Result<(UnitGroup<'a>, Range<u32>), UnitGroupRecordError> {
        self.resolve_with(surface, data, indexes, |index| codings.get(index as usize))
    }

    pub(crate) fn resolve_with<'a>(
        self,
        surface: SurfaceDescriptor,
        data: &'a [u8],
        indexes: &'a [u8],
        coding_at: impl FnOnce(u32) -> Option<crate::media::CodingRecord<'a>>,
    ) -> Result<(UnitGroup<'a>, Range<u32>), UnitGroupRecordError> {
        self.validate()?;
        u32::try_from(data.len()).map_err(|_| UnitGroupRecordError::SizeOverflow)?;
        u32::try_from(indexes.len()).map_err(|_| UnitGroupRecordError::SizeOverflow)?;
        let coding = coding_at(self.coding_index)
            .ok_or(UnitGroupRecordError::MissingCoding(self.coding_index))?;
        let range = self.data_range();
        let bytes = data
            .get(range.start as usize..range.end as usize)
            .ok_or(UnitGroupRecordError::DataOutOfBounds)?;
        let mut builder = UnitGroup::builder(surface, coding, bytes)
            .with_reference(self.reference)
            .with_input_alignment(self.input_alignment);
        if let Some((width, height)) = self.tiles {
            builder = builder.with_tiles(width, height);
        }
        if let Some(planes) = self.planes {
            builder = builder.with_planes(planes);
        }
        let cell_count = builder.grid().map_err(UnitGroupRecordError::Group)?.len() as u32;
        let selection_len = match self.selection {
            GroupSelection::All => 0,
            GroupSelection::List(count) => UnitSelectionEncoding::List
                .table_len(cell_count, count as usize)
                .map_err(UnitGroupRecordError::Selection)?,
            GroupSelection::Bitmap => UnitSelectionEncoding::Bitmap
                .table_len(cell_count, 0)
                .map_err(UnitGroupRecordError::Selection)?,
        };
        let selection_end = (self.index_offset as usize)
            .checked_add(selection_len)
            .ok_or(UnitGroupRecordError::SizeOverflow)?;
        let selected_bytes = indexes
            .get(self.index_offset as usize..selection_end)
            .ok_or(UnitGroupRecordError::IndexOutOfBounds)?;
        let selection = match self.selection {
            GroupSelection::All => UnitSelection::all(cell_count),
            GroupSelection::List(_) => UnitSelection::list(cell_count, selected_bytes),
            GroupSelection::Bitmap => UnitSelection::bitmap(cell_count, selected_bytes),
        }
        .map_err(UnitGroupRecordError::Selection)?;
        let range_len = match self.index_encoding {
            None => 0,
            Some(encoding) => encoding
                .table_len(selection.len())
                .map_err(UnitGroupRecordError::Index)?,
        };
        let index_end = selection_end
            .checked_add(range_len)
            .ok_or(UnitGroupRecordError::SizeOverflow)?;
        let encoded_ranges = indexes
            .get(selection_end..index_end)
            .ok_or(UnitGroupRecordError::IndexOutOfBounds)?;
        builder = builder.with_selection(selection);
        if let Some(encoding) = self.index_encoding {
            let ranges = match encoding {
                UnitIndexEncoding::Offsets => UnitIndex::offsets(encoded_ranges),
                UnitIndexEncoding::Lengths16 => UnitIndex::lengths16(
                    selection.len() as u32,
                    encoded_ranges,
                    self.input_alignment,
                ),
                UnitIndexEncoding::Lengths32 => UnitIndex::lengths32(
                    selection.len() as u32,
                    encoded_ranges,
                    self.input_alignment,
                ),
            }
            .map_err(UnitGroupRecordError::Index)?;
            builder = builder.with_index(ranges);
        }
        if index_end == self.index_offset as usize && self.index_offset != 0 {
            return Err(UnitGroupRecordError::NonCanonical { offset: 12 });
        }
        Ok((
            builder.build().map_err(UnitGroupRecordError::Group)?,
            self.index_offset..index_end as u32,
        ))
    }

    pub(crate) fn validate(self) -> Result<(), UnitGroupRecordError> {
        if self
            .tiles
            .is_some_and(|(width, height)| width == 0 || height == 0)
        {
            return Err(UnitGroupRecordError::NonCanonical { offset: 16 });
        }
        if self.planes == Some(GroupPlanes::Joint(0)) {
            return Err(UnitGroupRecordError::NonCanonical { offset: 29 });
        }
        if self.data_offset % self.input_alignment.get() != 0 {
            return Err(UnitGroupRecordError::UnalignedData {
                offset: self.data_offset,
                alignment: self.input_alignment,
            });
        }
        if self.index_encoding.is_none()
            && matches!(
                self.selection,
                GroupSelection::All | GroupSelection::List(0)
            )
            && self.index_offset != 0
        {
            return Err(UnitGroupRecordError::NonCanonical { offset: 12 });
        }
        Ok(())
    }
}

/// Failure while validating stored encoded-group metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EncodedGroupError {
    Truncated,
    InvalidDataRange,
    SizeOverflow,
    UnknownValue {
        offset: usize,
        value: u8,
    },
    NonCanonical {
        offset: usize,
    },
    UnalignedData {
        offset: u32,
        alignment: ByteAlignment,
    },
    MissingCoding(u32),
    DataOutOfBounds,
    IndexOutOfBounds,
    Selection(UnitSelectionError),
    Index(UnitIndexError),
    Group(UnitGroupError),
}

pub(crate) type UnitGroupRecordError = EncodedGroupError;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::{ColorDescription, Region, SampleLayout};
    use alloc::vec;

    const CODINGS: [u8; 14] = [1, 0, 0, 0, 0x34, 0x12, 7, 0, 2, 0, 0, 0, 8, 9];
    const RECORD: [u8; 36] = [
        0, 0, 0, 0, 4, 0, 0, 0, 8, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 1, 0,
        1, 1, 1, 0, 0, 0,
    ];
    const INDEXES: [u8; 30] = [
        0xa5, 0xa5, 0, 0, 0, 0, 2, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 5, 0, 0, 0, 8, 0,
        0, 0,
    ];
    fn surface() -> SurfaceDescriptor {
        SurfaceDescriptor::new(
            5,
            3,
            SampleLayout::NV12,
            ColorDescription::BT709_YUV_LIMITED,
        )
        .unwrap()
    }

    #[test]
    fn aligned_group_records_exclude_padding_from_exact_lz4_streams() {
        use crate::{coding::Lz4, image::SurfaceRequirements};
        let surface =
            SurfaceDescriptor::new(26, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let codec = Lz4::new();
        let mut table = [0; Lz4::TABLE_LEN];
        let mut encoder = codec.encoder(&mut table).unwrap();
        let mut coding_bytes = [0; 12];
        CodingTable::encode_into(&[codec.record()], &mut coding_bytes).unwrap();
        let codings = CodingTable::open(&coding_bytes).unwrap();
        for encoding in [
            None,
            Some(UnitIndexEncoding::Lengths16),
            Some(UnitIndexEncoding::Lengths32),
        ] {
            let first = [17; 13];
            let second = if encoding.is_none() {
                [23; 13]
            } else {
                [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]
            };
            #[repr(align(64))]
            struct Buffer([u8; 128]);
            let mut data = Buffer([0xa5; 128]);
            let a = encoder.encode_into(&first, &mut data.0).unwrap();
            let b = encoder.encode_into(&second, &mut data.0[64..]).unwrap();
            let mut indexes = [0; 12];
            let index_len = encoding
                .map(|e| {
                    e.encode_into(
                        &[a as u32, b as u32],
                        crate::ByteAlignment::new(64).unwrap(),
                        &mut indexes,
                    )
                    .unwrap()
                })
                .unwrap_or(0);
            let mut record = UnitGroupRecord::new(0, 0..(64 + b) as u32)
                .unwrap()
                .with_tiles(13, 1)
                .with_input_alignment(crate::ByteAlignment::new(64).unwrap());
            if let Some(encoding) = encoding {
                record = record.with_index_encoding(encoding);
            }
            let wire = record.encode_record().unwrap();
            let group = UnitGroupRecord::open(&wire)
                .unwrap()
                .resolve(surface, codings, &data.0[..64 + b], &indexes[..index_len])
                .unwrap();
            assert_eq!(group.index().byte_len(), (64 + b) as u32);
            for (unit, expected) in group.iter().zip([first, second]) {
                assert!(unit.data_address_is_aligned());
                let mut output = [0; 13];
                unit.decode_plan(SurfaceRequirements::new())
                    .unwrap()
                    .decode_into(&mut output)
                    .unwrap();
                assert_eq!(output, expected);
            }
            assert_eq!(group.get(0).unwrap().data_range(), 0..a as u32);
            assert_eq!(group.get(1).unwrap().data_range(), 64..(64 + b) as u32);
            assert!(data.0[a..64].iter().all(|b| *b == 0xa5));
        }
    }

    #[test]
    fn independent_group_packet_resolves_relative_bases_and_shared_rules() {
        let record = UnitGroupRecord::open(&RECORD).unwrap();
        let data = [0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 0xff];
        let (group, index_range) = record
            .resolve_with_index_range(
                surface(),
                CodingTable::open(&CODINGS).unwrap(),
                &data,
                &INDEXES,
            )
            .unwrap();
        assert_eq!(record.data_range(), 4..12);
        assert_eq!(index_range, 2..30);
        let unit = group.cell(5).unwrap();
        assert_eq!(unit.data(), &[6, 7, 8]);
        assert_eq!(unit.data().as_ptr(), data[9..].as_ptr());
        assert_eq!(unit.data_range(), 5..8);
        assert_eq!(unit.region(), Region::new(4, 2, 1, 1).unwrap());
        assert_eq!(unit.plane_region(1), Some(Region::new(2, 1, 1, 1).unwrap()));
        assert_eq!(unit.reference(), ReferenceMode::Previous);
        assert_eq!(unit.coding().id().raw(), 0x1234);
        assert_eq!(unit.coding().params(), &[8, 9]);
        assert_eq!(record.encode_record(), Ok(RECORD));
        let authored = UnitGroupRecord::new(0, 4..12)
            .unwrap()
            .with_tiles(2, 2)
            .with_index_offset(2)
            .with_selection(GroupSelection::List(3))
            .with_index_encoding(UnitIndexEncoding::Offsets)
            .with_reference(ReferenceMode::Previous);
        assert_eq!(authored, record);
    }

    #[test]
    fn selection_and_range_forms_share_one_resolution_contract() {
        let codings = CodingTable::open(&CODINGS).unwrap();
        for selection_form in [
            GroupSelection::All,
            GroupSelection::List(3),
            GroupSelection::Bitmap,
        ] {
            let selected = match selection_form {
                GroupSelection::All => &[0, 1, 2, 3, 4, 5][..],
                _ => &[0, 2, 5][..],
            };
            let lengths = vec![2; selected.len()];
            for encoding in [
                None,
                Some(UnitIndexEncoding::Offsets),
                Some(UnitIndexEncoding::Lengths16),
                Some(UnitIndexEncoding::Lengths32),
            ] {
                let selection_encoding = match selection_form {
                    GroupSelection::All => None,
                    GroupSelection::List(_) => Some(UnitSelectionEncoding::List),
                    GroupSelection::Bitmap => Some(UnitSelectionEncoding::Bitmap),
                };
                let selection_len = selection_encoding
                    .map(|encoding| encoding.encoded_len(6, selected).unwrap())
                    .unwrap_or(0);
                let range_len = encoding
                    .map(|encoding| {
                        encoding
                            .encoded_len(&lengths, crate::ByteAlignment::ONE)
                            .unwrap()
                    })
                    .unwrap_or(0);
                let mut indexes = vec![0xa5; selection_len + range_len];
                if let Some(encoding) = selection_encoding {
                    encoding.encode_into(6, selected, &mut indexes).unwrap();
                }
                if let Some(encoding) = encoding {
                    encoding
                        .encode_into(
                            &lengths,
                            crate::ByteAlignment::ONE,
                            &mut indexes[selection_len..],
                        )
                        .unwrap();
                }
                let data = vec![9; selected.len() * 2];
                let mut record = UnitGroupRecord::new(0, 0..data.len() as u32)
                    .unwrap()
                    .with_tiles(2, 2)
                    .with_selection(selection_form);
                if let Some(encoding) = encoding {
                    record = record.with_index_encoding(encoding);
                }
                let bytes = record.encode_record().unwrap();
                let group = UnitGroupRecord::open(&bytes)
                    .unwrap()
                    .resolve(surface(), codings, &data, &indexes)
                    .unwrap();
                assert!(
                    group
                        .iter()
                        .map(|unit| unit.cell())
                        .eq(selected.iter().copied())
                );
                assert!(group.iter().all(|unit| unit.data() == [9, 9]));
            }
        }
        let whole = UnitGroupRecord::new(0, 0..3)
            .unwrap()
            .resolve(surface(), codings, &[1, 2, 3], &[])
            .unwrap();
        assert_eq!(whole.len(), 1);
        assert_eq!(
            whole.get(0).unwrap().region(),
            Region::new(0, 0, 5, 3).unwrap()
        );
        let planar_record = UnitGroupRecord::new(0, 0..6)
            .unwrap()
            .with_tiles(1, 1)
            .with_planes(GroupPlanes::Plane(1));
        let planar_bytes = planar_record.encode_record().unwrap();
        assert_eq!(planar_bytes[29], 1);
        assert_eq!(planar_bytes[34], 1);
        let planar = UnitGroupRecord::open(&planar_bytes)
            .unwrap()
            .resolve(surface(), codings, &[1; 6], &[])
            .unwrap();
        assert_eq!(
            planar.get(5).unwrap().region(),
            Region::new(2, 1, 1, 1).unwrap()
        );
    }

    #[test]
    fn malformed_records_and_resolved_bounds_are_rejected_before_writes() {
        for offset in [28, 30, 31, 32, 33, 34, 35] {
            let mut record = RECORD;
            record[offset] = 255;
            assert!(UnitGroupRecord::open(&record).is_err());
        }
        for length in 0..36 {
            assert_eq!(
                UnitGroupRecord::open(&RECORD[..length]),
                Err(UnitGroupRecordError::Truncated)
            );
        }
        let mut overflow = RECORD;
        overflow[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(
            UnitGroupRecord::open(&overflow),
            Err(UnitGroupRecordError::SizeOverflow)
        );
        let mut noncanonical = RECORD;
        noncanonical[31] = 0;
        assert_eq!(
            UnitGroupRecord::open(&noncanonical),
            Err(UnitGroupRecordError::NonCanonical { offset: 24 })
        );
        noncanonical = RECORD;
        noncanonical[28] = 0;
        assert_eq!(
            UnitGroupRecord::open(&noncanonical),
            Err(UnitGroupRecordError::NonCanonical { offset: 16 })
        );
        let base = UnitGroupRecord::new(0, 0..4).unwrap();
        for record in [
            base.with_tiles(0, 1),
            base.with_planes(GroupPlanes::Joint(0)),
            base.with_index_offset(1),
            UnitGroupRecord::new(0, 1..4)
                .unwrap()
                .with_input_alignment(crate::ByteAlignment::new(64).unwrap()),
        ] {
            assert!(record.encode_record().is_err());
        }
        assert!(crate::ByteAlignment::new(3).is_err());
        let codings = CodingTable::open(&CODINGS).unwrap();
        let record = UnitGroupRecord::open(&RECORD).unwrap();
        assert_eq!(
            record.resolve(surface(), codings, &[1; 11], &INDEXES),
            Err(UnitGroupRecordError::DataOutOfBounds)
        );
        assert_eq!(
            record.resolve(surface(), codings, &[1; 12], &INDEXES[..29]),
            Err(UnitGroupRecordError::IndexOutOfBounds)
        );
        assert_eq!(
            UnitGroupRecord::new(1, 0..1)
                .unwrap()
                .resolve(surface(), codings, &[1], &[]),
            Err(UnitGroupRecordError::MissingCoding(1))
        );
        assert!(matches!(
            base.with_planes(GroupPlanes::Plane(7))
                .resolve(surface(), codings, &[1; 4], &[]),
            Err(UnitGroupRecordError::Group(UnitGroupError::InvalidPlanes(
                _
            )))
        ));
    }
}
