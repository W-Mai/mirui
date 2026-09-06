use super::*;
use crate::{
    Fixed,
    coding::Rle,
    font::{FontGlyphs, FontRepresentation, FontView},
};

fn asset(check: impl FnOnce(FontAsset<'_>)) {
    let map = GlyphMap::glyph_major(2, 2, 2).unwrap();
    let raw = RawGlyphs::builder(map, SampleLayout::A8)
        .build(&[7; 8])
        .unwrap();
    let image = EncodedImageAsset::new(
        SurfaceDescriptor::new(2, 4, SampleLayout::A8, ColorDescription::NONE).unwrap(),
        Rle::new().record(),
        &[0x87, 42],
    )
    .with_input_alignment(crate::ByteAlignment::new(64).unwrap())
    .with_integrity(DataIntegrity::Indexed(&[1, 2]));
    let surfaces = [
        GlyphSurfaceAsset::raw(raw),
        GlyphSurfaceAsset::Encoded { map, image },
    ];
    let metrics = [GlyphMetrics::default(); 2];
    let line = LineMetrics::new(Fixed::ONE, Fixed::ZERO, Fixed::ONE).unwrap();
    let representations = [
        RepresentationAsset::new(
            FontRepresentation::coverage(8, 12, 8).unwrap(),
            0,
            line,
            &metrics,
        ),
        RepresentationAsset::new(
            FontRepresentation::signed_distance(8, 3, 24, 17, 48, 8).unwrap(),
            1,
            line,
            &metrics,
        ),
    ];
    check(FontAsset::new(&['A', 'B'], &representations, &surfaces));
}

#[test]
fn owned_metadata_edits_preserve_encoded_data_and_shared_ordinals() {
    let mut result = None;
    asset(|asset| {
        let owned = Font::from_asset(asset, &PayloadLimits::EMBEDDED).unwrap();
        assert_eq!(owned.encode().unwrap(), asset.encode().unwrap());
        result = Some(owned);
    });
    let mut font = result.unwrap();
    let source = font.encode().unwrap();
    assert!(font.matches_payload(&source).unwrap());
    let stored = font.surface(1).unwrap().data().to_vec();
    assert_eq!(stored, [0x87, 42]);
    let metric = GlyphMetrics::new(
        Fixed::from_ratio(-17, 256),
        Fixed::from_ratio(-65, 256),
        Fixed::from_ratio(513, 256),
    );
    font.glyph_metrics_mut(1).unwrap()[0] = metric;
    let line =
        LineMetrics::new(Fixed::from_int(4), Fixed::from_int(-1), Fixed::from_int(5)).unwrap();
    font.set_line_metrics(1, line).unwrap();
    font.set_codepoint(1, '中').unwrap();
    assert!(font.set_codepoint(0, '中').is_err());
    assert!(font.set_codepoint(usize::MAX, 'x').is_err());
    assert!(font.set_line_metrics(usize::MAX, line).is_err());
    assert!(font.glyph_metrics_mut(usize::MAX).is_none());
    assert_eq!(font.codepoints(), ['A', '中']);
    assert_eq!(font.surface(1).unwrap().data(), stored);
    font.preflight(&PayloadLimits::EMBEDDED).unwrap();
    let bytes = font.encode().unwrap();
    let view = FontView::open_at(&bytes, 0, &PayloadLimits::EMBEDDED).unwrap();
    view.preflight(&PayloadLimits::EMBEDDED).unwrap();
    assert_eq!(view.tables().get(1).unwrap().metrics().get(0), Some(metric));
    assert_eq!(view.tables().get(1).unwrap().metrics().line_metrics(), line);
    assert!(matches!(view.glyphs(1), Some(FontGlyphs::Encoded(_))));
    let mut output = alloc::vec![0xcc; font.encoded_len().unwrap() + 8];
    let len = font.encode_into(&mut output).unwrap();
    assert_eq!(&output[..len], bytes);
    assert_eq!(&output[len..], &[0xcc; 8]);
}

#[test]
fn owning_limits_cover_metadata_and_encoded_storage_before_allocation() {
    asset(|asset| {
        assert!(matches!(
            Font::from_asset(asset, &PayloadLimits::EMBEDDED.with_max_decoded_bytes(8)),
            Err(FontError::OwnedBytesLimitExceeded { .. })
        ));
        let needed = (8..4096)
            .find(|&limit| {
                Font::from_asset(
                    asset,
                    &PayloadLimits::EMBEDDED.with_max_decoded_bytes(limit),
                )
                .is_ok()
            })
            .unwrap();
        assert!(
            Font::from_asset(
                asset,
                &PayloadLimits::EMBEDDED.with_max_decoded_bytes(needed - 1)
            )
            .is_err()
        );
        let font = Font::from_asset(
            asset,
            &PayloadLimits::EMBEDDED.with_max_decoded_bytes(needed),
        )
        .unwrap();
        assert_eq!(font.representation_count(), 2);
        assert_eq!(font.surface_count(), 2);
        assert!(font.surface(usize::MAX).is_none());
        assert!(font.representation(usize::MAX).is_none());
    });
}

#[test]
fn wire_owned_round_trip_preserves_raw_and_encoded_integrity_partitions() {
    asset(|asset| {
        let original = asset.encode().unwrap();
        let font = Font::decode_with_limits(&original, &PayloadLimits::EMBEDDED).unwrap();
        assert_eq!(font.encode().unwrap(), original);
        assert!(font.matches_payload(&original).unwrap());
        assert_eq!(
            font.surface(0).unwrap().integrity(),
            DataIntegrity::Indexed(&[8])
        );
        assert_eq!(
            font.surface(1).unwrap().integrity(),
            DataIntegrity::Indexed(&[1, 2])
        );
        for limits in [
            PayloadLimits::EMBEDDED.with_max_decoded_bytes(8),
            PayloadLimits::EMBEDDED.with_max_raster_work(0),
            PayloadLimits::EMBEDDED.with_max_font_glyphs(1),
        ] {
            assert!(Font::decode_with_limits(&original, &limits).is_err());
        }
    });
    let chars = ['A', 'B'];
    let map = GlyphMap::glyph_major(2, 2, 2).unwrap();
    let raw = RawGlyphs::builder(map, SampleLayout::A8)
        .build(&[7; 8])
        .unwrap();
    let metrics = [GlyphMetrics::default(); 2];
    let line = LineMetrics::new(Fixed::ONE, Fixed::ZERO, Fixed::ONE).unwrap();
    let representations = [RepresentationAsset::new(
        FontRepresentation::coverage(8, 12, 8).unwrap(),
        0,
        line,
        &metrics,
    )];
    let surfaces = [GlyphSurfaceAsset::raw(raw).with_integrity(DataIntegrity::Indexed(&[4, 8]))];
    let asset = FontAsset::new(&chars, &representations, &surfaces);
    let original = asset.encode().unwrap();
    let font = Font::decode_with_limits(&original, &PayloadLimits::EMBEDDED).unwrap();
    assert_eq!(font.encode().unwrap(), original);
    assert_eq!(
        font.surface(0).unwrap().integrity(),
        DataIntegrity::Indexed(&[4, 8])
    );
    let bad_surfaces = [GlyphSurfaceAsset::raw(raw).with_integrity(DataIntegrity::Indexed(&[3]))];
    let bad = FontAsset::new(&chars, &representations, &bad_surfaces);
    let mut output = [0xcc; 512];
    assert!(matches!(
        bad.encode_into(&mut output),
        Err(FontError::Integrity(_))
    ));
    assert_eq!(output, [0xcc; 512]);
}
