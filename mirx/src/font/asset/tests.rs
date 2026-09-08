use super::*;
use crate::{
    coding::Rle,
    font::{FontGlyphs, FontRepresentationRequest, FontView},
    image::{AtlasMap, Region, SampleLayout, SurfaceRequirements},
    media::MediaPayload,
    types::Fixed,
};
use alloc::vec;

fn with_face(aligned: bool, indexed: bool, check: impl FnOnce(FontAsset<'_>)) {
    let cmap = [
        CmapEntry::new('A', GlyphId::new(0)),
        CmapEntry::new('B', GlyphId::new(1)),
    ];
    let advances = [Fixed::from_ratio(5, 2); 2];
    let map = GlyphMap::cells(2, 2, cmap.len()).unwrap();
    let raw_data = vec![7; if aligned { 256 } else { 8 }];
    let mut raw = RawGlyphs::builder(map, SampleLayout::A8);
    if aligned {
        raw = raw.with_memory_layout(
            PlaneMemoryLayout::builder(SampleLayout::A8.plane_geometry(2, 2, 0).unwrap())
                .with_stride(64)
                .with_alignment(crate::ByteAlignment::new(64).unwrap())
                .build()
                .unwrap(),
        );
    }
    let image = EncodedImageAsset::new(
        SurfaceDescriptor::new(2, 4, SampleLayout::A8, ColorDescription::NONE).unwrap(),
        Rle::new().record(),
        &[0x87, 42],
    )
    .with_input_alignment(crate::ByteAlignment::new(if aligned { 64 } else { 1 }).unwrap())
    .with_integrity(if indexed {
        DataIntegrity::Indexed(&[1, 2])
    } else {
        DataIntegrity::Whole
    });
    let surfaces = [
        GlyphSurfaceAsset::raw(raw.build(&raw_data).unwrap()),
        GlyphSurfaceAsset::Encoded { map, image },
    ];
    let representations = [
        RepresentationAsset::new(FontRepresentation::coverage(8, 12, 8).unwrap(), 0),
        RepresentationAsset::new(FontRepresentation::coverage(8, 16, 8).unwrap(), 0),
        RepresentationAsset::new(
            FontRepresentation::signed_distance(8, 3, 24, 17, 48, 8).unwrap(),
            1,
        ),
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
    let raster_metrics = [RasterMetrics::new(Fixed::from_ratio(-1, 2), Fixed::from_int(2)); 6];
    check(
        FontAsset::new(face, &cmap, FontAdvanceSource::Advances(&advances)).with_rasters(
            &representations,
            &raster_metrics,
            &surfaces,
        ),
    );
}

#[test]
fn canonical_native_face_reuses_surfaces_and_reader_geometry() {
    with_face(false, false, |asset| {
        asset.preflight(&PayloadLimits::EMBEDDED).unwrap();
        let len = asset.encoded_len().unwrap();
        let mut bytes = vec![0x5a; len + 13];
        assert_eq!(asset.encode_into(&mut bytes), Ok(len));
        assert_eq!(&bytes[len..], &[0x5a; 13]);
        assert!(asset.matches_payload(&bytes[..len]).unwrap());
        assert!(!asset.matches_payload(&bytes).unwrap());
        assert_eq!(asset.encode().unwrap(), &bytes[..len]);
        let view = FontView::open(&bytes[..len], &PayloadLimits::EMBEDDED).unwrap();
        view.preflight(&PayloadLimits::EMBEDDED).unwrap();
        assert_eq!(view.media().header().section_count(), 9);
        assert_eq!(
            view.media()
                .sections_of_kind(MediaSectionKind::DATA)
                .count(),
            2
        );
        assert_eq!(
            view.media()
                .sections_of_kind(MediaSectionKind::ATLAS_MAPS)
                .count(),
            0
        );
        assert_eq!(
            view.media()
                .sections_of_kind(MediaSectionKind::PLANES)
                .count(),
            0
        );
        assert_eq!(
            view.cmap()
                .iter()
                .map(|entry| entry.scalar())
                .collect::<Vec<_>>(),
            ['A', 'B']
        );
        let selected = view.select(FontRepresentationRequest::new(24)).unwrap();
        assert_eq!(selected.index(), 2);
        assert_eq!(
            selected.raster_metrics(GlyphId::new(1)),
            Some(asset.raster_metrics[5])
        );
        assert_eq!(
            selected.advance(GlyphId::new(1)),
            Some(Fixed::from_ratio(5, 2))
        );
        let FontGlyphs::Encoded(glyphs) = view.glyphs(2).unwrap() else {
            panic!("encoded");
        };
        let mut slots = [None];
        let groups = glyphs
            .groups_into(&mut slots, &mut crate::image::CoverageBudget::new(1000))
            .unwrap();
        let plan = groups
            .decode_plan(1, SurfaceRequirements::new(), &PayloadLimits::EMBEDDED)
            .unwrap();
        let mut out = [0; 4];
        plan.decode_into(&mut out, &mut [0; 8]).unwrap();
        assert_eq!(out, [42; 4]);
    });
}

#[test]
fn aligned_multi_data_integrity_excludes_only_inter_section_gaps() {
    for indexed in [false, true] {
        with_face(true, indexed, |asset| {
            let bytes = asset.encode().unwrap();
            let view = FontView::open_at(&bytes, 128, &PayloadLimits::EMBEDDED).unwrap();
            view.preflight(&PayloadLimits::EMBEDDED).unwrap();
            asset.preflight(&PayloadLimits::EMBEDDED).unwrap();
            assert_eq!(
                view.input_alignment().map(crate::ByteAlignment::get),
                Ok(64)
            );
            for data in view.media().sections_of_kind(MediaSectionKind::DATA) {
                assert_eq!(data.descriptor().offset() % 64, 0);
            }
            assert!(FontView::open_at(&bytes, 129, &PayloadLimits::EMBEDDED).is_err());
            let integrity = view
                .media()
                .sections_of_kind(MediaSectionKind::INTEGRITY)
                .next();
            assert_eq!(
                integrity.map(|s| s.bytes().len()),
                indexed.then_some(3 * INTEGRITY_RECORD_LEN)
            );
            let first = view
                .media()
                .sections_of_kind(MediaSectionKind::DATA)
                .next()
                .unwrap()
                .descriptor();
            let mut corrupt = bytes.clone();
            corrupt[first.offset() as usize] ^= 1;
            let view = FontView::open(&corrupt, &PayloadLimits::EMBEDDED).unwrap();
            assert!(view.validate_data().is_err());
            assert!(!asset.matches_payload(&corrupt).unwrap());
        });
    }
}

#[test]
fn atlas_map_sharing_and_empty_glyphs_have_explicit_ownership() {
    let cmap = [
        CmapEntry::new(' ', GlyphId::new(0)),
        CmapEntry::new('A', GlyphId::new(1)),
    ];
    let regions = [
        Region::new(0, 0, 0, 0).unwrap(),
        Region::new(1, 0, 1, 1).unwrap(),
    ];
    let alternate_regions = [regions[1], regions[0]];
    let atlas = AtlasMap::new(2, 1, &regions).unwrap();
    let alternate_atlas = AtlasMap::new(2, 1, &alternate_regions).unwrap();
    let map = GlyphMap::atlas(atlas);
    let maps = [atlas, alternate_atlas];
    let raw = RawGlyphs::builder(map, SampleLayout::A4)
        .build(&[0x7f])
        .unwrap();
    let surfaces = [GlyphSurfaceAsset::raw(raw)];
    let representations = [
        RepresentationAsset::new(FontRepresentation::coverage(4, 12, 1).unwrap(), 0)
            .with_atlas_map(0),
        RepresentationAsset::new(FontRepresentation::coverage(4, 16, 1).unwrap(), 0)
            .with_atlas_map(0),
        RepresentationAsset::new(
            FontRepresentation::signed_distance(4, 3, 24, 17, 48, 1).unwrap(),
            0,
        )
        .with_atlas_map(1),
    ];
    let face = FontFace::new(
        1_000,
        GlyphId::NOTDEF,
        2,
        Fixed::ONE,
        Fixed::ZERO,
        Fixed::ZERO,
    )
    .unwrap();
    let advances = [Fixed::ONE; 2];
    let raster_metrics = [RasterMetrics::default(); 6];
    let asset = FontAsset::new(face, &cmap, FontAdvanceSource::Advances(&advances))
        .with_rasters(&representations, &raster_metrics, &surfaces)
        .with_atlas_maps(&maps);
    let bytes = asset.encode().unwrap();
    let view = FontView::open(&bytes, &PayloadLimits::EMBEDDED).unwrap();
    view.preflight(&PayloadLimits::EMBEDDED).unwrap();
    assert_eq!(
        view.media()
            .sections_of_kind(MediaSectionKind::ATLAS_MAPS)
            .next()
            .unwrap()
            .bytes()
            .len(),
        64
    );
    for index in 0..3 {
        assert_eq!(
            view.representation(index)
                .unwrap()
                .record()
                .atlas_map_offset(),
            u32::from(index == 2) * 2
        );
        assert_eq!(
            view.representation(index)
                .unwrap()
                .record()
                .atlas_map_count(),
            2
        );
        let FontGlyphs::Raw(glyphs) = view.glyphs(index).unwrap() else {
            panic!("RAW");
        };
        let plan = glyphs
            .get(usize::from(index != 2))
            .unwrap()
            .memory_plan(SurfaceRequirements::new())
            .unwrap();
        let mut out = [0];
        glyphs
            .get(usize::from(index != 2))
            .unwrap()
            .copy_into(&mut out, plan)
            .unwrap();
        assert_eq!(out, [0xf0]);
    }
    let owned = super::super::Font::decode(&bytes).unwrap();
    assert_eq!(owned.encode().unwrap(), bytes);
    let extras = [atlas, alternate_atlas, atlas];
    assert_eq!(
        asset.with_atlas_maps(&extras).encoded_len(),
        Err(FontError::UnreferencedStorage)
    );
    assert!(matches!(
        FontAsset {
            atlas_maps: &[],
            ..asset
        }
        .encoded_len(),
        Err(FontError::AtlasMapOutOfBounds { .. })
    ));
}

#[test]
fn invalid_assets_and_capacity_fail_before_output_writes() {
    with_face(false, false, |asset| {
        let mut output = vec![0xab; asset.encoded_len().unwrap()];
        let before = output.clone();
        let short = output.len() - 1;
        assert!(asset.encode_into(&mut output[..short]).is_err());
        assert_eq!(output, before);
        let unsorted = [
            CmapEntry::new('B', GlyphId::new(0)),
            CmapEntry::new('A', GlyphId::new(1)),
        ];
        assert!(
            FontAsset {
                cmap: &unsorted,
                ..asset
            }
            .encode_into(&mut output)
            .is_err()
        );
        assert_eq!(output, before);
        let shaping = super::super::shaping::test_sfnt();
        assert!(matches!(
            FontAsset {
                advance_source: FontAdvanceSource::Shaping(&shaping),
                ..asset
            }
            .encode_into(&mut output),
            Err(FontError::Shaping(super::super::ShapingDataError::CmapMismatch {
                scalar,
                shaping: Some(shaping),
                index: Some(index),
            })) if scalar == 'A' as u32 && shaping == GlyphId::new(1) && index == GlyphId::NOTDEF
        ));
        assert_eq!(output, before);
        let mut representations = asset.representations.to_vec();
        representations[0].surface = u16::MAX;
        assert!(
            FontAsset {
                representations: &representations,
                ..asset
            }
            .encode_into(&mut output)
            .is_err()
        );
        assert_eq!(output, before);
        assert!(
            asset
                .preflight(&PayloadLimits::EMBEDDED.with_max_raster_work(0))
                .is_err()
        );
        assert!(
            asset
                .preflight(&PayloadLimits::EMBEDDED.with_max_font_glyphs(1))
                .is_err()
        );
        assert!(
            asset
                .preflight(&PayloadLimits::EMBEDDED.with_max_font_representations(2))
                .is_err()
        );
        let encoded = asset.encode().unwrap();
        MediaPayload::open(&encoded)
            .unwrap()
            .validate_data()
            .unwrap();
    });
}
