use core::ops::Range;

use super::{PLANE_RECORD_LEN, RawImageLayout, RawImageViewError, SURFACE_RECORD_LEN};
use crate::media::{
    MEDIA_SECTION_LEN, MediaLayout, MediaPayloadError, MediaSectionDescriptor, MediaSectionKind,
    SectionScan,
};

pub(super) struct RawSections<T> {
    pub surface: Option<T>,
    pub planes: Option<T>,
    pub colors: Option<T>,
    pub data: Option<T>,
}

impl<T> Default for RawSections<T> {
    fn default() -> Self {
        Self {
            surface: None,
            planes: None,
            colors: None,
            data: None,
        }
    }
}

impl<T> RawSections<T> {
    pub fn accept(
        &mut self,
        value: T,
        descriptor: MediaSectionDescriptor,
    ) -> Result<(), RawImageViewError> {
        let kind = descriptor.kind();
        let slot = match kind {
            MediaSectionKind::SURFACE => Some(&mut self.surface),
            MediaSectionKind::PLANES => Some(&mut self.planes),
            MediaSectionKind::COLOR_TABLE => Some(&mut self.colors),
            MediaSectionKind::DATA => Some(&mut self.data),
            MediaSectionKind::INTEGRITY => None,
            MediaSectionKind::CODINGS
            | MediaSectionKind::UNIT_GROUPS
            | MediaSectionKind::UNIT_INDEX => {
                return Err(RawImageViewError::UnexpectedSection(kind));
            }
            _ if descriptor.flags().is_required() => {
                return Err(RawImageViewError::UnknownRequiredSection(kind));
            }
            _ => None,
        };
        if let Some(slot) = slot {
            if slot.is_some() {
                return Err(RawImageViewError::DuplicateSection(kind));
            }
            if !descriptor.flags().is_required() {
                return Err(RawImageViewError::SectionMustBeRequired(kind));
            }
            *slot = Some(value);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Directory,
    Surface,
    Planes,
    Done,
}

/// Bounded RAW metadata scan sharing the slice reader's typed validation.
///
/// Feed nonempty prefixes of `read_range()` into `accept`. Ranges are payload-relative and may move backwards. This scan does not read DATA, color entries or verify checksums; the common metadata and integrity scans remain required.
pub struct RawImageScan {
    directory: SectionScan,
    sections: RawSections<MediaSectionDescriptor>,
    record: [u8; MEDIA_SECTION_LEN],
    surface: [u8; SURFACE_RECORD_LEN],
    planes: [u8; super::borrowed::MAX_PLANES * PLANE_RECORD_LEN],
    phase: Phase,
    filled: usize,
    payload_offset: u32,
}

impl RawImageScan {
    pub fn new(layout: MediaLayout, payload_offset: u32) -> Self {
        Self {
            directory: layout.sections(),
            sections: RawSections::default(),
            record: [0; MEDIA_SECTION_LEN],
            surface: [0; SURFACE_RECORD_LEN],
            planes: [0; super::borrowed::MAX_PLANES * PLANE_RECORD_LEN],
            phase: Phase::Directory,
            filled: 0,
            payload_offset,
        }
    }

    pub fn read_range(&self) -> Option<Range<usize>> {
        let mut range = match self.phase {
            Phase::Directory => self.directory.record_range()?,
            Phase::Surface => range(self.sections.surface?),
            Phase::Planes => range(self.sections.planes?),
            Phase::Done => return None,
        };
        range.start += self.filled;
        Some(range)
    }

    pub fn accept(&mut self, bytes: &[u8]) -> Result<(), RawImageViewError> {
        let requested = self.read_range().ok_or(RawImageViewError::Media(
            MediaPayloadError::InvalidDataRange,
        ))?;
        if bytes.is_empty() || bytes.len() > requested.len() {
            return Err(RawImageViewError::Media(
                MediaPayloadError::InvalidDataRange,
            ));
        }
        let next = self.filled + bytes.len();
        match self.phase {
            Phase::Directory => {
                self.record[self.filled..next].copy_from_slice(bytes);
                if next == self.record.len() {
                    let mut directory = self.directory;
                    let descriptor = directory
                        .accept(&self.record)
                        .map_err(RawImageViewError::Media)?
                        .expect("requested record");
                    self.sections.accept(descriptor, descriptor)?;
                    self.directory = directory;
                }
            }
            Phase::Surface => self.surface[self.filled..next].copy_from_slice(bytes),
            Phase::Planes => self.planes[self.filled..next].copy_from_slice(bytes),
            Phase::Done => unreachable!(),
        }
        self.filled = next;
        if bytes.len() == requested.len() {
            self.filled = 0;
            match self.phase {
                Phase::Directory if self.directory.record_range().is_none() => {
                    let surface = self
                        .sections
                        .surface
                        .ok_or(RawImageViewError::MissingSection(MediaSectionKind::SURFACE))?;
                    check_size(surface, SURFACE_RECORD_LEN)?;
                    self.sections
                        .data
                        .ok_or(RawImageViewError::MissingSection(MediaSectionKind::DATA))?;
                    self.phase = Phase::Surface;
                }
                Phase::Surface => {
                    let surface = super::raw_layout::parse_surface(&self.surface)?;
                    surface.validate_color_table_size(
                        self.sections.colors.map(|s| s.size() as usize),
                    )?;
                    self.phase = match self.sections.planes {
                        Some(records) => {
                            let expected = surface.plane_count() as usize * PLANE_RECORD_LEN;
                            check_size(records, expected)?;
                            if expected > self.planes.len() {
                                return Err(RawImageViewError::SizeOverflow);
                            }
                            Phase::Planes
                        }
                        None => Phase::Done,
                    };
                }
                Phase::Planes => self.phase = Phase::Done,
                _ => {}
            }
        }
        Ok(())
    }

    /// Finish typed metadata checks. This does not establish DATA checksum validity.
    pub fn finish(&self) -> Result<RawImageLayout<'_>, RawImageViewError> {
        if self.phase != Phase::Done {
            return Err(RawImageViewError::Media(
                MediaPayloadError::InvalidDataRange,
            ));
        }
        let data = self.sections.data.expect("checked DATA section");
        RawImageLayout::parse(
            &self.surface,
            self.sections
                .planes
                .map(|s| &self.planes[..s.size() as usize]),
            data.size(),
            Some(
                self.payload_offset
                    .checked_add(data.offset())
                    .ok_or(RawImageViewError::SizeOverflow)?,
            ),
        )
    }

    pub fn data_range(&self) -> Option<Range<usize>> {
        self.sections.data.map(range)
    }
}

fn range(section: MediaSectionDescriptor) -> Range<usize> {
    section.offset() as usize..(section.offset() as usize + section.size() as usize)
}

fn check_size(section: MediaSectionDescriptor, expected: usize) -> Result<(), RawImageViewError> {
    let actual = section.size() as usize;
    if actual == expected {
        Ok(())
    } else {
        Err(RawImageViewError::SectionSizeMismatch {
            kind: section.kind(),
            expected,
            actual,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::{
        ColorDescription, RawImageAsset, RawImageView, SampleLayout, SurfaceDescriptor,
        SurfaceRequirements,
    };
    use alloc::{vec, vec::Vec};

    #[test]
    fn metadata_windows_match_static_plane_and_color_table_validation() {
        for layout in [SampleLayout::RGBA8888, SampleLayout::I420, SampleLayout::I4] {
            let color = if layout == SampleLayout::I420 {
                ColorDescription::BT709_YUV_LIMITED
            } else {
                ColorDescription::SRGB
            };
            let surface = SurfaceDescriptor::new(5, 3, layout, color).unwrap();
            let plan = surface
                .memory_plan(SurfaceRequirements::new().with_stride_multiple(8))
                .unwrap();
            let memory: Vec<_> = plan.planes().collect();
            let data: Vec<_> = memory
                .iter()
                .map(|plane| vec![0; plane.byte_len() as usize])
                .collect();
            let planes: Vec<_> = data.iter().map(Vec::as_slice).collect();
            let asset = RawImageAsset::new(surface, &planes).with_memory_layouts(&memory);
            let colors = [0; 64];
            let asset = if layout == SampleLayout::I4 {
                asset.with_color_table(&colors)
            } else {
                asset
            };
            let payload = asset.encode().unwrap();
            let expected = RawImageView::open_at(&payload, 0).unwrap();
            for count in [1, 7, 32] {
                let media = MediaLayout::parse(&payload[..8], payload.len()).unwrap();
                let mut scan = RawImageScan::new(media, 0);
                assert!(scan.finish().is_err());
                while let Some(range) = scan.read_range() {
                    scan.accept(&payload[range.start..range.start + range.len().min(count)])
                        .unwrap();
                }
                let actual = scan.finish().unwrap();
                assert_eq!(actual.surface(), surface);
                for plane in 0..surface.plane_count() {
                    assert_eq!(
                        actual.plane(plane),
                        Some(expected.plane(plane).unwrap().memory())
                    );
                }
            }
        }
    }
}
