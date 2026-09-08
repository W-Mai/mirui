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
    let representations = [
        RepresentationAsset::new(FontRepresentation::coverage(8, 12, 8).unwrap(), 0),
        RepresentationAsset::new(
            FontRepresentation::signed_distance(8, 3, 24, 17, 48, 8).unwrap(),
            1,
        ),
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
    let cmap = [
        CmapEntry::new('A', GlyphId::new(0)),
        CmapEntry::new('B', GlyphId::new(1)),
    ];
    let advances = [Fixed::ONE; 2];
    let raster_metrics = [RasterMetrics::default(); 4];
    check(
        FontAsset::new(face, &cmap, FontAdvanceSource::Advances(&advances)).with_rasters(
            &representations,
            &raster_metrics,
            &surfaces,
        ),
    );
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
    let metric = RasterMetrics::new(Fixed::from_ratio(-17, 256), Fixed::from_ratio(-65, 256));
    font.raster_metrics_mut()[2] = metric;
    font.advances_mut().unwrap()[0] = Fixed::from_ratio(513, 256);
    font.set_cmap_entry(1, CmapEntry::new('中', GlyphId::new(1)))
        .unwrap();
    assert!(
        font.set_cmap_entry(0, CmapEntry::new('中', GlyphId::new(0)))
            .is_err()
    );
    assert!(
        font.set_cmap_entry(usize::MAX, CmapEntry::new('x', GlyphId::new(0)))
            .is_err()
    );
    assert_eq!(
        font.cmap()
            .iter()
            .map(|entry| entry.scalar())
            .collect::<Vec<_>>(),
        ['A', '中']
    );
    assert_eq!(font.surface(1).unwrap().data(), stored);
    font.preflight(&PayloadLimits::EMBEDDED).unwrap();
    let bytes = font.encode().unwrap();
    let view = FontView::open_at(&bytes, 0, &PayloadLimits::EMBEDDED).unwrap();
    view.preflight(&PayloadLimits::EMBEDDED).unwrap();
    let representation = view.representation(1).unwrap();
    assert_eq!(representation.raster_metrics(GlyphId::new(0)), Some(metric));
    assert_eq!(
        representation.advance(GlyphId::new(0)),
        Some(Fixed::from_ratio(513, 256))
    );
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
    let cmap = [
        CmapEntry::new('A', GlyphId::new(0)),
        CmapEntry::new('B', GlyphId::new(1)),
    ];
    let map = GlyphMap::glyph_major(2, 2, 2).unwrap();
    let raw = RawGlyphs::builder(map, SampleLayout::A8)
        .build(&[7; 8])
        .unwrap();
    let representations = [RepresentationAsset::new(
        FontRepresentation::coverage(8, 12, 8).unwrap(),
        0,
    )];
    let surfaces = [GlyphSurfaceAsset::raw(raw).with_integrity(DataIntegrity::Indexed(&[4, 8]))];
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
    let raster_metrics = [RasterMetrics::default(); 2];
    let asset = FontAsset::new(face, &cmap, FontAdvanceSource::Advances(&advances)).with_rasters(
        &representations,
        &raster_metrics,
        &surfaces,
    );
    let original = asset.encode().unwrap();
    let font = Font::decode_with_limits(&original, &PayloadLimits::EMBEDDED).unwrap();
    assert_eq!(font.encode().unwrap(), original);
    assert_eq!(
        font.surface(0).unwrap().integrity(),
        DataIntegrity::Indexed(&[4, 8])
    );
    let bad_surfaces = [GlyphSurfaceAsset::raw(raw).with_integrity(DataIntegrity::Indexed(&[3]))];
    let bad = FontAsset::new(face, &cmap, FontAdvanceSource::Advances(&advances)).with_rasters(
        &representations,
        &raster_metrics,
        &bad_surfaces,
    );
    let mut output = [0xcc; 512];
    assert!(matches!(
        bad.encode_into(&mut output),
        Err(FontError::Integrity(_))
    ));
    assert_eq!(output, [0xcc; 512]);
}
