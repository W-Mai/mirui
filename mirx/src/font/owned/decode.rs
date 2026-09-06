use super::*;
use crate::{
    font::{FontError, FontGlyphs, FontView, GLYPH_REGION_LEN},
    image::{RasterPreflight, UNIT_GROUP_RECORD_LEN},
    media::{IntegrityRanges, MediaPayload, MediaSectionKind},
};

impl Font {
    /// Decodes stored components using the host resource profile.
    pub fn decode(bytes: &[u8]) -> Result<Self, FontError> {
        Self::decode_with_limits(bytes, &PayloadLimits::HOST)
    }
    /// Copies stored bytes and native metadata after complete bounded admission.
    /// Codec samples are validated but remain encoded in the owned value.
    pub fn decode_with_limits(bytes: &[u8], limits: &PayloadLimits) -> Result<Self, FontError> {
        let view = FontView::open(bytes, limits)?;
        let media = view.media();
        for section in media.sections() {
            match section.descriptor().kind() {
                MediaSectionKind::CODEPOINTS
                | MediaSectionKind::REPRESENTATIONS
                | MediaSectionKind::METRICS
                | MediaSectionKind::GLYPH_MAPS
                | MediaSectionKind::SURFACE_GROUPS
                | MediaSectionKind::DATA
                | MediaSectionKind::PLANES
                | MediaSectionKind::CODINGS
                | MediaSectionKind::UNIT_GROUPS
                | MediaSectionKind::UNIT_INDEX
                | MediaSectionKind::INTEGRITY => {}
                kind => return Err(FontError::UnsupportedSection(kind)),
            }
        }
        let mut preflight = RasterPreflight::new(limits, 0).map_err(FontError::Raster)?;
        view.preflight_in(&mut preflight)?;
        let glyphs = view.tables().glyph_count();
        let representation_count = view.tables().len();
        let map_len = glyphs * GLYPH_REGION_LEN;
        let map_count = media
            .section(MediaSectionKind::GLYPH_MAPS)
            .map_or(0, |s| s.bytes().len() / map_len);
        let mut size = OwnedSize::new(limits.max_decoded_bytes());
        size.items::<char>(glyphs)?;
        size.items::<Representation>(representation_count)?;
        size.items::<GlyphMetrics>(glyphs * representation_count)?;
        size.items::<Map>(map_count)?;
        size.items::<Region>(map_count * glyphs)?;
        size.items::<Surface>(view.surface_count())?;
        for index in 0..view.surface_count() {
            let record = view.surface_record(index);
            for id in [
                Some(record.data_section()),
                record.codings_section(),
                record.index_section(),
            ]
            .into_iter()
            .flatten()
            {
                size.items::<u8>(
                    media
                        .get(usize::from(id))
                        .expect("validated section")
                        .bytes()
                        .len(),
                )?;
            }
            if let Some(id) = record.groups_section() {
                size.items::<UnitGroupRecord>(
                    media
                        .get(usize::from(id))
                        .expect("validated groups")
                        .bytes()
                        .len()
                        / UNIT_GROUP_RECORD_LEN,
                )?;
            }
            size.items::<u32>(
                Self::partitions(media, record.data_section()).map_or(0, |p| p.len()),
            )?;
        }
        // Persistent-byte copies and metadata visits share the existing raster budget.
        // Two binary searches per surface need at most64 comparisons for u32 payloads.
        preflight
            .spend(
                size.used as u64
                    + view.surface_count() as u64 * (64 + representation_count as u64)
                    + map_count as u64 * representation_count as u64,
            )
            .map_err(FontError::Raster)?;

        let mut codepoints = Self::reserve(glyphs)?;
        codepoints.extend(view.tables().codepoints());
        let mut representations = Self::reserve(representation_count)?;
        for index in 0..representation_count {
            let source = view.tables().get(index).expect("representation ordinal");
            let mut metrics = Self::reserve(glyphs)?;
            metrics.extend(source.metrics().iter());
            representations.push(Representation {
                metadata: source.record().representation(),
                surface: source.record().surface_index(),
                line: source.metrics().line_metrics(),
                metrics,
                map: (source.map().packing() == GlyphPacking::Atlas2D)
                    .then_some(source.record().glyph_map_offset() / map_len as u32),
            });
        }
        let mut maps = Self::reserve(map_count)?;
        for index in 0..map_count {
            let representation = (0..representation_count)
                .map(|r| view.tables().get(r).expect("representation ordinal"))
                .find(|r| {
                    r.map().packing() == GlyphPacking::Atlas2D
                        && r.record().glyph_map_offset() as usize == index * map_len
                })
                .expect("referenced map");
            let map = representation.map();
            let mut regions = Self::reserve(glyphs)?;
            regions.extend(map);
            maps.push(Map {
                width: map.width(),
                height: map.height(),
                regions,
            });
        }
        let mut surfaces = Self::reserve(view.surface_count())?;
        for index in 0..view.surface_count() {
            let record = view.surface_record(index);
            let data = media
                .get(usize::from(record.data_section()))
                .expect("validated DATA");
            let partitions = Self::partitions(media, record.data_section())
                .map(|ranges| {
                    let mut ends = Self::reserve(ranges.len())?;
                    ends.extend(ranges.map(|range| range.range().end - data.descriptor().offset()));
                    Ok::<_, FontError>(ends)
                })
                .transpose()?;
            let storage = if let Some(codings) = record.codings_section() {
                let groups = record
                    .groups_section()
                    .map(|id| {
                        let bytes = media
                            .get(usize::from(id))
                            .expect("validated groups")
                            .bytes();
                        let mut records = Self::reserve(bytes.len() / UNIT_GROUP_RECORD_LEN)?;
                        for bytes in bytes.chunks_exact(UNIT_GROUP_RECORD_LEN) {
                            records.push(UnitGroupRecord::open(bytes).expect("preflighted group"));
                        }
                        Ok::<_, FontError>(records)
                    })
                    .transpose()?;
                Storage::Encoded {
                    codings: Self::copy(
                        media
                            .get(usize::from(codings))
                            .expect("validated coding body")
                            .bytes(),
                    )?,
                    groups,
                    index: Self::copy(record.index_section().map_or(&[], |id| {
                        media.get(usize::from(id)).expect("validated index").bytes()
                    }))?,
                    partitions,
                    alignment: crate::ByteAlignment::ONE,
                    data: Self::copy(data.bytes())?,
                }
            } else {
                let representation = view
                    .surface_representation(index)
                    .expect("referenced surface");
                let FontGlyphs::Raw(raw) =
                    view.glyphs(representation.index()).expect("valid storage")
                else {
                    unreachable!()
                };
                Storage::Raw {
                    memory: raw.memory_layout(),
                    data: Self::copy(data.bytes())?,
                    partitions,
                }
            };
            surfaces.push(Surface {
                width: record.width(),
                height: record.height(),
                layout: record.sample_layout(),
                packing: record.packing(),
                storage,
            });
        }
        Ok(Self {
            codepoints,
            representations,
            surfaces,
            maps,
        })
    }

    fn partitions(media: MediaPayload<'_>, data: u16) -> Option<IntegrityRanges<'_>> {
        let data = media
            .get(usize::from(data))
            .expect("validated DATA")
            .descriptor();
        media
            .integrity()
            .map(|table| table.intersecting(data.offset()..data.offset() + data.size()))
    }
}
