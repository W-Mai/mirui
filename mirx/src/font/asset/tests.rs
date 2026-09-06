use super::*;
use crate::{
    Fixed,
    coding::Rle,
    font::{FontGlyphs, FontRepresentationRequest, FontView},
    image::{Region, SampleLayout, SurfaceRequirements},
    media::MediaPayload,
};
use alloc::vec;

fn with_face(aligned: bool, indexed: bool, check: impl FnOnce(FontAsset<'_>)) {
    let chars = ['A', 'B'];
    let map = GlyphMap::glyph_major(2, 2, chars.len()).unwrap();
    let metrics = [GlyphMetrics::new(
        Fixed::from_ratio(5, 2),
        Fixed::from_ratio(-1, 2),
        Fixed::from_int(2),
    ); 2];
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
        RepresentationAsset::new(
            FontRepresentation::coverage(8, 12, 8).unwrap(),
            0,
            LineMetrics::new(Fixed::from_int(9), Fixed::from_int(-3), Fixed::from_int(12)).unwrap(),
            &metrics,
        ),
        RepresentationAsset::new(
            FontRepresentation::coverage(8, 16, 8).unwrap(),
            0,
            LineMetrics::new(
                Fixed::from_int(12),
                Fixed::from_int(-4),
                Fixed::from_int(16),
            )
            .unwrap(),
            &metrics,
        ),
        RepresentationAsset::new(
            FontRepresentation::signed_distance(8, 3, 24, 17, 48, 8).unwrap(),
            1,
            LineMetrics::new(
                Fixed::from_int(18),
                Fixed::from_int(-6),
                Fixed::from_int(24),
            )
            .unwrap(),
            &metrics,
        ),
    ];
    check(FontAsset::new(&chars, &representations, &surfaces));
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
        assert_eq!(view.media().header().section_count(), 7);
        assert_eq!(
            view.media()
                .sections_of_kind(MediaSectionKind::DATA)
                .count(),
            2
        );
        assert_eq!(
            view.media()
                .sections_of_kind(MediaSectionKind::GLYPH_MAPS)
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
            view.tables().codepoints().iter().collect::<Vec<_>>(),
            ['A', 'B']
        );
        let selected = view
            .tables()
            .select(FontRepresentationRequest::new(24))
            .unwrap();
        assert_eq!(selected.index(), 2);
        assert_eq!(
            selected.metrics().get(1),
            Some(asset.representations[2].metrics[1])
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
    let chars = [' ', 'A'];
    let regions = [
        Region::new(0, 0, 0, 0).unwrap(),
        Region::new(1, 0, 1, 1).unwrap(),
    ];
    let map = GlyphMap::atlas(2, 1, &regions).unwrap();
    let maps = [map];
    let raw = RawGlyphs::builder(map, SampleLayout::A4)
        .build(&[0x7f])
        .unwrap();
    let surfaces = [GlyphSurfaceAsset::raw(raw)];
    let metrics = [GlyphMetrics::default(); 2];
    let line = LineMetrics::new(Fixed::ONE, Fixed::ZERO, Fixed::ONE).unwrap();
    let representations = [
        RepresentationAsset::new(
            FontRepresentation::coverage(4, 12, 1).unwrap(),
            0,
            line,
            &metrics,
        )
        .with_map(0),
        RepresentationAsset::new(
            FontRepresentation::coverage(4, 16, 1).unwrap(),
            0,
            line,
            &metrics,
        )
        .with_map(0),
    ];
    let asset = FontAsset::new(&chars, &representations, &surfaces).with_maps(&maps);
    let bytes = asset.encode().unwrap();
    let view = FontView::open(&bytes, &PayloadLimits::EMBEDDED).unwrap();
    view.preflight(&PayloadLimits::EMBEDDED).unwrap();
    assert_eq!(
        view.media()
            .sections_of_kind(MediaSectionKind::GLYPH_MAPS)
            .next()
            .unwrap()
            .bytes()
            .len(),
        32
    );
    for index in 0..2 {
        assert_eq!(
            view.tables()
                .get(index)
                .unwrap()
                .record()
                .glyph_map_offset(),
            0
        );
        let FontGlyphs::Raw(glyphs) = view.glyphs(index).unwrap() else {
            panic!("RAW");
        };
        let plan = glyphs
            .get(1)
            .unwrap()
            .memory_plan(SurfaceRequirements::new())
            .unwrap();
        let mut out = [0];
        glyphs.get(1).unwrap().copy_into(&mut out, plan).unwrap();
        assert_eq!(out, [0xf0]);
    }
    let extras = [map, map];
    assert_eq!(
        asset.with_maps(&extras).encoded_len(),
        Err(FontError::UnreferencedStorage)
    );
    assert!(matches!(
        FontAsset { maps: &[], ..asset }.encoded_len(),
        Err(FontError::MapOutOfBounds { .. })
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
        let unsorted = ['B', 'A'];
        assert!(
            FontAsset {
                codepoints: &unsorted,
                ..asset
            }
            .encode_into(&mut output)
            .is_err()
        );
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
