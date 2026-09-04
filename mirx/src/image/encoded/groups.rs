use super::EncodedImageError;
use crate::image::{
    CoverageBudget, ReferenceMode, SurfaceDescriptor, UNIT_GROUP_RECORD_LEN, UnitGroup,
    UnitGroupRecord,
};
use crate::media::{CodingRecord, CodingTable, CodingTableError};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy)]
pub(super) enum CodingRecords<'a> {
    Wire(CodingTable<'a>),
    Native(&'a [CodingRecord<'a>]),
}

impl<'a> CodingRecords<'a> {
    fn len(self) -> usize {
        match self {
            Self::Wire(table) => table.len(),
            Self::Native(records) => records.len(),
        }
    }
    fn get(self, index: usize) -> Option<CodingRecord<'a>> {
        match self {
            Self::Wire(table) => table.get(index),
            Self::Native(records) => records.get(index).copied(),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum GroupRecords<'a> {
    Implicit,
    Wire(&'a [u8]),
    Native(&'a [UnitGroupRecord]),
}

/// Borrowed static-image storage, independent of record serialization.
#[derive(Clone, Copy)]
pub(super) struct GroupSource<'a> {
    pub(super) surface: SurfaceDescriptor,
    pub(super) codings: CodingRecords<'a>,
    pub(super) records: GroupRecords<'a>,
    pub(super) data: &'a [u8],
    pub(super) indexes: &'a [u8],
    pub(super) file_offset: Option<u32>,
    pub(super) data_offset: u32,
}

impl<'a> GroupSource<'a> {
    pub(super) fn group_count(self) -> usize {
        match self.records {
            GroupRecords::Implicit => 1,
            GroupRecords::Wire(bytes) => bytes.len() / UNIT_GROUP_RECORD_LEN,
            GroupRecords::Native(records) => records.len(),
        }
    }

    fn validate_tables(self) -> Result<(), EncodedImageError> {
        u32::try_from(self.data.len()).map_err(|_| EncodedImageError::SizeOverflow)?;
        u32::try_from(self.indexes.len()).map_err(|_| EncodedImageError::SizeOverflow)?;
        if self.codings.len() == 0 {
            return Err(EncodedImageError::Codings(CodingTableError::EmptyTable));
        }
        match self.records {
            GroupRecords::Implicit if self.codings.len() != 1 || !self.indexes.is_empty() => {
                Err(EncodedImageError::AmbiguousImplicitGroup)
            }
            GroupRecords::Wire(bytes)
                if bytes.is_empty() || bytes.len() % UNIT_GROUP_RECORD_LEN != 0 =>
            {
                Err(EncodedImageError::InvalidGroupTableLength(bytes.len()))
            }
            GroupRecords::Native([]) => Err(EncodedImageError::InvalidGroupTableLength(0)),
            _ => Ok(()),
        }
    }

    pub(super) fn record(self, index: usize) -> Result<UnitGroupRecord, EncodedImageError> {
        if index >= self.group_count() {
            return Err(EncodedImageError::GroupOutOfBounds(index));
        }
        let record = match self.records {
            GroupRecords::Implicit => UnitGroupRecord::new(0, 0..self.data.len() as u32),
            GroupRecords::Wire(bytes) => {
                UnitGroupRecord::open(&bytes[index * UNIT_GROUP_RECORD_LEN..])
            }
            GroupRecords::Native(records) => {
                let record = records[index];
                record.validate().map(|()| record)
            }
        };
        record.map_err(|error| EncodedImageError::Group { index, error })
    }

    pub(super) fn input_alignment(self) -> Result<u32, EncodedImageError> {
        self.validate_tables()?;
        let mut alignment = 1;
        for index in 0..self.group_count() {
            alignment = alignment.max(self.record(index)?.input_alignment());
        }
        Ok(alignment)
    }

    pub(super) fn validate_groups(
        self,
        budget: &mut CoverageBudget,
    ) -> Result<(), EncodedImageError> {
        self.visit_groups(Some(budget), |_, _| {})?;
        self.surface
            .validate_coverage_by(
                self.group_count(),
                |index, budget| {
                    let record = self.record(index).expect("validated group record");
                    budget.spend_many(self.resolution_cost(record))?;
                    Ok(self
                        .resolve_record(index, record)
                        .expect("validated immutable group")
                        .0)
                },
                budget,
            )
            .map_err(EncodedImageError::Coverage)
    }

    pub(super) fn resolution_cost(self, record: UnitGroupRecord) -> u64 {
        record.resolution_work(self.indexes.len())
    }

    pub(super) fn resolve_record(
        self,
        index: usize,
        record: UnitGroupRecord,
    ) -> Result<(UnitGroup<'a>, core::ops::Range<u32>), EncodedImageError> {
        record
            .resolve_with(self.surface, self.data, self.indexes, |ordinal| {
                self.codings.get(ordinal as usize)
            })
            .map_err(|error| EncodedImageError::Group { index, error })
    }

    pub(super) fn visit_groups(
        self,
        mut budget: Option<&mut CoverageBudget>,
        mut visit: impl FnMut(usize, UnitGroup<'a>),
    ) -> Result<(), EncodedImageError> {
        self.validate_tables()?;
        let index_bytes = self.indexes;
        let mut data_end = 0u32;
        let mut index_end = 0u32;
        for index in 0..self.group_count() {
            let record = self.record(index)?;
            if let Some(budget) = budget.as_deref_mut() {
                budget
                    .spend_many(self.resolution_cost(record))
                    .map_err(EncodedImageError::Coverage)?;
            }
            if record.reference() != ReferenceMode::Independent {
                return Err(EncodedImageError::ReferenceInStaticImage(index));
            }
            let alignment = record.input_alignment();
            let expected_start = data_end
                .checked_add(alignment - 1)
                .map(|end| end & !(alignment - 1))
                .ok_or(EncodedImageError::SizeOverflow)?;
            if record.data_range().start != expected_start {
                return Err(EncodedImageError::NonCanonicalDataRange {
                    index,
                    expected_start,
                    actual_start: record.data_range().start,
                });
            }
            if let Some(file_offset) = self.file_offset {
                let absolute_offset = file_offset
                    .checked_add(self.data_offset)
                    .and_then(|offset| offset.checked_add(record.data_range().start))
                    .ok_or(EncodedImageError::SizeOverflow)?;
                if absolute_offset % alignment != 0 {
                    return Err(EncodedImageError::FileAddressUnaligned {
                        index,
                        absolute_offset,
                        alignment,
                    });
                }
            }
            let (group, range) = self.resolve_record(index, record)?;
            if group.is_empty() && !matches!(self.records, GroupRecords::Implicit) {
                return Err(EncodedImageError::EmptyGroup(index));
            }
            if !range.is_empty() {
                if range.start != index_end {
                    return Err(EncodedImageError::NonCanonicalIndexRange {
                        index,
                        expected_start: index_end,
                        actual_start: range.start,
                    });
                }
                index_end = range.end;
            }
            data_end = record.data_range().end;
            visit(index, group);
        }
        if data_end != self.data.len() as u32 {
            return Err(EncodedImageError::UnreferencedData);
        }
        if index_end as usize != index_bytes.len() {
            return Err(EncodedImageError::UnreferencedIndexes);
        }
        Ok(())
    }
}
