use super::*;
use mirx::{
    coding::Rle,
    font::{
        CmapEntry, FontAdvanceSource, FontAsset, FontFace, GlyphId, GlyphMap, GlyphSurfaceAsset,
        RasterMetrics, RawGlyphs, RepresentationAsset,
    },
    image::{
        AtlasMap, ColorDescription, EncodedImageAsset, PlaneMemoryLayout, Region, SampleLayout,
        SurfaceDescriptor,
    },
    types::Fixed,
};

fn atlas_face() -> MirxFontProvider {
    let cmap = [
        CmapEntry::new(' ', GlyphId::new(0)),
        CmapEntry::new('A', GlyphId::new(1)),
    ];
    let regions = [
        Region::new(0, 0, 0, 0).unwrap(),
        Region::new(3, 1, 3, 2).unwrap(),
    ];
    let atlas = AtlasMap::new(8, 4, &regions).unwrap();
    let map = GlyphMap::atlas(atlas);
    let surface = SurfaceDescriptor::new(8, 4, SampleLayout::A1, ColorDescription::NONE).unwrap();
    let memory = PlaneMemoryLayout::builder(surface.plane(0).unwrap())
        .with_stride(64)
        .with_alignment(mirx::types::ByteAlignment::new(64).unwrap())
        .build()
        .unwrap();
    let samples = [0xa5; 256];
    let glyphs = RawGlyphs::builder(map, SampleLayout::A1)
        .with_memory_layout(memory)
        .build(&samples)
        .unwrap();
    let advances = [Fixed::from_int(250), Fixed::from_ratio(1_375, 4)];
    let face = FontFace::new(
        1_000,
        GlyphId::NOTDEF,
        2,
        Fixed::from_int(750),
        Fixed::from_int(-250),
        Fixed::ZERO,
    )
    .unwrap();
    let representation = RepresentationAsset::new(
        mirx::font::FontRepresentation::coverage(1, 16, 4).unwrap(),
        0,
    )
    .with_atlas_map(0);
    let raster_metrics = [
        RasterMetrics::new(Fixed::ZERO, Fixed::from_int(12)),
        RasterMetrics::new(Fixed::from_ratio(-1, 2), Fixed::from_ratio(45, 4)),
    ];
    let payload = FontAsset::new(face, &cmap, FontAdvanceSource::Advances(&advances))
        .with_rasters(
            &[representation],
            &raster_metrics,
            &[GlyphSurfaceAsset::raw(glyphs)],
        )
        .with_atlas_maps(&[atlas])
        .encode()
        .unwrap();
    MirxFontProvider::from_payload(alloc::vec::Vec::leak(payload), &PayloadLimits::HOST).unwrap()
}

#[test]
fn atlas_regions_and_fractional_metrics_reach_the_renderer_unchanged() {
    let provider = atlas_face();
    let glyph_id = provider.map_char('A').unwrap();
    let glyph = provider.raster(glyph_id, 16, 16).unwrap();
    assert_eq!(
        provider.glyph_advance(glyph_id, 16),
        Some(crate::types::Fixed::from_ratio(11, 2))
    );
    assert_eq!(glyph.surface.stride(), 64);
    let region = glyph.region.unwrap();
    assert_eq!(
        (region.x(), region.y(), region.width(), region.height()),
        (3, 1, 3, 2)
    );
    assert_eq!(glyph.offset_x, crate::types::Fixed::from_ratio(-1, 2));
    assert_eq!(glyph.offset_y, crate::types::Fixed::from_ratio(45, 4));
    let space = provider.map_char(' ').unwrap();
    assert!(provider.raster(space, 16, 16).unwrap().region.is_none());
    assert!(provider.map_char('Z').is_none());
}

#[test]
fn face_metrics_and_glyph_advances_scale_with_requested_size() {
    let provider = atlas_face();
    let metrics = provider.metrics(8);
    assert_eq!(metrics.ascender, crate::types::Fixed::from_int(6));
    assert_eq!(metrics.descender, crate::types::Fixed::from_int(-2));
    assert_eq!(metrics.line_height, crate::types::Fixed::from_int(8));
    assert_eq!(
        provider.glyph_advance(provider.map_char('A').unwrap(), 8),
        Some(crate::types::Fixed::from_ratio(11, 4))
    );
}

#[test]
fn encoded_surfaces_require_an_explicit_decode_workspace_provider() {
    let cmap = [
        CmapEntry::new('A', GlyphId::new(0)),
        CmapEntry::new('B', GlyphId::new(1)),
    ];
    let advances = [Fixed::ONE; 2];
    let face = FontFace::new(1, GlyphId::NOTDEF, 2, Fixed::ONE, Fixed::ZERO, Fixed::ZERO).unwrap();
    let map = GlyphMap::cells(2, 2, cmap.len()).unwrap();
    let surface = SurfaceDescriptor::new(2, 4, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let image = EncodedImageAsset::new(surface, Rle::new().record(), &[0x87, 42]);
    let representation = RepresentationAsset::new(
        mirx::font::FontRepresentation::coverage(8, 1, 8).unwrap(),
        0,
    );
    let raster_metrics = [RasterMetrics::default(); 2];
    let payload = FontAsset::new(face, &cmap, FontAdvanceSource::Advances(&advances))
        .with_rasters(
            &[representation],
            &raster_metrics,
            &[GlyphSurfaceAsset::Encoded { map, image }],
        )
        .encode()
        .unwrap();
    assert!(matches!(
        MirxFontProvider::from_payload(alloc::vec::Vec::leak(payload), &PayloadLimits::HOST),
        Err(MirxFontError::EncodedStorage)
    ));
}

fn encoded_face_payload() -> &'static [u8] {
    let cmap = [
        CmapEntry::new('A', GlyphId::new(0)),
        CmapEntry::new('B', GlyphId::new(1)),
    ];
    let advances = [Fixed::ONE; 2];
    let face = FontFace::new(1, GlyphId::NOTDEF, 2, Fixed::ONE, Fixed::ZERO, Fixed::ZERO).unwrap();
    let map = GlyphMap::cells(2, 2, cmap.len()).unwrap();
    let surface = SurfaceDescriptor::new(2, 4, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let image = EncodedImageAsset::new(surface, Rle::new().record(), &[0x87, 42]);
    let representation = RepresentationAsset::new(
        mirx::font::FontRepresentation::coverage(8, 1, 8).unwrap(),
        0,
    );
    let raster_metrics = [RasterMetrics::default(); 2];
    alloc::vec::Vec::leak(
        FontAsset::new(face, &cmap, FontAdvanceSource::Advances(&advances))
            .with_rasters(
                &[representation],
                &raster_metrics,
                &[GlyphSurfaceAsset::Encoded { map, image }],
            )
            .encode()
            .unwrap(),
    )
}

#[test]
fn encoded_surfaces_reside_in_aligned_caller_storage() {
    #[repr(align(64))]
    struct Aligned([u8; 320]);

    let payload = encoded_face_payload();
    let surfaces = alloc::boxed::Box::leak(alloc::boxed::Box::new([None]));
    let output = &mut alloc::boxed::Box::leak(alloc::boxed::Box::new(Aligned([0xa5; 320]))).0[1..];
    let mut groups = [None];
    let mut workspace = [0x5a; 8];
    let storage = MirxFontStorage::new(surfaces, output, &mut groups, &mut workspace)
        .with_requirements(
            SurfaceRequirements::new()
                .with_base_alignment(mirx::types::ByteAlignment::new(64).unwrap())
                .with_stride_multiple(64),
        );
    let provider =
        MirxFontProvider::from_payload_with_storage(payload, &PayloadLimits::HOST, storage)
            .unwrap();

    for (ch, y) in [('A', 0), ('B', 2)] {
        let glyph = provider
            .raster(provider.map_char(ch).unwrap(), 8, 8)
            .unwrap();
        let samples = glyph.surface.samples();
        let stride = glyph.surface.stride();
        let region = glyph.region.unwrap();
        assert_eq!(samples.as_ptr() as usize % 64, 0);
        assert_eq!(samples.len(), 256);
        assert_eq!(stride, 64);
        assert_eq!(region, Region::new(0, y, 2, 2).unwrap());
        for row in 0..4 {
            assert_eq!(&samples[row * 64..row * 64 + 2], &[42; 2]);
            assert!(
                samples[row * 64 + 2..row * 64 + 64]
                    .iter()
                    .all(|byte| *byte == 0)
            );
        }
    }

    groups.fill(None);
    workspace.fill(0);
}

#[test]
fn encoded_surface_admission_precedes_persistent_writes() {
    #[repr(align(64))]
    struct Short([u8; 255]);

    let payload = encoded_face_payload();
    let surfaces = alloc::boxed::Box::leak(alloc::boxed::Box::new([None]));
    let surfaces_ptr = surfaces.as_ptr();
    let output = &mut alloc::boxed::Box::leak(alloc::boxed::Box::new(Short([0xa5; 255]))).0;
    let output_ptr = output.as_ptr();
    let mut groups = [None];
    let mut workspace = [0x5a; 8];
    let storage = MirxFontStorage::new(surfaces, output, &mut groups, &mut workspace)
        .with_requirements(
            SurfaceRequirements::new()
                .with_base_alignment(mirx::types::ByteAlignment::new(64).unwrap())
                .with_stride_multiple(64),
        );
    assert!(matches!(
        MirxFontProvider::from_payload_with_storage(payload, &PayloadLimits::HOST, storage),
        Err(MirxFontError::SurfaceOutputTooSmall {
            surface: 0,
            needed: 256,
            available: 255,
        })
    ));

    // SAFETY: construction failed, so no provider retains the static
    // buffers; the leaked allocations remain live and uniquely owned here.
    let output = unsafe { core::slice::from_raw_parts(output_ptr, 255) };
    let surfaces = unsafe { core::slice::from_raw_parts(surfaces_ptr, 1) };
    assert!(output.iter().all(|byte| *byte == 0xa5));
    assert_eq!(surfaces, &[None]);
}
