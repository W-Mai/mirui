use core::iter::FusedIterator;

use super::{
    PLANE_RECORD_LEN, PlaneGeometry, PlaneMemoryError, PlaneMemoryLayout, PlaneMemoryRecordError,
    SURFACE_RECORD_LEN, SampleLayout, SurfaceDescriptor, SurfaceRecordError,
};
use crate::media::{CodingId, MediaPayload, MediaPayloadError, MediaSection, MediaSectionKind};
use crate::payload::ColorTableView;

/// Borrowed zero-allocation view of one RAW sectioned IMAGE payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RawImageView<'a> {
    media: MediaPayload<'a>,
    surface: SurfaceDescriptor,
    plane_records: Option<MediaSection<'a>>,
    color_table: Option<ColorTableView<'a>>,
    data: MediaSection<'a>,
}

impl<'a> RawImageView<'a> {
    /// Opens a RAW IMAGE payload without assuming its outer file position.
    pub fn open(payload: &'a [u8]) -> Result<Self, RawImageViewError> {
        let media = MediaPayload::open(payload).map_err(RawImageViewError::Media)?;
        Self::from_media(media, None)
    }

    /// Opens a RAW IMAGE payload and validates file-relative DATA and plane
    /// alignment against its outer MIRX position.
    pub fn open_at(payload: &'a [u8], payload_file_offset: u32) -> Result<Self, RawImageViewError> {
        let media = MediaPayload::open_at(payload, payload_file_offset)
            .map_err(RawImageViewError::Media)?;
        Self::from_media(media, Some(payload_file_offset))
    }

    pub const fn media(self) -> MediaPayload<'a> {
        self.media
    }

    pub const fn surface(self) -> SurfaceDescriptor {
        self.surface
    }

    pub const fn sample_layout(self) -> SampleLayout {
        self.surface.sample_layout()
    }

    pub const fn color_table(self) -> Option<ColorTableView<'a>> {
        self.color_table
    }

    pub const fn plane_count(self) -> u8 {
        self.surface.plane_count()
    }

    pub fn plane(self, index: u8) -> Option<SurfacePlane<'a>> {
        let geometry = self.surface.plane(index)?;
        let memory = self.plane_memory(index, geometry).ok()?;
        let bytes = memory.bytes(self.data.bytes())?;
        Some(SurfacePlane {
            geometry,
            memory,
            bytes,
        })
    }

    pub fn planes(self) -> RawImagePlanes<'a> {
        RawImagePlanes {
            image: self,
            front: 0,
            back: self.plane_count(),
        }
    }

    /// Returns whether every plane can use its declared alignment at the
    /// actual in-memory DATA address.
    pub fn data_addresses_are_aligned(self) -> bool {
        self.planes().all(|plane| plane.address_is_aligned())
    }

    fn from_media(
        media: MediaPayload<'a>,
        payload_file_offset: Option<u32>,
    ) -> Result<Self, RawImageViewError> {
        if media.header().default_coding() != CodingId::RAW {
            return Err(RawImageViewError::UnsupportedCoding(
                media.header().default_coding(),
            ));
        }
        if media.header().profile_revision() != 0 {
            return Err(RawImageViewError::UnsupportedRevision(
                media.header().profile_revision(),
            ));
        }

        let mut surface_section = None;
        let mut plane_records = None;
        let mut color_table_section = None;
        let mut data = None;
        for section in media.sections() {
            let kind = section.descriptor().kind();
            let slot = match kind {
                MediaSectionKind::SURFACE => Some(&mut surface_section),
                MediaSectionKind::PLANES => Some(&mut plane_records),
                MediaSectionKind::COLOR_TABLE => Some(&mut color_table_section),
                MediaSectionKind::DATA => Some(&mut data),
                MediaSectionKind::CODING_PARAMS | MediaSectionKind::ACCESS_UNITS => {
                    return Err(RawImageViewError::UnexpectedSection(kind));
                }
                _ if section.descriptor().flags().is_required() => {
                    return Err(RawImageViewError::UnknownRequiredSection(kind));
                }
                _ => None,
            };
            if let Some(slot) = slot {
                if slot.replace(section).is_some() {
                    return Err(RawImageViewError::DuplicateSection(kind));
                }
                if !section.descriptor().flags().is_required() {
                    return Err(RawImageViewError::SectionMustBeRequired(kind));
                }
            }
        }

        let surface_section =
            surface_section.ok_or(RawImageViewError::MissingSection(MediaSectionKind::SURFACE))?;
        validate_section_size(surface_section, SURFACE_RECORD_LEN)?;
        let surface = SurfaceDescriptor::from_record(surface_section.bytes())
            .map_err(RawImageViewError::Surface)?;
        let data = data.ok_or(RawImageViewError::MissingSection(MediaSectionKind::DATA))?;
        validate_raw_section_size(data)?;

        let color_table = validate_color_table(surface, color_table_section)?;
        let view = Self {
            media,
            surface,
            plane_records,
            color_table,
            data,
        };
        view.validate_planes(payload_file_offset)?;
        Ok(view)
    }

    fn validate_planes(self, payload_file_offset: Option<u32>) -> Result<(), RawImageViewError> {
        if let Some(section) = self.plane_records {
            let expected = usize::from(self.plane_count())
                .checked_mul(PLANE_RECORD_LEN)
                .ok_or(RawImageViewError::SizeOverflow)?;
            validate_section_size(section, expected)?;
        }

        let data_file_offset = match payload_file_offset {
            Some(payload_offset) => Some(
                payload_offset
                    .checked_add(self.data.descriptor().offset())
                    .ok_or(RawImageViewError::SizeOverflow)?,
            ),
            None => None,
        };
        let data_len =
            u32::try_from(self.data.bytes().len()).map_err(|_| RawImageViewError::SizeOverflow)?;
        let mut previous_end = 0;
        for index in 0..self.plane_count() {
            let geometry = self
                .surface
                .plane(index)
                .expect("validated surface plane index");
            let memory = self.plane_memory(index, geometry)?;
            if memory.data_offset() < previous_end {
                return Err(RawImageViewError::PlaneRangesOverlap {
                    previous: index - 1,
                    next: index,
                });
            }
            if memory.data_end() > data_len {
                return Err(RawImageViewError::PlaneDataOutOfBounds {
                    index,
                    offset: memory.data_offset(),
                    size: memory.byte_len(),
                    data_size: data_len,
                });
            }
            if let Some(data_offset) = data_file_offset {
                if !memory.file_address_is_aligned(data_offset) {
                    let absolute_offset = data_offset
                        .checked_add(memory.data_offset())
                        .ok_or(RawImageViewError::SizeOverflow)?;
                    return Err(RawImageViewError::PlaneFileAddressUnaligned {
                        index,
                        absolute_offset,
                        alignment: memory.required_alignment(),
                    });
                }
            }
            previous_end = memory.data_end();
        }
        if self.plane_records.is_none() && previous_end != data_len {
            return Err(RawImageViewError::CanonicalDataSizeMismatch {
                expected: previous_end,
                actual: data_len,
            });
        }
        Ok(())
    }

    fn plane_memory(
        self,
        index: u8,
        geometry: PlaneGeometry,
    ) -> Result<PlaneMemoryLayout, RawImageViewError> {
        match self.plane_records {
            Some(records) => {
                let start = usize::from(index)
                    .checked_mul(PLANE_RECORD_LEN)
                    .ok_or(RawImageViewError::SizeOverflow)?;
                let record = records
                    .bytes()
                    .get(
                        start
                            ..start
                                .checked_add(PLANE_RECORD_LEN)
                                .ok_or(RawImageViewError::SizeOverflow)?,
                    )
                    .ok_or(RawImageViewError::SizeOverflow)?;
                PlaneMemoryLayout::from_record(geometry, record)
                    .map_err(|error| RawImageViewError::InvalidPlaneRecord { index, error })
            }
            None => canonical_plane_memory(self.surface, index).map_err(|error| {
                RawImageViewError::InvalidPlaneRecord {
                    index,
                    error: PlaneMemoryRecordError::InvalidLayout(error),
                }
            }),
        }
    }
}

/// One borrowed RAW image plane and its logical and physical descriptors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SurfacePlane<'a> {
    pub(super) geometry: PlaneGeometry,
    pub(super) memory: PlaneMemoryLayout,
    pub(super) bytes: &'a [u8],
}

impl<'a> SurfacePlane<'a> {
    pub const fn geometry(self) -> PlaneGeometry {
        self.geometry
    }

    pub const fn memory(self) -> PlaneMemoryLayout {
        self.memory
    }

    pub const fn bytes(self) -> &'a [u8] {
        self.bytes
    }

    pub fn address_is_aligned(self) -> bool {
        self.bytes.is_empty()
            || self.bytes.as_ptr() as usize % self.memory.required_alignment() as usize == 0
    }
}

/// Exact-size iterator over borrowed RAW image planes.
#[derive(Clone, Debug)]
pub struct RawImagePlanes<'a> {
    image: RawImageView<'a>,
    front: u8,
    back: u8,
}

impl<'a> Iterator for RawImagePlanes<'a> {
    type Item = SurfacePlane<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        let index = self.front;
        self.front += 1;
        self.image.plane(index)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl DoubleEndedIterator for RawImagePlanes<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        self.back -= 1;
        self.image.plane(self.back)
    }
}

impl ExactSizeIterator for RawImagePlanes<'_> {
    fn len(&self) -> usize {
        usize::from(self.back - self.front)
    }
}

impl FusedIterator for RawImagePlanes<'_> {}

/// Failure while opening a typed zero-copy RAW IMAGE payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum RawImageViewError {
    Media(MediaPayloadError),
    UnsupportedCoding(CodingId),
    UnsupportedRevision(u16),
    MissingSection(MediaSectionKind),
    DuplicateSection(MediaSectionKind),
    SectionMustBeRequired(MediaSectionKind),
    UnknownRequiredSection(MediaSectionKind),
    UnexpectedSection(MediaSectionKind),
    SectionSizeMismatch {
        kind: MediaSectionKind,
        expected: usize,
        actual: usize,
    },
    SectionDecodedSizeMismatch {
        kind: MediaSectionKind,
        expected: u32,
        actual: u32,
    },
    Surface(SurfaceRecordError),
    InvalidPlaneRecord {
        index: u8,
        error: PlaneMemoryRecordError,
    },
    MissingColorTable,
    UnexpectedColorTable,
    PlaneRangesOverlap {
        previous: u8,
        next: u8,
    },
    PlaneDataOutOfBounds {
        index: u8,
        offset: u32,
        size: u32,
        data_size: u32,
    },
    PlaneFileAddressUnaligned {
        index: u8,
        absolute_offset: u32,
        alignment: u32,
    },
    CanonicalDataSizeMismatch {
        expected: u32,
        actual: u32,
    },
    SizeOverflow,
}

fn validate_section_size(
    section: MediaSection<'_>,
    expected: usize,
) -> Result<(), RawImageViewError> {
    let kind = section.descriptor().kind();
    let actual = section.bytes().len();
    if actual != expected {
        return Err(RawImageViewError::SectionSizeMismatch {
            kind,
            expected,
            actual,
        });
    }
    let expected_decoded = u32::try_from(expected).map_err(|_| RawImageViewError::SizeOverflow)?;
    let actual_decoded = section.descriptor().decoded_size();
    if actual_decoded != expected_decoded {
        return Err(RawImageViewError::SectionDecodedSizeMismatch {
            kind,
            expected: expected_decoded,
            actual: actual_decoded,
        });
    }
    Ok(())
}

fn validate_raw_section_size(section: MediaSection<'_>) -> Result<(), RawImageViewError> {
    let expected =
        u32::try_from(section.bytes().len()).map_err(|_| RawImageViewError::SizeOverflow)?;
    let actual = section.descriptor().decoded_size();
    if actual != expected {
        return Err(RawImageViewError::SectionDecodedSizeMismatch {
            kind: section.descriptor().kind(),
            expected,
            actual,
        });
    }
    Ok(())
}

fn validate_color_table(
    surface: SurfaceDescriptor,
    section: Option<MediaSection<'_>>,
) -> Result<Option<ColorTableView<'_>>, RawImageViewError> {
    let expected_entries = surface.sample_layout().color_table_entries();
    match (expected_entries, section) {
        (Some(entries), Some(section)) => {
            let expected = usize::try_from(entries)
                .ok()
                .and_then(|entries| entries.checked_mul(4))
                .ok_or(RawImageViewError::SizeOverflow)?;
            validate_section_size(section, expected)?;
            let table = ColorTableView::from_rgba_bytes(section.bytes())
                .expect("validated indexed color table length");
            Ok(Some(table))
        }
        (Some(_), None) => Err(RawImageViewError::MissingColorTable),
        (None, Some(_)) => Err(RawImageViewError::UnexpectedColorTable),
        (None, None) => Ok(None),
    }
}

fn canonical_plane_memory(
    surface: SurfaceDescriptor,
    requested: u8,
) -> Result<PlaneMemoryLayout, PlaneMemoryError> {
    let mut offset = 0;
    for index in 0..surface.plane_count() {
        let geometry = surface.plane(index).ok_or(PlaneMemoryError::SizeOverflow)?;
        let memory = PlaneMemoryLayout::builder(geometry)
            .with_data_offset(offset)
            .build()?;
        if index == requested {
            return Ok(memory);
        }
        offset = memory.data_end();
    }
    Err(PlaneMemoryError::SizeOverflow)
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use alloc::vec::Vec;

    use super::*;
    use crate::crc32;
    use crate::image::{ColorDescription, PlaneMemoryFlags};
    use crate::media::{
        MEDIA_CRC_LEN, MEDIA_HEADER_LEN, MEDIA_SECTION_LEN, MEDIA_VERSION, MediaSectionFlags,
    };
    use crate::wire::{write_u16_le, write_u32_le};

    fn surface_record(surface: SurfaceDescriptor) -> [u8; SURFACE_RECORD_LEN] {
        let mut record = [0; SURFACE_RECORD_LEN];
        surface.encode_record_into(&mut record).unwrap();
        record
    }

    fn plane_record(
        plane: PlaneGeometry,
        offset: u32,
        alignment: u32,
        allocation_width: u32,
        allocation_height: u32,
        stride: u32,
    ) -> [u8; PLANE_RECORD_LEN] {
        let memory = PlaneMemoryLayout::builder(plane)
            .with_allocation_extent(allocation_width, allocation_height)
            .with_stride(stride)
            .with_data_offset(offset)
            .with_alignment(alignment)
            .with_flags(PlaneMemoryFlags::from_bits_retain(0xa500))
            .build()
            .unwrap();
        let mut record = [0; PLANE_RECORD_LEN];
        memory.encode_record_into(&mut record).unwrap();
        record
    }

    fn payload(
        surface: &[u8],
        planes: Option<&[u8]>,
        color_table: Option<&[u8]>,
        data: &[u8],
        alignment_log2: u8,
    ) -> Vec<u8> {
        let section_count = 2 + usize::from(planes.is_some()) + usize::from(color_table.is_some());
        let directory_end = MEDIA_HEADER_LEN + section_count * MEDIA_SECTION_LEN;
        let alignment = 1usize << alignment_log2;
        let mut bodies: Vec<(MediaSectionKind, usize, &[u8])> = Vec::new();
        let mut cursor = directory_end;
        bodies.push((MediaSectionKind::SURFACE, cursor, surface));
        cursor += surface.len();
        if let Some(planes) = planes {
            bodies.push((MediaSectionKind::PLANES, cursor, planes));
            cursor += planes.len();
        }
        if let Some(color_table) = color_table {
            bodies.push((MediaSectionKind::COLOR_TABLE, cursor, color_table));
            cursor += color_table.len();
        }
        cursor = cursor.div_ceil(alignment) * alignment;
        bodies.push((MediaSectionKind::DATA, cursor, data));
        cursor += data.len();

        let mut out = vec![0; cursor + MEDIA_CRC_LEN];
        out[0] = MEDIA_VERSION;
        write_u16_le(&mut out, 2, section_count as u16);
        write_u16_le(&mut out, 4, MEDIA_SECTION_LEN as u16);
        out[6] = alignment_log2;
        write_u32_le(&mut out, 8, MEDIA_HEADER_LEN as u32);
        let payload_len = out.len() as u32;
        write_u32_le(&mut out, 12, payload_len);
        write_u32_le(&mut out, 16, data.len() as u32);
        write_u16_le(&mut out, 24, CodingId::RAW.raw());

        for (index, (kind, offset, bytes)) in bodies.into_iter().enumerate() {
            let entry = MEDIA_HEADER_LEN + index * MEDIA_SECTION_LEN;
            write_u16_le(&mut out, entry, kind.raw());
            write_u16_le(&mut out, entry + 2, MediaSectionFlags::REQUIRED.bits());
            write_u32_le(&mut out, entry + 4, offset as u32);
            write_u32_le(&mut out, entry + 8, bytes.len() as u32);
            write_u32_le(&mut out, entry + 12, bytes.len() as u32);
            out[offset..offset + bytes.len()].copy_from_slice(bytes);
        }
        reseal(&mut out);
        out
    }

    fn reseal(bytes: &mut [u8]) {
        let crc_offset = bytes.len() - MEDIA_CRC_LEN;
        let crc = crc32(&bytes[..crc_offset]);
        bytes[crc_offset..].copy_from_slice(&crc.to_le_bytes());
    }

    #[test]
    fn tight_nv12_omits_plane_records_and_borrows_two_planes() {
        let surface = SurfaceDescriptor::new(
            5,
            3,
            SampleLayout::NV12,
            ColorDescription::BT709_YUV_LIMITED,
        )
        .unwrap();
        let surface_bytes = surface_record(surface);
        let data: Vec<u8> = (0..27).collect();
        let bytes = payload(&surface_bytes, None, None, &data, 0);
        let image = RawImageView::open(&bytes).unwrap();

        assert_eq!(image.surface(), surface);
        assert_eq!(image.plane_count(), 2);
        let y = image.plane(0).unwrap();
        let uv = image.plane(1).unwrap();
        assert_eq!(y.bytes(), &data[..15]);
        assert_eq!(uv.bytes(), &data[15..]);
        assert_eq!(uv.geometry().width(), 3);
        assert_eq!(uv.memory().stride(), 6);
        assert_eq!(
            y.bytes().as_ptr(),
            image
                .media()
                .section(MediaSectionKind::DATA)
                .unwrap()
                .bytes()
                .as_ptr()
        );
    }

    #[test]
    fn indexed_image_requires_and_borrows_exact_color_table() {
        let surface =
            SurfaceDescriptor::new(5, 3, SampleLayout::I4, ColorDescription::SRGB).unwrap();
        let surface_bytes = surface_record(surface);
        let colors = [0x44; 64];
        let data = [0x12; 9];
        let bytes = payload(&surface_bytes, None, Some(&colors), &data, 0);
        let image = RawImageView::open(&bytes).unwrap();
        let table = image.color_table().unwrap();
        assert_eq!(table.len(), 16);
        assert_eq!(
            table.as_bytes().as_ptr(),
            image
                .media()
                .section(MediaSectionKind::COLOR_TABLE)
                .unwrap()
                .bytes()
                .as_ptr()
        );

        let missing = payload(&surface_bytes, None, None, &data, 0);
        assert_eq!(
            RawImageView::open(&missing),
            Err(RawImageViewError::MissingColorTable)
        );
    }

    #[test]
    fn explicit_gpu_plane_retains_padding_stride_and_alignment() {
        let surface =
            SurfaceDescriptor::new(319, 181, SampleLayout::RGBA8888, ColorDescription::SRGB)
                .unwrap();
        let surface_bytes = surface_record(surface);
        let record = plane_record(surface.plane(0).unwrap(), 0, 64, 320, 192, 1_280);
        let data = vec![0; 245_760];
        let bytes = payload(&surface_bytes, Some(&record), None, &data, 6);
        let image = RawImageView::open_at(&bytes, 0).unwrap();
        let plane = image.plane(0).unwrap();
        assert_eq!(plane.memory().allocation_width(), 320);
        assert_eq!(plane.memory().allocation_height(), 192);
        assert_eq!(plane.memory().stride(), 1_280);
        assert_eq!(plane.memory().required_alignment(), 64);
    }

    #[test]
    fn explicit_planes_reject_overlap_and_out_of_bounds() {
        let surface =
            SurfaceDescriptor::new(4, 4, SampleLayout::RGB565_A8, ColorDescription::SRGB).unwrap();
        let surface_bytes = surface_record(surface);
        let first = plane_record(surface.plane(0).unwrap(), 0, 1, 4, 4, 8);
        let overlapping = plane_record(surface.plane(1).unwrap(), 16, 1, 4, 4, 4);
        let mut records = Vec::from(first);
        records.extend_from_slice(&overlapping);
        let data = [0; 48];
        let bytes = payload(&surface_bytes, Some(&records), None, &data, 0);
        assert_eq!(
            RawImageView::open(&bytes),
            Err(RawImageViewError::PlaneRangesOverlap {
                previous: 0,
                next: 1
            })
        );

        let outside = plane_record(surface.plane(1).unwrap(), 48, 1, 4, 4, 4);
        records[PLANE_RECORD_LEN..].copy_from_slice(&outside);
        let bytes = payload(&surface_bytes, Some(&records), None, &data, 0);
        assert_eq!(
            RawImageView::open(&bytes),
            Err(RawImageViewError::PlaneDataOutOfBounds {
                index: 1,
                offset: 48,
                size: 16,
                data_size: 48,
            })
        );
    }

    #[test]
    fn relative_open_does_not_claim_file_alignment() {
        let surface =
            SurfaceDescriptor::new(16, 4, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let surface_bytes = surface_record(surface);
        let record = plane_record(surface.plane(0).unwrap(), 0, 64, 16, 4, 16);
        let data = [0; 64];
        let bytes = payload(&surface_bytes, Some(&record), None, &data, 6);
        assert!(RawImageView::open(&bytes).is_ok());
        assert!(matches!(
            RawImageView::open_at(&bytes, 4),
            Err(RawImageViewError::Media(
                MediaPayloadError::DataOffsetUnaligned { .. }
            ))
        ));
    }

    #[test]
    fn canonical_data_size_must_match_every_derived_plane() {
        let surface = SurfaceDescriptor::new(
            5,
            3,
            SampleLayout::I420,
            ColorDescription::BT709_YUV_LIMITED,
        )
        .unwrap();
        let surface_bytes = surface_record(surface);
        let short = [0; 26];
        let bytes = payload(&surface_bytes, None, None, &short, 0);
        assert_eq!(
            RawImageView::open(&bytes),
            Err(RawImageViewError::PlaneDataOutOfBounds {
                index: 2,
                offset: 21,
                size: 6,
                data_size: 26,
            })
        );

        let long = [0; 28];
        let bytes = payload(&surface_bytes, None, None, &long, 0);
        assert_eq!(
            RawImageView::open(&bytes),
            Err(RawImageViewError::CanonicalDataSizeMismatch {
                expected: 27,
                actual: 28,
            })
        );
    }

    #[test]
    fn section_cardinality_coding_and_required_flags_are_strict() {
        let surface =
            SurfaceDescriptor::new(1, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let surface_bytes = surface_record(surface);
        let mut bytes = payload(&surface_bytes, None, None, &[0], 0);

        write_u16_le(&mut bytes, 24, 7);
        reseal(&mut bytes);
        assert_eq!(
            RawImageView::open(&bytes),
            Err(RawImageViewError::UnsupportedCoding(CodingId::new(7)))
        );

        write_u16_le(&mut bytes, 24, CodingId::RAW.raw());
        write_u16_le(&mut bytes, MEDIA_HEADER_LEN + 2, 0);
        reseal(&mut bytes);
        assert_eq!(
            RawImageView::open(&bytes),
            Err(RawImageViewError::SectionMustBeRequired(
                MediaSectionKind::SURFACE
            ))
        );
    }
}
