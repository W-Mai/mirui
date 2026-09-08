use super::*;
use crate::{
    Fixed,
    coding::Rle,
    font::{
        CmapEntry, FontAdvanceSource, FontAsset, FontFace, FontRepresentation,
        FontRepresentationRequest, GlyphId, GlyphMap, GlyphPacking, GlyphSurfaceAsset,
        RasterMetrics, RawGlyphs, RepresentationAsset, RepresentationRecord,
    },
    image::{PlaneMemoryLayout, SampleLayout, SurfaceRequirements},
    media::{CodingTable, MEDIA_CRC_LEN, MEDIA_HEADER_LEN, MEDIA_SECTION_LEN, MEDIA_VERSION},
    wire::{write_u16_le, write_u32_le},
};
use alloc::{vec, vec::Vec};

fn payload(sections: &[(MediaSectionKind, &[u8])]) -> Vec<u8> {
    let mut offset = MEDIA_HEADER_LEN + MEDIA_SECTION_LEN * sections.len();
    let mut out =
        vec![
            0;
            offset + sections.iter().map(|(_, bytes)| bytes.len()).sum::<usize>() + MEDIA_CRC_LEN
        ];
    out[0] = MEDIA_VERSION;
    write_u16_le(&mut out, 2, sections.len() as u16);
    for (index, (kind, bytes)) in sections.iter().enumerate() {
        let entry = MEDIA_HEADER_LEN + index * MEDIA_SECTION_LEN;
        write_u16_le(&mut out, entry, kind.raw());
        write_u16_le(&mut out, entry + 2, 1);
        write_u32_le(&mut out, entry + 4, offset as u32);
        write_u32_le(&mut out, entry + 8, bytes.len() as u32);
        out[offset..offset + bytes.len()].copy_from_slice(bytes);
        offset += bytes.len();
    }
    crate::media::refresh_checksums(&mut out);
    out
}

struct Fixture {
    face: [u8; 20],
    cmap: [u8; 12],
    records: [u8; 48],
    advances: [u8; 8],
    raster_metrics: [u8; 48],
    surfaces: [u8; 48],
    codings: [u8; 12],
    data: Vec<u8>,
    planes: Vec<u8>,
}
impl Fixture {
    fn new() -> Self {
        let mut result = Self {
            face: [0; 20],
            cmap: [0; 12],
            records: [0; 48],
            advances: [0; 8],
            raster_metrics: [0; 48],
            surfaces: [0; 48],
            codings: [0; 12],
            data: vec![7; 8],
            planes: Vec::new(),
        };
        FontFace::new(
            1_000,
            GlyphId::NOTDEF,
            2,
            Fixed::from_int(750),
            Fixed::from_int(-250),
            Fixed::from_int(200),
        )
        .unwrap()
        .encode_record_into(&mut result.face)
        .unwrap();
        for (record, entry) in result.cmap.chunks_exact_mut(6).zip([
            CmapEntry::new('A', GlyphId::new(0)),
            CmapEntry::new('B', GlyphId::new(1)),
        ]) {
            entry.encode_record_into(record).unwrap();
        }
        result.advances[..4].copy_from_slice(&Fixed::from_int(500).to_le_bytes());
        result.advances[4..].copy_from_slice(&Fixed::from_int(600).to_le_bytes());
        for (index, metadata) in [
            FontRepresentation::coverage(8, 12, 8).unwrap(),
            FontRepresentation::coverage(8, 16, 8).unwrap(),
            FontRepresentation::signed_distance(8, 3, 24, 17, 48, 8).unwrap(),
        ]
        .into_iter()
        .enumerate()
        {
            RepresentationRecord::new(metadata, u16::from(index == 2))
                .encode_record_into(&mut result.records[index * 16..])
                .unwrap();
            for ordinal in 0..2 {
                RasterMetrics::new(Fixed::ZERO, Fixed::from_int(metadata.design_ppem().into()))
                    .encode_record_into(&mut result.raster_metrics[(index * 2 + ordinal) * 8..])
                    .unwrap();
            }
        }
        GlyphSurfaceRecord::new(SampleLayout::A8, GlyphPacking::GlyphMajor, 2, 2, 7)
            .unwrap()
            .encode_record_into(&mut result.surfaces)
            .unwrap();
        GlyphSurfaceRecord::new(SampleLayout::A8, GlyphPacking::GlyphMajor, 2, 2, 8)
            .unwrap()
            .with_codings(6)
            .unwrap()
            .encode_record_into(&mut result.surfaces[24..])
            .unwrap();
        CodingTable::encode_into(&[Rle::new().record()], &mut result.codings).unwrap();
        result
    }
    fn bytes(&self) -> Vec<u8> {
        let mut sections = vec![
            (MediaSectionKind::FACE, &self.face[..]),
            (MediaSectionKind::CMAP_INDEX, &self.cmap[..]),
            (MediaSectionKind::REPRESENTATIONS, &self.records),
            (MediaSectionKind::ADVANCES, &self.advances),
            (MediaSectionKind::RASTER_METRICS, &self.raster_metrics),
            (MediaSectionKind::SURFACE_GROUPS, &self.surfaces),
            (MediaSectionKind::CODINGS, &self.codings),
            (MediaSectionKind::DATA, &self.data),
            (MediaSectionKind::DATA, &[0x87, 42][..]),
        ];
        if !self.planes.is_empty() {
            sections.push((MediaSectionKind::PLANES, &self.planes));
        }
        payload(&sections)
    }
}

#[test]
fn one_face_binds_multiple_representations_and_shared_raw_encoded_storage() {
    let bytes = Fixture::new().bytes();
    let view = FontView::open(&bytes, &PayloadLimits::EMBEDDED).unwrap();
    view.preflight(&PayloadLimits::EMBEDDED).unwrap();
    assert_eq!(view.surface_count(), 2);
    assert_eq!(view.representations().len(), 3);
    let chosen = view.select(FontRepresentationRequest::new(24)).unwrap();
    assert_eq!(chosen.index(), 2);
    assert_eq!(
        chosen.raster_metrics(GlyphId::new(1)).unwrap().offset_y(),
        Fixed::from_int(24)
    );
    for index in 0..2 {
        let FontGlyphs::Raw(glyphs) = view.glyphs(index).unwrap() else {
            panic!("RAW");
        };
        assert_eq!(
            glyphs.get(1).unwrap().storage().plane(0).unwrap().bytes(),
            &[7; 4]
        );
    }
    let FontGlyphs::Encoded(glyphs) = view.glyphs(2).unwrap() else {
        panic!("encoded");
    };
    let mut slots = [None];
    let groups = glyphs
        .groups_into(&mut slots, &mut CoverageBudget::new(1000))
        .unwrap();
    let plan = groups
        .decode_plan(1, SurfaceRequirements::new(), &PayloadLimits::EMBEDDED)
        .unwrap();
    let mut output = [0; 4];
    plan.decode_into(&mut output, &mut [0; 8]).unwrap();
    assert_eq!(output, [42; 4]);
    assert!(view.glyphs(usize::MAX).is_none());
}

#[test]
fn unique_surfaces_share_one_preflight_budget_without_duplicate_representation_work() {
    let mut fixture = Fixture::new();
    GlyphSurfaceRecord::from_record(&fixture.surfaces)
        .unwrap()
        .with_codings(6)
        .unwrap()
        .encode_record_into(&mut fixture.surfaces)
        .unwrap();
    fixture.data = vec![0x87, 7];
    let bytes = fixture.bytes();
    let view = FontView::open(&bytes, &PayloadLimits::EMBEDDED).unwrap();
    let exact = PayloadLimits::EMBEDDED
        .with_max_raster_groups(2)
        .with_max_raster_units(2);
    view.preflight(&exact).unwrap();
    assert!(view.preflight(&exact.with_max_raster_groups(1)).is_err());
    assert!(view.preflight(&exact.with_max_raster_units(1)).is_err());
    assert!(
        view.preflight(&exact.with_max_font_representations(2))
            .is_err()
    );
    assert!(view.preflight(&exact.with_max_font_glyphs(1)).is_err());
    assert!(FontView::open(&bytes, &exact.with_max_raster_work(0)).is_err());
}

#[test]
fn metadata_inspection_and_complete_integrity_have_distinct_failure_boundaries() {
    let mut bytes = Fixture::new().bytes();
    let start = MediaPayload::open(&bytes)
        .unwrap()
        .get(7)
        .unwrap()
        .descriptor()
        .offset() as usize;
    bytes[start] ^= 1;
    let view = FontView::open(&bytes, &PayloadLimits::EMBEDDED).unwrap();
    assert!(view.validate_data().is_err());
    assert!(view.preflight(&PayloadLimits::EMBEDDED).is_err());
    assert!(view.glyphs(0).is_some());
    let mut fixture = Fixture::new();
    fixture.codings[4..6].copy_from_slice(&500u16.to_le_bytes());
    let bytes = fixture.bytes();
    let view = FontView::open(&bytes, &PayloadLimits::EMBEDDED).unwrap();
    assert!(matches!(
        view.preflight(&PayloadLimits::EMBEDDED),
        Err(FontError::EncodedSurface { index: 1, .. })
    ));
}

#[test]
fn referenced_raw_alignment_checks_file_position_not_slice_alignment() {
    let mut fixture = Fixture::new();
    let geometry = SampleLayout::A8.plane_geometry(2, 2, 0).unwrap();
    fixture.planes.resize(24, 0);
    PlaneMemoryLayout::builder(geometry)
        .with_stride(64)
        .with_alignment(crate::ByteAlignment::new(64).unwrap())
        .build()
        .unwrap()
        .encode_record_into(&mut fixture.planes)
        .unwrap();
    fixture.data.resize(256, 7);
    GlyphSurfaceRecord::from_record(&fixture.surfaces)
        .unwrap()
        .with_planes(9)
        .unwrap()
        .encode_record_into(&mut fixture.surfaces)
        .unwrap();
    let bytes = fixture.bytes();
    let offset = MediaPayload::open(&bytes)
        .unwrap()
        .get(7)
        .unwrap()
        .descriptor()
        .offset();
    let base = (64 - offset % 64) % 64;
    let view = FontView::open_at(&bytes, base, &PayloadLimits::EMBEDDED).unwrap();
    assert_eq!(
        view.input_alignment().map(crate::ByteAlignment::get),
        Ok(64)
    );
    assert!(matches!(
        FontView::open_at(&bytes, base + 1, &PayloadLimits::EMBEDDED),
        Err(FontError::FileAddressUnaligned { surface: 0, .. })
    ));
    assert!(matches!(
        FontView::open_at(&bytes, u32::MAX, &PayloadLimits::EMBEDDED),
        Err(FontError::SizeOverflow)
    ));
}

#[test]
fn canonical_faces_solve_every_surface_alignment_from_any_container_cursor() {
    let cmap = [
        CmapEntry::new('A', GlyphId::new(0)),
        CmapEntry::new('B', GlyphId::new(1)),
    ];
    let advances = [Fixed::from_int(2); 2];
    let map8 = GlyphMap::glyph_major(2, 2, cmap.len()).unwrap();
    let map4 = GlyphMap::glyph_major(2, 2, cmap.len()).unwrap();
    let geometry8 = SampleLayout::A8.plane_geometry(2, 2, 0).unwrap();
    let geometry4 = SampleLayout::A4.plane_geometry(2, 2, 0).unwrap();
    let memory8 = PlaneMemoryLayout::builder(geometry8)
        .with_stride(64)
        .with_alignment(crate::ByteAlignment::new(64).unwrap())
        .build()
        .unwrap();
    let memory4 = PlaneMemoryLayout::builder(geometry4)
        .with_stride(16)
        .with_alignment(crate::ByteAlignment::new(16).unwrap())
        .build()
        .unwrap();
    let samples8 = [0x80; 256];
    let samples4 = [0x40; 64];
    let glyphs8 = RawGlyphs::builder(map8, SampleLayout::A8)
        .with_memory_layout(memory8)
        .build(&samples8)
        .unwrap();
    let glyphs4 = RawGlyphs::builder(map4, SampleLayout::A4)
        .with_memory_layout(memory4)
        .build(&samples4)
        .unwrap();
    let surfaces = [
        GlyphSurfaceAsset::raw(glyphs8),
        GlyphSurfaceAsset::raw(glyphs4),
    ];
    let representations = [
        RepresentationAsset::new(FontRepresentation::coverage(8, 12, 8).unwrap(), 0),
        RepresentationAsset::new(FontRepresentation::coverage(4, 16, 4).unwrap(), 1),
    ];
    let face = FontFace::new(
        1_000,
        GlyphId::NOTDEF,
        2,
        Fixed::from_int(750),
        Fixed::from_int(-250),
        Fixed::from_int(200),
    )
    .unwrap();
    let raster_metrics = [RasterMetrics::default(); 4];
    let bytes = FontAsset::new(face, &cmap, FontAdvanceSource::Advances(&advances))
        .with_rasters(&representations, &raster_metrics, &surfaces)
        .encode()
        .unwrap();
    let view = FontView::open(&bytes, &PayloadLimits::EMBEDDED).unwrap();
    assert_eq!(
        view.input_alignment().map(crate::ByteAlignment::get),
        Ok(64)
    );

    for cursor in 0..128 {
        let placed = view.aligned_file_offset(cursor, 4).unwrap();
        assert!(placed >= cursor);
        assert!(placed - cursor < 64);
        FontView::open_at(&bytes, placed, &PayloadLimits::EMBEDDED).unwrap();
    }
}

#[test]
fn directory_ownership_and_required_sections_are_unambiguous() {
    let original = Fixture::new().bytes();
    let inspect = |bytes: &[u8]| FontView::open(bytes, &PayloadLimits::EMBEDDED).map(|_| ());
    for (index, kind) in [
        MediaSectionKind::FACE,
        MediaSectionKind::CMAP_INDEX,
        MediaSectionKind::REPRESENTATIONS,
        MediaSectionKind::ADVANCES,
        MediaSectionKind::RASTER_METRICS,
        MediaSectionKind::SURFACE_GROUPS,
    ]
    .into_iter()
    .enumerate()
    {
        let mut bytes = original.clone();
        let entry = MEDIA_HEADER_LEN + index * MEDIA_SECTION_LEN;
        write_u16_le(&mut bytes, entry, 500);
        write_u16_le(&mut bytes, entry + 2, 0);
        crate::media::refresh_checksums(&mut bytes);
        let missing = if kind == MediaSectionKind::ADVANCES {
            FontMetadataError::MissingAdvanceSource
        } else {
            FontMetadataError::MissingSection(kind)
        };
        assert_eq!(inspect(&bytes), Err(FontError::Metadata(missing)));
        for flags in [0, 2, 3, u16::MAX] {
            let mut bytes = original.clone();
            write_u16_le(&mut bytes, entry + 2, flags);
            crate::media::refresh_checksums(&mut bytes);
            assert_eq!(
                inspect(&bytes),
                Err(FontError::Metadata(FontMetadataError::SectionFlags(kind)))
            );
        }
    }
    let media = MediaPayload::open(&original).unwrap();
    let mut sections: Vec<_> = media
        .sections()
        .map(|section| (section.descriptor().kind(), section.bytes()))
        .collect();
    for (kind, body, expected) in [
        (
            MediaSectionKind::FACE,
            &[0; 20][..],
            FontError::Metadata(FontMetadataError::DuplicateSection(MediaSectionKind::FACE)),
        ),
        (
            MediaSectionKind::ATLAS_MAPS,
            &[][..],
            FontError::EmptyMapSection,
        ),
        (
            MediaSectionKind::PLANES,
            &[][..],
            FontError::UnreferencedSection { index: 9 },
        ),
        (
            MediaSectionKind::DATA,
            &[][..],
            FontError::UnreferencedSection { index: 9 },
        ),
        (
            MediaSectionKind::new(500).unwrap(),
            &[1, 2, 3][..],
            FontError::Metadata(FontMetadataError::UnknownRequiredSection(
                MediaSectionKind::new(500).unwrap(),
            )),
        ),
    ] {
        sections.push((kind, body));
        assert_eq!(inspect(&payload(&sections)), Err(expected));
        sections.pop();
    }
    sections.push((MediaSectionKind::new(500).unwrap(), &[1, 2, 3]));
    let mut optional = payload(&sections);
    write_u16_le(
        &mut optional,
        MEDIA_HEADER_LEN + 9 * MEDIA_SECTION_LEN + 2,
        0,
    );
    crate::media::refresh_checksums(&mut optional);
    let view = FontView::open(&optional, &PayloadLimits::EMBEDDED).unwrap();
    assert_eq!(view.media().get(9).unwrap().bytes(), &[1, 2, 3]);
    view.preflight(&PayloadLimits::EMBEDDED).unwrap();
}

#[test]
fn atlas_maps_and_empty_samples_keep_exact_table_ownership() {
    let mut fixture = Fixture::new();
    for index in 0..2 {
        let data_section = 7 + index as u16;
        let mut record =
            GlyphSurfaceRecord::new(SampleLayout::A8, GlyphPacking::Atlas2D, 0, 0, data_section)
                .unwrap();
        if index == 1 {
            record = record.with_codings(6).unwrap();
        }
        record
            .encode_record_into(&mut fixture.surfaces[index * 24..])
            .unwrap();
    }
    let original = fixture.bytes();
    let media = MediaPayload::open(&original).unwrap();
    let mut sections: Vec<_> = media
        .sections()
        .map(|section| (section.descriptor().kind(), section.bytes()))
        .collect();
    sections[7].1 = &[];
    sections[8].1 = &[];
    let map = [0; 32];
    sections.push((MediaSectionKind::ATLAS_MAPS, &map));
    let bytes = payload(&sections);
    let view = FontView::open(&bytes, &PayloadLimits::EMBEDDED).unwrap();
    view.preflight(&PayloadLimits::EMBEDDED).unwrap();
    assert_eq!(
        view.representation(0)
            .unwrap()
            .map()
            .get(0)
            .unwrap()
            .width(),
        0
    );
    let FontGlyphs::Raw(raw) = view.glyphs(0).unwrap() else {
        panic!("RAW");
    };
    assert!(
        raw.get(1)
            .unwrap()
            .storage()
            .plane(0)
            .unwrap()
            .bytes()
            .is_empty()
    );

    let orphan = [0; 64];
    sections[9].1 = &orphan;
    assert!(matches!(
        FontView::open(&payload(&sections), &PayloadLimits::EMBEDDED),
        Err(FontError::UnreferencedMaps)
    ));
    sections.pop();
    assert!(matches!(
        FontView::open(&payload(&sections), &PayloadLimits::EMBEDDED),
        Err(FontError::MapOutOfBounds { .. })
    ));
}
