use super::*;
use crate::{
    image::SampleLayout,
    media::{MEDIA_CRC_LEN, MEDIA_HEADER_LEN, MEDIA_SECTION_LEN, MEDIA_VERSION},
    wire::{write_u16_le, write_u32_le},
};
use alloc::{vec, vec::Vec};

fn payload(sections: &[(u16, &[u8])]) -> Vec<u8> {
    let mut offset = MEDIA_HEADER_LEN + MEDIA_SECTION_LEN * sections.len();
    let mut bytes =
        vec![
            0;
            offset + sections.iter().map(|(_, body)| body.len()).sum::<usize>() + MEDIA_CRC_LEN
        ];
    bytes[0] = MEDIA_VERSION;
    write_u16_le(&mut bytes, 2, sections.len() as u16);
    for (index, (kind, body)) in sections.iter().enumerate() {
        let entry = MEDIA_HEADER_LEN + index * MEDIA_SECTION_LEN;
        write_u16_le(&mut bytes, entry, *kind);
        write_u16_le(&mut bytes, entry + 2, MediaSectionFlags::REQUIRED.bits());
        write_u32_le(&mut bytes, entry + 4, offset as u32);
        write_u32_le(&mut bytes, entry + 8, body.len() as u32);
        bytes[offset..offset + body.len()].copy_from_slice(body);
        offset += body.len();
    }
    crate::media::refresh_checksums(&mut bytes);
    bytes
}

struct Fixture {
    face: [u8; FACE_RECORD_LEN],
    cmap: [u8; 2 * super::super::CMAP_INDEX_RECORD_LEN],
    representations: [u8; super::super::REPRESENTATION_RECORD_LEN],
    advances: [u8; 2 * ADVANCE_RECORD_LEN],
    raster_metrics: [u8; 2 * RASTER_METRICS_RECORD_LEN],
    surfaces: [u8; super::super::GLYPH_SURFACE_RECORD_LEN],
}

impl Fixture {
    fn new() -> Self {
        let face = FontFace::new(
            1_000,
            GlyphId::NOTDEF,
            2,
            Fixed::from_int(800),
            Fixed::from_int(-200),
            Fixed::from_int(100),
        )
        .unwrap();
        let mut face_record = [0; FACE_RECORD_LEN];
        face_record[..2].copy_from_slice(&face.units_per_em().to_le_bytes());
        face_record[2..4].copy_from_slice(&face.default_glyph().get().to_le_bytes());
        face_record[4..6].copy_from_slice(&face.raster_count().to_le_bytes());
        face_record[8..12].copy_from_slice(&face.ascender().to_le_bytes());
        face_record[12..16].copy_from_slice(&face.descender().to_le_bytes());
        face_record[16..20].copy_from_slice(&face.line_gap().to_le_bytes());
        let mut cmap = [0; 2 * super::super::CMAP_INDEX_RECORD_LEN];
        for (record, (scalar, glyph)) in cmap
            .chunks_exact_mut(super::super::CMAP_INDEX_RECORD_LEN)
            .zip([('A', 0_u16), ('B', 1)])
        {
            record[..4].copy_from_slice(&(scalar as u32).to_le_bytes());
            record[4..].copy_from_slice(&glyph.to_le_bytes());
        }
        let mut surfaces = [0; super::super::GLYPH_SURFACE_RECORD_LEN];
        super::super::GlyphSurfaceRecord::new(
            SampleLayout::A8,
            super::super::GlyphPacking::GlyphMajor,
            1,
            1,
            6,
        )
        .unwrap()
        .encode_record_into(&mut surfaces)
        .unwrap();
        let descriptor = crate::image::SurfaceDescriptor::new(
            1,
            2,
            SampleLayout::A8,
            crate::image::ColorDescription::NONE,
        )
        .unwrap();
        let representation = super::super::FontRepresentation::coverage(8, 16, 2).unwrap();
        let mut representations = [0; super::super::REPRESENTATION_RECORD_LEN];
        super::super::RepresentationRecord::new(representation, 0)
            .validate_for(descriptor)
            .unwrap();
        super::super::RepresentationRecord::new(representation, 0)
            .encode_record_into(&mut representations)
            .unwrap();
        let mut advances = [0; 2 * ADVANCE_RECORD_LEN];
        advances[..4].copy_from_slice(&Fixed::from_int(500).to_le_bytes());
        advances[4..].copy_from_slice(&Fixed::from_int(600).to_le_bytes());
        Self {
            face: face_record,
            cmap,
            representations,
            advances,
            raster_metrics: [0; 2 * RASTER_METRICS_RECORD_LEN],
            surfaces,
        }
    }

    fn sections(&self) -> [(u16, &[u8]); 6] {
        [
            (MediaSectionKind::RASTER_METRICS.raw(), &self.raster_metrics),
            (MediaSectionKind::FACE.raw(), &self.face),
            (MediaSectionKind::SURFACE_GROUPS.raw(), &self.surfaces),
            (MediaSectionKind::ADVANCES.raw(), &self.advances),
            (MediaSectionKind::CMAP_INDEX.raw(), &self.cmap),
            (
                MediaSectionKind::REPRESENTATIONS.raw(),
                &self.representations,
            ),
        ]
    }
}

fn shaping() -> Vec<u8> {
    const TAGS: [[u8; 4]; 8] = [
        *b"OS/2", *b"cmap", *b"glyf", *b"head", *b"hhea", *b"hmtx", *b"loca", *b"maxp",
    ];
    let directory_len =
        super::super::SFNT_HEADER_LEN + TAGS.len() * super::super::SFNT_TABLE_RECORD_LEN;
    let mut bytes = vec![0; directory_len + TAGS.len() * 4];
    bytes[..4].copy_from_slice(&[0, 1, 0, 0]);
    bytes[4..6].copy_from_slice(&(TAGS.len() as u16).to_be_bytes());
    for (index, tag) in TAGS.into_iter().enumerate() {
        let record = super::super::SFNT_HEADER_LEN + index * super::super::SFNT_TABLE_RECORD_LEN;
        bytes[record..record + 4].copy_from_slice(&tag);
        bytes[record + 8..record + 12]
            .copy_from_slice(&(directory_len as u32 + index as u32 * 4).to_be_bytes());
        bytes[record + 12..record + 16].copy_from_slice(&4_u32.to_be_bytes());
    }
    bytes
}

#[test]
fn metadata_joins_order_independent_identity_and_placement() {
    let fixture = Fixture::new();
    let bytes = payload(&fixture.sections());
    let metadata = FontMetadata::open(&bytes, &PayloadLimits::EMBEDDED).unwrap();
    assert_eq!(metadata.face().units_per_em(), 1_000);
    assert_eq!(metadata.map_char('B'), Some(GlyphId::new(1)));
    assert_eq!(metadata.glyph_id(1), Some(GlyphId::new(1)));
    assert_eq!(metadata.raster_ordinal(GlyphId::NOTDEF), Some(0));
    assert_eq!(
        metadata.advance(GlyphId::new(1)),
        Some(Fixed::from_int(600))
    );
    assert_eq!(
        metadata.raster_metrics(0, GlyphId::new(1)),
        Some(RasterMetrics::default())
    );
    assert!(metadata.shaping_data().is_none());
}

#[test]
fn metadata_requires_exactly_one_advance_source() {
    let fixture = Fixture::new();
    let sections = fixture.sections();
    let without_advances = payload(
        &sections
            .into_iter()
            .filter(|(kind, _)| *kind != MediaSectionKind::ADVANCES.raw())
            .collect::<Vec<_>>(),
    );
    assert!(matches!(
        FontMetadata::open(&without_advances, &PayloadLimits::EMBEDDED),
        Err(FontMetadataError::MissingAdvanceSource)
    ));

    let shaping = shaping();
    let mut shaped = fixture.sections().to_vec();
    shaped.retain(|(kind, _)| *kind != MediaSectionKind::ADVANCES.raw());
    shaped.push((MediaSectionKind::SHAPING.raw(), &shaping));
    let bytes = payload(&shaped);
    assert!(
        FontMetadata::open(&bytes, &PayloadLimits::EMBEDDED)
            .unwrap()
            .shaping_data()
            .is_some()
    );

    let mut conflicting = fixture.sections().to_vec();
    conflicting.push((MediaSectionKind::SHAPING.raw(), &shaping));
    assert!(matches!(
        FontMetadata::open(&payload(&conflicting), &PayloadLimits::EMBEDDED),
        Err(FontMetadataError::ConflictingAdvanceSources)
    ));
}

#[test]
fn metadata_validates_sparse_identity_and_all_cardinalities() {
    let fixture = Fixture::new();
    let mut glyph_ids = [0; 2 * GLYPH_ID_RECORD_LEN];
    glyph_ids[..2].copy_from_slice(&0_u16.to_le_bytes());
    glyph_ids[2..].copy_from_slice(&7_u16.to_le_bytes());
    let mut cmap = fixture.cmap;
    cmap[4..6].copy_from_slice(&7_u16.to_le_bytes());
    cmap[10..12].copy_from_slice(&7_u16.to_le_bytes());
    let mut sections = fixture.sections().to_vec();
    sections.retain(|(kind, _)| *kind != MediaSectionKind::CMAP_INDEX.raw());
    sections.push((MediaSectionKind::CMAP_INDEX.raw(), &cmap));
    sections.push((MediaSectionKind::GLYPH_IDS.raw(), &glyph_ids));
    let bytes = payload(&sections);
    let metadata = FontMetadata::open(&bytes, &PayloadLimits::EMBEDDED).unwrap();
    assert_eq!(metadata.map_char('A'), Some(GlyphId::new(7)));
    assert_eq!(metadata.raster_ordinal(GlyphId::new(7)), Some(1));

    let short_ids = &glyph_ids[..2];
    let mut sections = fixture.sections().to_vec();
    sections.push((MediaSectionKind::GLYPH_IDS.raw(), short_ids));
    assert!(matches!(
        FontMetadata::open(&payload(&sections), &PayloadLimits::EMBEDDED),
        Err(FontMetadataError::GlyphIdCount {
            expected: 2,
            actual: 1
        })
    ));
}

#[test]
fn metadata_rejects_cmap_targets_without_rasters_and_limit_aliasing() {
    let mut fixture = Fixture::new();
    fixture.cmap[10..12].copy_from_slice(&9_u16.to_le_bytes());
    let bytes = payload(&fixture.sections());
    assert!(matches!(
        FontMetadata::open(&bytes, &PayloadLimits::EMBEDDED),
        Err(FontMetadataError::MissingRasterGlyph(id)) if id == GlyphId::new(9)
    ));

    let fixture = Fixture::new();
    let bytes = payload(&fixture.sections());
    assert!(matches!(
        FontMetadata::open(
            &bytes,
            &PayloadLimits::EMBEDDED.with_max_font_cmap_entries(1)
        ),
        Err(FontMetadataError::TooManyCmapEntries {
            limit: 1,
            actual: 2
        })
    ));
}
