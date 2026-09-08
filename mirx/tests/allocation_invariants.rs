use std::alloc::{GlobalAlloc, Layout, System};
use std::borrow::Cow;
use std::cell::Cell;

mod support;

use mirx::frames::{FrameSequence, FramesEncoder};
use mirx::image::{
    ColorDescription, ColorFormat, ImageAsset, PLANE_RECORD_LEN, PlaneMemoryLayout, RawImageAsset,
    RawImageView, SURFACE_RECORD_LEN, SampleLayout, SurfaceDescriptor, SurfaceRequirements,
};
use mirx::media::{MEDIA_CRC_LEN, MEDIA_HEADER_LEN, MEDIA_SECTION_LEN, MediaPayload};
use mirx::meta::{Meta, MetaEntry};
use mirx::{ChunkFlags, ChunkType, Color, Document, EncodeOptions, Palette, PayloadLimits, Reader};
use support::encode_chunks;

fn wire_fixed(bits: i32) -> mirx::Fixed {
    mirx::Fixed::from_le_bytes(bits.to_le_bytes())
}

struct TrackingAllocator;

#[test]
fn sectioned_frames_authoring_and_inspection_allocate_nothing() {
    use mirx::{
        coding::{FrameDelta, ScalarFrameDelta},
        frames::{
            FrameCandidate, FramePolicy, FrameSelector, FrameSequence, FramesAsset, FramesView,
        },
        image::{CoverageBudget, DecodeRequest, MemoryPlacement, ReferenceMode, UnitGroupRecord},
        media::{CodingId, CodingRecord},
    };
    #[repr(align(64))]
    struct Aligned([u8; 64]);
    let sequence = FrameSequence::new(2, 1_000, 40)
        .unwrap()
        .with_max_delta_frames(1)
        .unwrap();
    let surface = SurfaceDescriptor::new(1, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let mut output = [0xa5; 256];
    let mut canvas = Aligned([0xa5; 64]);
    let mut workspace = Aligned([0; 64]);
    let mut slots = [None];
    let (_, allocations) = count_allocations(|| {
        let mut selector = FrameSelector::new(FramePolicy::new(1));
        selector
            .select(&[FrameCandidate::keyframe(CodingId::RAW, 1)])
            .unwrap();
        assert_eq!(
            selector
                .select(&[
                    FrameCandidate::delta(2),
                    FrameCandidate::keyframe(CodingId::RAW, 3),
                ])
                .unwrap()
                .candidate(),
            FrameCandidate::delta(2)
        );
        let codec = FrameDelta::new();
        let codings = [CodingRecord::RAW, codec.record()];
        let mut data = [7, 0, 0];
        let delta_len = codec.encode_into(&[7], &[9], &mut data[1..]).unwrap();
        let mut direct = [7];
        codec
            .plan(&data[1..1 + delta_len], direct.len())
            .unwrap()
            .apply_with(&mut direct, &mut ScalarFrameDelta)
            .unwrap();
        assert_eq!(direct, [9]);
        let groups = [
            UnitGroupRecord::new(0, 0..1).unwrap(),
            UnitGroupRecord::new(1, 1..1 + delta_len as u32)
                .unwrap()
                .with_reference(ReferenceMode::Previous),
        ];
        let asset = FramesAsset::new(sequence, surface, &codings, &groups, &[1, 1], &data).unwrap();
        let len = asset.encode_into(&mut output).unwrap();
        assert_eq!(asset.encoded_len(), Ok(len));
        let frames = FramesView::open(&output[..len], &PayloadLimits::EMBEDDED).unwrap();
        assert_eq!(frames.frame(1).unwrap().groups(), 1..2);
        let groups = frames
            .groups_into(0, &mut slots, &mut CoverageBudget::new(100))
            .unwrap();
        let request = DecodeRequest::new(
            SurfaceRequirements::new()
                .with_base_alignment(mirx::ByteAlignment::new(64).unwrap())
                .with_stride_multiple(64),
        )
        .with_input(MemoryPlacement::Flash)
        .with_output(MemoryPlacement::SharedCoherent)
        .with_workspace_alignment(mirx::ByteAlignment::new(64).unwrap());
        groups
            .decode_plan_for(request, &PayloadLimits::EMBEDDED)
            .unwrap()
            .decode_into(&mut canvas.0, &mut workspace.0, &mut [])
            .unwrap();
        assert_eq!(canvas.0[0], 7);
        let timeline = frames.timeline();
        assert_eq!(timeline.cycle_duration_ticks(), 80);
        assert_eq!(timeline.locate(40).unwrap().frame(), 1);
        let plan = frames
            .playback_plan_for(request, PayloadLimits::EMBEDDED, &mut slots)
            .unwrap();
        assert!(plan.access_capabilities().supports_random_frame());
        assert_eq!(plan.canvas_requirements().byte_len(), 64);
        assert_eq!(plan.workspace_requirements().byte_len(), 1);
        let mut session = plan
            .bind(mirx::frames::PlaybackStorage {
                groups: &mut slots,
                canvas: &mut canvas.0,
                workspace: &mut workspace.0,
                backup: &mut [],
            })
            .unwrap();
        assert_eq!(session.present(0).unwrap().plane(0).unwrap().bytes()[0], 7);
        assert_eq!(session.present(1).unwrap().plane(0).unwrap().bytes()[0], 9);
        frames.validate_data().unwrap();
    });
    assert_eq!(allocations, 0);
}

#[test]
fn frequency_authoring_preflight_and_aligned_decode_allocate_nothing() {
    use mirx::{
        coding::{Frequency, FrequencyGeometry},
        image::{CoverageBudget, EncodedImageAsset, EncodedImageView},
    };
    #[repr(align(64))]
    struct Aligned([u8; 1024]);
    let samples = core::array::from_fn::<_, 64, _>(|index| (index * 37 + 11) as u8);
    let geometry = FrequencyGeometry::for_plane(SampleLayout::A8, 0, 8, 8).unwrap();
    let codec = Frequency::reversible();
    let mut stream = [0xa5; 384];
    let mut payload = Aligned([0xa5; 1024]);
    let mut output = Aligned([0xa5; 1024]);
    let mut workspace = [0xa5; 64];
    let (_, allocations) = count_allocations(|| {
        let stream_len = codec.encode_into(geometry, &samples, &mut stream).unwrap();
        let mut params = [0];
        let asset = EncodedImageAsset::new(
            SurfaceDescriptor::new(8, 8, SampleLayout::A8, ColorDescription::NONE).unwrap(),
            codec.record_into(&mut params),
            &stream[..stream_len],
        );
        asset.preflight(&PayloadLimits::EMBEDDED).unwrap();
        let payload_len = asset.encode_into(&mut payload.0).unwrap();
        let image = EncodedImageView::open(&payload.0[..payload_len]).unwrap();
        let mut slots = [None];
        let groups = image
            .groups_into(&mut slots, &mut CoverageBudget::new(1024))
            .unwrap();
        let plan = groups
            .decode_plan(
                SurfaceRequirements::new()
                    .with_base_alignment(mirx::ByteAlignment::new(64).unwrap())
                    .with_stride_multiple(64),
                &PayloadLimits::EMBEDDED,
            )
            .unwrap();
        let decoded = plan.decode_into(&mut output.0, &mut workspace).unwrap();
        assert_eq!(
            decoded.plane(0).unwrap().row(7).unwrap(),
            Some(&samples[56..])
        );
    });
    assert_eq!(allocations, 0);
    assert!(output.0[512..].iter().all(|byte| *byte == 0xa5));
}

#[test]
fn owned_font_edits_and_emission_need_no_temporary_reference_arrays() {
    use mirx::{
        Fixed,
        font::{
            CmapEntry, Font, FontAdvanceSource, FontAsset, FontFace, FontRepresentation, GlyphId,
            GlyphMap, GlyphSurfaceAsset, RasterMetrics, RawGlyphs, RepresentationAsset,
        },
    };
    let cmap = [CmapEntry::new('A', GlyphId::NOTDEF)];
    let advances = [wire_fixed(256)];
    let face = FontFace::new(
        1_000,
        GlyphId::NOTDEF,
        1,
        wire_fixed(256),
        Fixed::ZERO,
        Fixed::ZERO,
    )
    .unwrap();
    let raw = RawGlyphs::builder(GlyphMap::cells(2, 2, 1).unwrap(), SampleLayout::A8)
        .build(&[7; 4])
        .unwrap();
    let surfaces = [GlyphSurfaceAsset::raw(raw)];
    let representations = [RepresentationAsset::new(
        FontRepresentation::coverage(8, 12, 4).unwrap(),
        0,
    )];
    let raster_metrics = [RasterMetrics::default()];
    let asset = FontAsset::new(face, &cmap, FontAdvanceSource::Advances(&advances)).with_rasters(
        &representations,
        &raster_metrics,
        &surfaces,
    );
    let mut owned = Font::from_asset(asset, &PayloadLimits::EMBEDDED).unwrap();
    let (_, allocations) = count_allocations(|| {
        owned
            .set_cmap_entry(0, CmapEntry::new('中', GlyphId::NOTDEF))
            .unwrap();
        owned.advances_mut().unwrap()[0] = wire_fixed(517);
        owned.raster_metrics_mut()[0] = RasterMetrics::new(Fixed::ZERO, Fixed::ZERO);
        let mut output = [0; 512];
        let len = owned.encode_into(&mut output).unwrap();
        assert_eq!(owned.encoded_len(), Ok(len));
        assert!(owned.matches_payload(&output[..len]).unwrap());
        owned.preflight(&PayloadLimits::EMBEDDED).unwrap();
    });
    assert_eq!(allocations, 0);
    let bytes = asset.encode().unwrap();
    let (_, allocations) = count_allocations(|| {
        assert!(
            Font::from_asset(asset, &PayloadLimits::EMBEDDED.with_max_decoded_bytes(4)).is_err()
        );
        assert!(
            Font::decode_with_limits(&bytes, &PayloadLimits::EMBEDDED.with_max_decoded_bytes(4))
                .is_err()
        );
    });
    assert_eq!(allocations, 0);
}

#[test]
fn native_font_emission_and_complete_borrowed_access_allocate_nothing() {
    use mirx::{
        Fixed,
        coding::Rle,
        font::{
            CmapEntry, FontAdvanceSource, FontAsset, FontFace, FontGlyphs, FontRepresentation,
            FontRepresentationRequest, FontView, GlyphId, GlyphMap, GlyphSurfaceAsset,
            RasterMetrics, RawGlyphs, RepresentationAsset,
        },
        image::{CoverageBudget, EncodedImageAsset},
    };
    let (_, allocations) = count_allocations(|| {
        let map = GlyphMap::cells(2, 2, 2).unwrap();
        let cmap = [
            CmapEntry::new('A', GlyphId::new(0)),
            CmapEntry::new('B', GlyphId::new(1)),
        ];
        let advances = [wire_fixed(256); 2];
        let raw = RawGlyphs::builder(map, SampleLayout::A8)
            .build(&[7; 8])
            .unwrap();
        let image = EncodedImageAsset::new(
            SurfaceDescriptor::new(2, 4, SampleLayout::A8, ColorDescription::NONE).unwrap(),
            Rle::new().record(),
            &[0x87, 42],
        );
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
            wire_fixed(256),
            Fixed::ZERO,
            Fixed::ZERO,
        )
        .unwrap();
        let raster_metrics = [RasterMetrics::default(); 4];
        let asset = FontAsset::new(face, &cmap, FontAdvanceSource::Advances(&advances))
            .with_rasters(&representations, &raster_metrics, &surfaces);
        asset.preflight(&PayloadLimits::EMBEDDED).unwrap();
        let mut output = [0; 512];
        let len = asset.encode_into(&mut output).unwrap();
        assert_eq!(asset.encoded_len(), Ok(len));
        assert!(asset.matches_payload(&output[..len]).unwrap());
        let font = FontView::open(&output[..len], &PayloadLimits::EMBEDDED).unwrap();
        font.preflight(&PayloadLimits::EMBEDDED).unwrap();
        let chosen = font.select(FontRepresentationRequest::new(24)).unwrap();
        assert_eq!(chosen.index(), 1);
        let FontGlyphs::Encoded(glyphs) = font.glyphs(chosen.index()).unwrap() else {
            panic!("encoded");
        };
        let mut slots = [None];
        let groups = glyphs
            .groups_into(&mut slots, &mut CoverageBudget::new(1000))
            .unwrap();
        let plan = groups
            .decode_plan(1, SurfaceRequirements::new(), &PayloadLimits::EMBEDDED)
            .unwrap();
        let mut decoded = [0; 4];
        plan.decode_into(&mut decoded, &mut [0; 8]).unwrap();
        assert_eq!(decoded, [42; 4]);

        let mut coding_body = [0; 12];
        mirx::media::CodingTable::encode_into(&[Rle::new().record()], &mut coding_body).unwrap();
        let image = EncodedImageAsset::from_codings(
            image.surface(),
            mirx::media::CodingTable::open(&coding_body).unwrap(),
            image.data(),
        );
        let mut image_bytes = [0; 256];
        let image_len = image.encode_into(&mut image_bytes).unwrap();
        assert!(image.matches_payload(&image_bytes[..image_len]).unwrap());
        image.preflight(&PayloadLimits::EMBEDDED).unwrap();
    });
    assert_eq!(allocations, 0);
}

#[test]
fn encoded_glyph_planning_and_execution_allocate_nothing() {
    use mirx::{
        coding::Rle,
        font::{GlyphMap, GlyphPacking, GlyphSurfaceRecord},
        image::{CoverageBudget, EncodedImageAsset, UnitGroupRecord},
    };
    let surface = SurfaceDescriptor::new(2, 4, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let bytes = EncodedImageAsset::from_groups(
        surface,
        &[Rle::new().record()],
        &[UnitGroupRecord::new(0, 0..4).unwrap().with_tiles(2, 2)],
        &[0x83, 1, 0x83, 2],
    )
    .encode()
    .unwrap();
    let (_, allocations) = count_allocations(|| {
        let media = MediaPayload::open(&bytes).unwrap();
        let record = GlyphSurfaceRecord::new(SampleLayout::A8, GlyphPacking::GlyphMajor, 2, 2, 3)
            .unwrap()
            .with_codings(1)
            .unwrap()
            .with_groups(2, None)
            .unwrap();
        let glyphs = record
            .encoded_glyphs(media, GlyphMap::cells(2, 2, 2).unwrap())
            .unwrap();
        glyphs.preflight(&PayloadLimits::EMBEDDED).unwrap();
        let mut slots = [None];
        let groups = glyphs
            .groups_into(&mut slots, &mut CoverageBudget::new(1000))
            .unwrap();
        let plan = groups
            .decode_plan(1, SurfaceRequirements::new(), &PayloadLimits::EMBEDDED)
            .unwrap();
        let mut output = [0; 4];
        let mut workspace = [0; 4];
        plan.decode_into(&mut output, &mut workspace).unwrap();
        assert_eq!(output, [2; 4]);
    });
    assert_eq!(allocations, 0);
}

#[test]
fn borrowed_representation_tables_resolve_and_select_without_allocation() {
    use mirx::{
        FontRepresentation, FontRepresentationRequest,
        font::{
            GlyphPacking, GlyphSurfaceRecord, REPRESENTATION_RECORD_LEN, RepresentationRecord,
            RepresentationTable,
        },
    };
    let (_, allocations) = count_allocations(|| {
        let mut surfaces = [0; 24];
        GlyphSurfaceRecord::new(SampleLayout::A4, GlyphPacking::GlyphMajor, 8, 8, 0)
            .unwrap()
            .encode_record_into(&mut surfaces)
            .unwrap();
        let mut records = [0; REPRESENTATION_RECORD_LEN * 2];
        RepresentationRecord::new(FontRepresentation::coverage(4, 16, 64).unwrap(), 0)
            .encode_record_into(&mut records)
            .unwrap();
        RepresentationRecord::new(
            FontRepresentation::signed_distance(4, 3, 24, 17, 48, 64).unwrap(),
            0,
        )
        .encode_record_into(&mut records[REPRESENTATION_RECORD_LEN..])
        .unwrap();
        let table =
            RepresentationTable::open(&records, &surfaces, 2, &PayloadLimits::EMBEDDED).unwrap();
        assert_eq!(
            table
                .select(FontRepresentationRequest::new(24))
                .unwrap()
                .index(),
            1
        );
        assert_eq!(table.iter().nth_back(1), table.get(0));
        assert_eq!(table.iter().count(), 2);
        assert_eq!(table.get(1).unwrap().representation().decoded_bytes(), 64);
    });
    assert_eq!(allocations, 0);
}

#[test]
fn referenced_raw_glyph_binding_allocates_nothing() {
    use mirx::font::{GlyphMap, GlyphPacking, GlyphSurfaceRecord};
    let surface = SurfaceDescriptor::new(3, 2, SampleLayout::A4, ColorDescription::NONE).unwrap();
    let bytes = RawImageAsset::new(surface, &[&[0x12, 0x30, 0x45, 0x60]])
        .encode()
        .unwrap();
    let (_, allocations) = count_allocations(|| {
        let media = MediaPayload::open(&bytes).unwrap();
        media.validate_data().unwrap();
        let record =
            GlyphSurfaceRecord::new(SampleLayout::A4, GlyphPacking::GlyphMajor, 3, 1, 1).unwrap();
        let glyphs = record
            .raw_glyphs(media, GlyphMap::cells(3, 1, 2).unwrap())
            .unwrap();
        assert_eq!(
            glyphs.get(1).unwrap().storage().plane(0).unwrap().bytes(),
            &[0x45, 0x60]
        );
    });
    assert_eq!(allocations, 0);
}

#[test]
fn glyph_surface_records_and_directory_binding_allocate_nothing() {
    use mirx::font::{GlyphPacking, GlyphSurfaceRecord};
    let bytes = raw_a8_media_payload();
    let (_, allocations) = count_allocations(|| {
        let media = MediaPayload::open(&bytes).unwrap();
        let record =
            GlyphSurfaceRecord::new(SampleLayout::A8, GlyphPacking::GlyphMajor, 1, 1, 1).unwrap();
        record.validate_sections(media).unwrap();
        let encoded = record
            .with_codings(2)
            .unwrap()
            .with_groups(3, Some(4))
            .unwrap();
        assert_eq!(encoded.logical_extent(100).unwrap(), (1, 100));
        let mut out = [0xa5; 25];
        encoded.encode_record_into(&mut out[1..]).unwrap();
        assert_eq!(GlyphSurfaceRecord::from_record(&out[1..]).unwrap(), encoded);
        assert_eq!(out[0], 0xa5);
    });
    assert_eq!(allocations, 0);
}

#[test]
fn representation_record_binding_and_emission_allocate_nothing() {
    use mirx::{FontRepresentation, font::RepresentationRecord};
    let (_, allocations) = count_allocations(|| {
        let surface =
            SurfaceDescriptor::new(5, 3, SampleLayout::A4, ColorDescription::NONE).unwrap();
        let metadata = FontRepresentation::signed_distance(4, 3, 24, 17, 48, 9).unwrap();
        let record = RepresentationRecord::new(metadata, 2).with_atlas_map_range(32, 4);
        record.validate_for(surface).unwrap();
        let mut bytes = [0xa5; 21];
        record.encode_record_into(&mut bytes[1..]).unwrap();
        let decoded = RepresentationRecord::from_record(&bytes[1..], surface).unwrap();
        assert_eq!(decoded, record);
        assert_eq!(bytes[0], 0xa5);
    });
    assert_eq!(allocations, 0);
}

#[test]
fn raw_glyph_cells_and_atlas_regions_borrow_without_allocation() {
    use mirx::{
        font::{GlyphMap, RawGlyphs},
        image::{AtlasMap, Region},
    };
    #[repr(align(64))]
    struct Buffer([u8; 512]);
    let source = Buffer([0x55; 512]);
    let mut output = Buffer([0xa5; 512]);
    let (_, allocations) = count_allocations(|| {
        let cell = SampleLayout::A2.plane_geometry(5, 3, 0).unwrap();
        let memory = PlaneMemoryLayout::builder(cell)
            .with_stride(64)
            .with_alignment(mirx::ByteAlignment::new(64).unwrap())
            .build()
            .unwrap();
        let map = GlyphMap::cells(5, 3, 2).unwrap();
        let glyphs = RawGlyphs::builder(map, SampleLayout::A2)
            .with_memory_layout(memory)
            .build(&source.0[..384])
            .unwrap();
        assert!(glyphs.data_addresses_are_aligned());
        assert!(glyphs.file_address_is_aligned(64));
        let glyph = glyphs.get(1).unwrap();
        assert_eq!(
            glyph.storage().plane(0).unwrap().bytes().as_ptr(),
            source.0[192..].as_ptr()
        );
        let plan = glyph
            .memory_plan(
                SurfaceRequirements::new()
                    .with_base_alignment(mirx::ByteAlignment::new(64).unwrap())
                    .with_stride_multiple(64),
            )
            .unwrap();
        glyph.copy_into(&mut output.0, plan).unwrap();
        let regions = [Region::new(1, 1, 3, 2).unwrap()];
        let map = GlyphMap::atlas(AtlasMap::new(5, 3, &regions).unwrap());
        let glyph = RawGlyphs::builder(map, SampleLayout::A2)
            .with_memory_layout(memory)
            .build(&source.0[..192])
            .unwrap()
            .get(0)
            .unwrap();
        assert_eq!(
            glyph.storage().plane(0).unwrap().bytes().as_ptr(),
            source.0.as_ptr()
        );
        let plan = glyph.memory_plan(SurfaceRequirements::new()).unwrap();
        let view = glyph.copy_into(&mut output.0, plan).unwrap();
        assert_eq!(view.plane(0).unwrap().row(0).unwrap(), Some(&[0x54][..]));
        assert_eq!(view.plane(0).unwrap().row(1).unwrap(), Some(&[0x54][..]));
    });
    assert_eq!(allocations, 0);
}

#[test]
fn indexed_region_decode_borrows_palette_and_uses_only_caller_output_and_workspace() {
    use mirx::{
        coding::Rle,
        image::{
            CoverageBudget, DecodeRequest, EncodedImageAsset, EncodedImageView, MemoryPlacement,
            UnitGroupRecord,
        },
        media::DataIntegrity,
    };
    let surface = SurfaceDescriptor::new(4, 1, SampleLayout::I4, ColorDescription::SRGB).unwrap();
    let coding = [Rle::new().record()];
    let records = [UnitGroupRecord::new(0, 0..4).unwrap().with_tiles(2, 1)];
    let payload =
        EncodedImageAsset::from_groups(surface, &coding, &records, &[0x80, 0x12, 0x80, 0x34])
            .with_color_table(&[0; 64])
            .with_integrity(DataIntegrity::Indexed(&[2, 4]))
            .encode()
            .unwrap();
    #[repr(align(64))]
    struct Buffer([u8; 128]);
    let mut output = Buffer([0xa5; 128]);
    let mut workspace = [0x5a; 4];
    let (_, allocations) = count_allocations(|| {
        let image = EncodedImageView::open(&payload).unwrap();
        let mut slots = [None];
        let groups = image
            .groups_into(&mut slots, &mut CoverageBudget::new(1000))
            .unwrap();
        let plan = groups
            .decode_region_plan_for(
                surface.region(1, 0, 2, 1).unwrap(),
                DecodeRequest::new(
                    SurfaceRequirements::new()
                        .with_base_alignment(mirx::ByteAlignment::new(64).unwrap())
                        .with_stride_multiple(64),
                )
                .with_input(MemoryPlacement::Flash)
                .with_output(MemoryPlacement::SharedCoherent)
                .with_workspace_alignment(mirx::ByteAlignment::new(4).unwrap()),
                &PayloadLimits::EMBEDDED,
            )
            .unwrap();
        assert_eq!(plan.workspace_requirements().byte_len(), 1);
        assert_eq!(plan.checksum_byte_len(), 4);
        let view = plan.decode_into(&mut output.0, &mut workspace).unwrap();
        assert_eq!(
            view.color_table().unwrap().as_bytes().as_ptr(),
            image.color_table().unwrap().as_bytes().as_ptr()
        );
        assert_eq!(view.plane(0).unwrap().row(0).unwrap(), Some(&[0x23][..]));
    });
    assert_eq!(allocations, 0);
    assert_eq!(&output.0[64..], &[0xa5; 64]);
    assert_eq!(&workspace[1..], &[0x5a; 3]);
}

#[test]
fn data_check_planning_and_verification_borrow_partition_metadata() {
    use mirx::{
        coding::Rle,
        image::EncodedImageAsset,
        media::{DataIntegrity, MediaSectionKind},
    };
    let surface = SurfaceDescriptor::new(8, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let bytes = EncodedImageAsset::new(surface, Rle::new().record(), &[0x83, 1, 0x83, 2])
        .with_integrity(DataIntegrity::Indexed(&[2, 4]))
        .encode()
        .unwrap();
    let (_, allocations) = count_allocations(|| {
        let media = MediaPayload::open(&bytes).unwrap();
        let start = media
            .section(MediaSectionKind::DATA)
            .unwrap()
            .descriptor()
            .offset();
        let plan = media.data_check_plan(start..start + 1).unwrap();
        assert_eq!(plan.byte_len(), 2);
        plan.verify().unwrap();
    });
    assert_eq!(allocations, 0);
}

#[test]
fn sparse_region_queries_skip_extreme_empty_spans_without_allocation() {
    use mirx::{
        image::UnitGroup,
        media::{CodingRecord, UnitSelection},
    };
    let surface =
        SurfaceDescriptor::new(1, u32::MAX, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let cells = (u32::MAX - 1).to_le_bytes();
    let selection = UnitSelection::list(u32::MAX, &cells).unwrap();
    let group = UnitGroup::builder(surface, CodingRecord::RAW, &[42])
        .with_tiles(1, 1)
        .with_selection(selection)
        .build()
        .unwrap();
    let (_, allocations) = count_allocations(|| {
        let region = surface.region(0, 0, 1, u32::MAX).unwrap();
        let mut units = group.units_in(region).unwrap();
        assert_eq!(units.work_bound(), 4);
        let unit = units.next().unwrap();
        assert_eq!(unit.cell(), u32::MAX - 1);
        assert_eq!(unit.data(), &[42]);
        assert_eq!(units.next(), None);
    });
    assert_eq!(allocations, 0);
}

#[test]
fn exact_indexed_crops_reuse_caller_storage_and_borrow_the_palette() {
    let surface = SurfaceDescriptor::new(9, 2, SampleLayout::I2, ColorDescription::SRGB).unwrap();
    let palette = [0; 16];
    let source = RawImageAsset::new(surface, &[&[0x1b, 0xe4, 0x80, 0xe4, 0x1b, 0x40]])
        .with_color_table(&palette)
        .view()
        .unwrap();
    #[repr(align(64))]
    struct Buffer([u8; 256]);
    let mut output = Buffer([0xa5; 256]);
    let (_, allocations) = count_allocations(|| {
        let plan = surface
            .region_plan(
                surface.region(3, 0, 5, 2).unwrap(),
                SurfaceRequirements::new()
                    .with_base_alignment(mirx::ByteAlignment::new(64).unwrap())
                    .with_stride_multiple(64),
            )
            .unwrap();
        let view = source.copy_region_into(&mut output.0, plan).unwrap();
        assert_eq!(
            view.color_table().unwrap().as_bytes().as_ptr(),
            palette.as_ptr()
        );
        assert_eq!(view.plane(0).unwrap().row(0).unwrap(), Some(&[0xf9, 0][..]));
        assert_eq!(
            view.plane(0).unwrap().row(1).unwrap(),
            Some(&[0x06, 0xc0][..])
        );
    });
    assert_eq!(allocations, 0);
    assert_eq!(&output.0[128..], &[0xa5; 128]);
}

#[test]
fn prepared_image_decode_uses_only_caller_output_workspace_and_borrowed_palette() {
    use mirx::{
        PayloadLimits,
        coding::Rle,
        image::{
            ColorDescription, CoverageBudget, EncodedImageAsset, EncodedImageView, SampleLayout,
            SurfaceDescriptor, SurfaceRequirements,
        },
    };
    let surface = SurfaceDescriptor::new(3, 2, SampleLayout::I4, ColorDescription::SRGB).unwrap();
    let payload = EncodedImageAsset::new(surface, Rle::new().record(), &[0x83, 0xff])
        .with_color_table(&[0; 64])
        .encode()
        .unwrap();
    #[repr(align(64))]
    struct Buffer([u8; 256]);
    let mut output = Buffer([0xa5; 256]);
    let mut workspace = [0x5a; 8];
    let (_, allocations) = count_allocations(|| {
        let image = EncodedImageView::open(&payload).unwrap();
        let mut slots = [None];
        let groups = image
            .groups_into(&mut slots, &mut CoverageBudget::new(1000))
            .unwrap();
        let plan = groups
            .decode_plan(
                SurfaceRequirements::new()
                    .with_base_alignment(mirx::ByteAlignment::new(64).unwrap())
                    .with_stride_multiple(64),
                &PayloadLimits::EMBEDDED,
            )
            .unwrap();
        assert_eq!(plan.workspace_requirements().byte_len(), 4);
        let view = plan.decode_into(&mut output.0, &mut workspace).unwrap();
        assert_eq!(
            view.color_table().unwrap().as_bytes().as_ptr(),
            image.color_table().unwrap().as_bytes().as_ptr()
        );
        assert_eq!(
            view.plane(0).unwrap().row(0).unwrap(),
            Some(&[0xff, 0xf0][..])
        );
        assert_eq!(
            view.plane(0).unwrap().row(1).unwrap(),
            Some(&[0xff, 0xf0][..])
        );
    });
    assert_eq!(allocations, 0);
    assert_eq!(&output.0[128..], &[0xa5; 128]);
    assert_eq!(&workspace[4..], &[0x5a; 4]);
}

#[test]
fn decoded_units_place_samples_in_shared_surface_storage_without_allocation() {
    use mirx::{
        image::{
            ColorDescription, SampleLayout, SurfaceDescriptor, SurfaceRequirements, UnitGroup,
        },
        media::{CodingRecord, UnitSelection},
    };
    let surface = SurfaceDescriptor::new(9, 1, SampleLayout::A1, ColorDescription::NONE).unwrap();
    let selection_bytes = 1u32.to_le_bytes();
    let selection = UnitSelection::list(3, &selection_bytes).unwrap();
    let unit = UnitGroup::builder(surface, CodingRecord::RAW, &[0b1010_0000])
        .with_tiles(3, 1)
        .with_selection(selection)
        .build()
        .unwrap()
        .get(0)
        .unwrap();
    let target = surface
        .memory_plan(SurfaceRequirements::new().with_stride_multiple(8))
        .unwrap();
    let mut unit_buffer = [0; 1];
    let mut output = [0x5a; 16];
    let (_, allocations) = count_allocations(|| {
        let decoded = unit
            .decode_plan(SurfaceRequirements::new())
            .unwrap()
            .decode_into(&mut unit_buffer)
            .unwrap();
        decoded.copy_into(&mut output, target).unwrap();
    });
    assert_eq!(allocations, 0);
    assert_eq!(output[0], 0b0101_0110);
    assert_eq!(&output[1..], &[0x5a; 15]);
}

#[test]
fn native_wire_atlas_and_implicit_glyph_maps_allocate_nothing() {
    use mirx::{
        font::GlyphMap,
        image::{AtlasMap, Region},
    };
    let regions = [
        Region::new(1, 2, 3, 4).unwrap(),
        Region::new(0, 0, 0, 0).unwrap(),
    ];
    let mut bytes = [0; 32];
    let (region, allocations) = count_allocations(|| {
        let native = AtlasMap::new(7, 9, &regions).unwrap();
        native.encode_into(&mut bytes).unwrap();
        let wire = AtlasMap::open(7, 9, &bytes).unwrap();
        assert_eq!(wire.iter().count(), 2);
        assert_eq!(wire.get(1), native.get(1));
        let implicit = GlyphMap::cells(7, 9, 10).unwrap();
        implicit.get(9).unwrap()
    });
    assert_eq!(allocations, 0);
    assert_eq!(region, Region::new(0, 81, 7, 9).unwrap());
}

#[test]
fn font_selection_retains_only_inline_metadata_without_source_allocation() {
    use mirx::{FontRepresentation, FontRepresentationRequest, FontRepresentations};
    let (selected, allocations) = count_allocations(|| {
        let records = [
            FontRepresentation::coverage(4, 16, 512).unwrap(),
            FontRepresentation::signed_distance(8, 4, 24, 17, 48, 1024).unwrap(),
        ];
        FontRepresentations::new(&records)
            .unwrap()
            .select(FontRepresentationRequest::new(32))
            .unwrap()
    });
    assert_eq!(allocations, 0);
    assert_eq!(selected.representation().design_ppem(), 24);
    assert_eq!(selected.index(), 1);
}

#[test]
fn raw_units_preflight_and_transfer_without_staging_or_heap() {
    use mirx::{
        image::{EncodedImageAsset, EncodedImageView, UnitGroup},
        media::CodingRecord,
    };
    let surface = SurfaceDescriptor::new(
        5,
        3,
        SampleLayout::NV12,
        ColorDescription::BT709_YUV_LIMITED,
    )
    .unwrap();
    let samples = [128; 27];
    let records = [mirx::image::UnitGroupRecord::new(0, 0..27).unwrap()];
    let codings = [CodingRecord::RAW];
    let asset = EncodedImageAsset::from_groups(surface, &codings, &records, &samples);
    #[repr(align(64))]
    struct Aligned([u8; 512]);
    let mut output = Aligned([0xad; 512]);
    let mut payload = [0; 256];
    let (_, allocations) = count_allocations(|| {
        asset.preflight(&PayloadLimits::EMBEDDED).unwrap();
        let len = asset.encode_into(&mut payload).unwrap();
        EncodedImageView::open(&payload[..len])
            .unwrap()
            .preflight(&PayloadLimits::EMBEDDED)
            .unwrap();
        let unit = UnitGroup::builder(surface, CodingRecord::RAW, &samples)
            .build()
            .unwrap()
            .get(0)
            .unwrap();
        let plan = unit
            .decode_plan(
                SurfaceRequirements::new()
                    .with_base_alignment(mirx::ByteAlignment::new(64).unwrap())
                    .with_plane_alignment(mirx::ByteAlignment::new(64).unwrap())
                    .with_stride_multiple(64),
            )
            .unwrap();
        let decoded = plan.decode_into(&mut output.0).unwrap();
        assert_eq!(
            decoded.plane(0).unwrap().row(0).unwrap(),
            Some(&[128; 5][..])
        );
        assert_eq!(
            decoded.plane(1).unwrap().row(0).unwrap(),
            Some(&[128; 6][..])
        );
    });
    assert_eq!(allocations, 0);
}

#[test]
fn grouped_authoring_uses_caller_tables_and_allocates_only_changed_payloads() {
    use mirx::{
        coding::Rle,
        image::{EncodedImageAsset, EncodedImageView, UnitGroupRecord},
        media::{DataIntegrity, UnitIndexEncoding},
    };
    let surface = SurfaceDescriptor::new(3, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let codings = [Rle::new().record()];
    let records = [UnitGroupRecord::new(0, 0..5)
        .unwrap()
        .with_tiles(2, 1)
        .with_index_encoding(UnitIndexEncoding::Lengths16)];
    let mut index = [0; 16];
    let index_len = UnitIndexEncoding::Lengths16
        .encode_into(&[3, 2], mirx::ByteAlignment::ONE, &mut index)
        .unwrap();
    let asset = EncodedImageAsset::from_groups(surface, &codings, &records, &[1, 1, 2, 0, 3])
        .with_unit_index(&index[..index_len])
        .with_integrity(DataIntegrity::Indexed(&[3, 5]));
    let mut output = [0xad; 512];
    let (len, allocations) = count_allocations(|| {
        asset.preflight(&PayloadLimits::EMBEDDED).unwrap();
        let len = asset.encode_into(&mut output).unwrap();
        assert_eq!(asset.encoded_len(), Ok(len));
        assert_eq!(asset.matches_payload(&output[..len]), Ok(true));
        EncodedImageView::open(&output[..len])
            .unwrap()
            .preflight(&PayloadLimits::EMBEDDED)
            .unwrap();
        len
    });
    assert_eq!(allocations, 0);
    assert!(output[len..].iter().all(|b| *b == 0xad));
    let mut document = Document::new();
    let (id, allocations) = count_allocations(|| document.push_encoded_image(&asset).unwrap());
    assert_eq!(allocations, 2);
    let (_, allocations) = count_allocations(|| {
        document
            .get_mut(id)
            .unwrap()
            .replace_encoded_image(&asset)
            .unwrap()
    });
    assert_eq!(allocations, 0);
    let changed = EncodedImageAsset::from_groups(surface, &codings, &records, &[1, 1, 2, 0, 4])
        .with_unit_index(&index[..index_len])
        .with_integrity(DataIntegrity::Indexed(&[3, 5]));
    let (_, allocations) = count_allocations(|| {
        document
            .get_mut(id)
            .unwrap()
            .replace_encoded_image(&changed)
            .unwrap()
    });
    assert_eq!(allocations, 1);
    let invalid = asset.with_unit_index(&index[..index_len - 1]);
    let (_, allocations) = count_allocations(|| {
        assert!(
            document
                .get_mut(id)
                .unwrap()
                .replace_encoded_image(&invalid)
                .is_err()
        )
    });
    assert_eq!(allocations, 0);
}

#[test]
fn typed_encoded_edits_allocate_only_final_payloads_and_keep_noops_borrowed() {
    use mirx::{coding::Rle, image::EncodedImageAsset};
    let surface = SurfaceDescriptor::new(8, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let asset = EncodedImageAsset::new(surface, Rle::new().record(), &[0x87, 42]);
    let malformed = EncodedImageAsset::new(surface, Rle::new().record(), &[0xff]);
    let mut document = Document::new();
    let (_, allocations) = count_allocations(|| {
        asset.preflight(&PayloadLimits::EMBEDDED).unwrap();
        assert!(document.push_encoded_image(&malformed).is_err());
        assert!(
            document
                .push_encoded_image(
                    &asset.with_input_alignment(mirx::ByteAlignment::new(1 << 31).unwrap()),
                )
                .is_err()
        );
    });
    assert_eq!(allocations, 0);
    let (id, allocations) = count_allocations(|| document.push_encoded_image(&asset).unwrap());
    assert_eq!(allocations, 2, "one payload and one document node table");
    document.set_primary(id).unwrap();
    let bytes = document.encode(&EncodeOptions::new()).unwrap();
    let mut document = Document::open(&bytes).unwrap();
    let id = document.chunks().next().unwrap().id();
    let pointer = document.get(id).unwrap().payload_bytes().unwrap().as_ptr();
    let (_, allocations) = count_allocations(|| {
        document
            .get_mut(id)
            .unwrap()
            .replace_encoded_image(&asset)
            .unwrap();
        assert!(!document.is_dirty());
        assert_eq!(
            document.get(id).unwrap().payload_bytes().unwrap().as_ptr(),
            pointer
        );
        assert!(
            document
                .get_mut(id)
                .unwrap()
                .replace_encoded_image(&malformed)
                .is_err()
        );
        assert!(!document.is_dirty());
    });
    assert_eq!(allocations, 0);
    let changed = EncodedImageAsset::new(surface, Rle::new().record(), &[0x87, 43]);
    let (_, allocations) = count_allocations(|| {
        document
            .get_mut(id)
            .unwrap()
            .replace_encoded_image(&changed)
            .unwrap()
    });
    assert_eq!(allocations, 1);
    assert!(document.is_dirty());
}

#[test]
fn encoded_document_queries_and_placement_allocate_no_samples_or_group_table() {
    use mirx::{
        PayloadInput, RawChunkInput, RawChunkPolicy, coding::Rle, image::EncodedImageAsset,
    };
    let surface = SurfaceDescriptor::new(8, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let payload = EncodedImageAsset::new(surface, Rle::new().record(), &[0x87, 42])
        .with_input_alignment(mirx::ByteAlignment::new(64).unwrap())
        .encode()
        .unwrap();
    let mut document = Document::new();
    let (_, allocations) = count_allocations(|| {
        document
            .push_raw(RawChunkInput {
                chunk_type: ChunkType::IMAGE,
                flags: ChunkFlags::CRITICAL,
                payload: PayloadInput::Borrowed(&payload),
                policy: RawChunkPolicy::infer(),
            })
            .unwrap();
    });
    assert_eq!(allocations, 1, "only the document node table is owned");
    let id = document.chunks().next().unwrap().id();
    let mut output = [0; 512];
    let (_, allocations) = count_allocations(|| {
        document.set_primary(id).unwrap();
        let image = document
            .get(id)
            .unwrap()
            .image()
            .unwrap()
            .encoded()
            .unwrap();
        assert_eq!(
            image.input_alignment().map(mirx::ByteAlignment::get),
            Ok(64)
        );
        assert_eq!(document.primary_hints().stride(), 0);
        let size = document
            .encode_into(&mut output, &EncodeOptions::new())
            .unwrap();
        Reader::open(&output[..size]).unwrap();
    });
    assert_eq!(allocations, 0);
}

#[test]
fn critical_encoded_reader_uses_no_decoder_or_group_allocation() {
    use mirx::{
        coding::Rle,
        image::{CoverageBudget, EncodedImageAsset},
    };
    let surface = SurfaceDescriptor::new(8, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let payload = EncodedImageAsset::new(surface, Rle::new().record(), &[0x87, 42])
        .encode()
        .unwrap();
    let bytes = encode_chunks(&[(
        ChunkType::IMAGE.raw(),
        ChunkFlags::CRITICAL.bits(),
        &payload,
    )]);
    let mut slots = [None];
    let mut output = [0; 8];
    let (_, allocations) = count_allocations(|| {
        let reader = Reader::open(&bytes).unwrap();
        reader
            .validate_known_payloads(&PayloadLimits::EMBEDDED)
            .unwrap();
        let image = reader
            .chunks()
            .next()
            .unwrap()
            .image()
            .unwrap()
            .unwrap()
            .encoded()
            .unwrap();
        let groups = image
            .groups_into(&mut slots, &mut CoverageBudget::new(100))
            .unwrap();
        groups
            .get(0)
            .unwrap()
            .get(0)
            .unwrap()
            .decode_plan(SurfaceRequirements::new())
            .unwrap()
            .decode_into(&mut output)
            .unwrap();
        assert_eq!(output, [42; 8]);
    });
    assert_eq!(allocations, 0);
}

#[test]
fn encoded_preflight_has_no_output_or_group_table_allocation() {
    use mirx::{
        coding::Lz4,
        image::{EncodedImageAsset, EncodedImageView},
    };
    let surface = SurfaceDescriptor::new(33, 8, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let mut table = [0; Lz4::TABLE_LEN];
    let mut stream = [0; 288];
    let codec = Lz4::new();
    let len = codec
        .encoder(&mut table)
        .unwrap()
        .encode_into(&[42; 264], &mut stream)
        .unwrap();
    let bytes = EncodedImageAsset::new(surface, codec.record(), &stream[..len])
        .encode()
        .unwrap();
    let (_, allocations) = count_allocations(|| {
        let image = EncodedImageView::open(&bytes).unwrap();
        image.preflight(&PayloadLimits::EMBEDDED).unwrap();
        assert!(
            image
                .preflight(&PayloadLimits::EMBEDDED.with_max_decoded_bytes(263))
                .is_err()
        );
        assert!(
            image
                .preflight(&PayloadLimits::EMBEDDED.with_max_raster_work(0))
                .is_err()
        );
    });
    assert_eq!(allocations, 0);
}

#[test]
fn constant_space_group_validation_has_no_hidden_table() {
    use mirx::{
        coding::Rle,
        image::{CoverageBudget, EncodedImageAsset, EncodedImageView},
    };
    let surface = SurfaceDescriptor::new(
        3,
        3,
        SampleLayout::NV12,
        ColorDescription::BT709_YUV_LIMITED,
    )
    .unwrap();
    let bytes = EncodedImageAsset::new(surface, Rle::new().record(), &[0x90, 128])
        .with_input_alignment(mirx::ByteAlignment::new(64).unwrap())
        .encode()
        .unwrap();
    let (_, allocations) = count_allocations(|| {
        let image = EncodedImageView::open_at(&bytes, 0).unwrap();
        image
            .validate_groups(&mut CoverageBudget::new(10_000))
            .unwrap();
        assert!(image.validate_groups(&mut CoverageBudget::new(0)).is_err());
    });
    assert_eq!(allocations, 0);
}

#[test]
fn image_storage_dispatch_does_not_allocate_or_decode() {
    use mirx::{
        coding::Rle,
        image::{EncodedImageAsset, ImageRef},
    };
    let surface = SurfaceDescriptor::new(4, 2, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let raw = RawImageAsset::new(surface, &[&[42; 8]]).encode().unwrap();
    let encoded = EncodedImageAsset::new(surface, Rle::new().record(), &[0x87, 42])
        .encode()
        .unwrap();
    let (_, allocations) = count_allocations(|| {
        let image = ImageRef::open_at(&raw, 0).unwrap();
        assert_eq!(image.surface(), surface);
        assert_eq!(image.raw().unwrap().plane(0).unwrap().bytes(), &[42; 8]);
        assert!(image.encoded().is_none());
        assert!(
            image
                .raw()
                .unwrap()
                .access_capabilities()
                .supports_direct_borrow()
        );
        let image = ImageRef::open_at(&encoded, 0).unwrap();
        assert_eq!(image.surface(), surface);
        assert!(image.raw().is_none());
        assert!(image.encoded().is_some());
    });
    assert_eq!(allocations, 0);
}

#[test]
fn encoded_image_authoring_and_decode_use_only_caller_storage() {
    use mirx::{
        coding::Rle,
        image::{CoverageBudget, EncodedImageAsset, EncodedImageView},
    };
    let surface = SurfaceDescriptor::new(
        3,
        3,
        SampleLayout::NV12,
        ColorDescription::BT709_YUV_LIMITED,
    )
    .unwrap();
    let mut bytes = [0xad; 256];
    let mut stream = [0; 32];
    let mut output = [0; 320];
    let mut slots = [None];
    let (_, allocations) = count_allocations(|| {
        let codec = Rle::new();
        let len = codec.encode_into(&[128; 17], &mut stream).unwrap();
        let asset = EncodedImageAsset::new(surface, codec.record(), &stream[..len])
            .with_input_alignment(mirx::ByteAlignment::new(64).unwrap());
        let size = asset.encoded_len().unwrap();
        assert_eq!(asset.encode_into(&mut bytes), Ok(size));
        assert_eq!(asset.matches_payload(&bytes[..size]), Ok(true));
        let view = EncodedImageView::open_at(&bytes[..size], 0).unwrap();
        let groups = view
            .groups_into(&mut slots, &mut CoverageBudget::new(100))
            .unwrap();
        assert!(groups.access_capabilities().unwrap().supports_region());
        assert_eq!(groups.validate_unit(0, 0), Ok(len as u32));
        let plan = groups
            .get(0)
            .unwrap()
            .get(0)
            .unwrap()
            .decode_plan(SurfaceRequirements::new().with_stride_multiple(64))
            .unwrap();
        let decoded = plan.decode_into(&mut output).unwrap();
        assert_eq!(
            decoded.plane(1).unwrap().row(1).unwrap(),
            Some(&[128; 4][..])
        );
    });
    assert_eq!(allocations, 0);
}

#[test]
fn aligned_indexes_keep_checkpoints_and_exact_ranges_without_allocation() {
    use mirx::media::{UnitIndex, UnitIndexEncoding};
    let mut bytes = [0xad; 272];
    let lengths = [3; 65];
    let (_, allocations) = count_allocations(|| {
        for encoding in [UnitIndexEncoding::Lengths16, UnitIndexEncoding::Lengths32] {
            let alignment = mirx::ByteAlignment::new(64).unwrap();
            let len = encoding
                .encode_into(&lengths, alignment, &mut bytes)
                .unwrap();
            let index = if encoding == UnitIndexEncoding::Lengths16 {
                UnitIndex::lengths16(65, &bytes[..len], alignment)
            } else {
                UnitIndex::lengths32(65, &bytes[..len], alignment)
            }
            .unwrap();
            assert_eq!(index.byte_len(), 4099);
            assert_eq!(index.get(64), Some(4096..4099));
            assert!(index.iter().eq((0..65).map(|i| i * 64..i * 64 + 3)));
            assert!(
                index
                    .iter()
                    .rev()
                    .eq((0..65).rev().map(|i| i * 64..i * 64 + 3))
            );
        }
    });
    assert_eq!(allocations, 0);
}

#[test]
fn lz4_plane_history_has_no_staging_or_heap_index() {
    use mirx::{coding::Lz4, image::UnitGroup};
    let surface = SurfaceDescriptor::new(33, 8, SampleLayout::A1, ColorDescription::NONE).unwrap();
    let mut table = [0; Lz4::TABLE_LEN];
    let mut encoded = [0; 64];
    let mut input = [0; 40];
    for (i, byte) in input.iter_mut().enumerate() {
        *byte = [1, 2, 3, 4, 255, 6][i % 6];
    }
    let mut output = [0xad; 72];
    let (_, allocations) = count_allocations(|| {
        let codec = Lz4::new();
        let len = codec
            .encoder(&mut table)
            .unwrap()
            .encode_into(&input, &mut encoded)
            .unwrap();
        let unit = UnitGroup::builder(surface, codec.record(), &encoded[..len])
            .build()
            .unwrap()
            .get(0)
            .unwrap();
        let plan = unit
            .decode_plan(SurfaceRequirements::new().with_stride_multiple(8))
            .unwrap();
        let decoded = plan.decode_into(&mut output).unwrap();
        for (actual, expected) in decoded
            .plane(0)
            .unwrap()
            .rows()
            .unwrap()
            .zip(input.chunks_exact(5))
        {
            assert_eq!(&actual[..4], &expected[..4]);
            assert_eq!(actual[4], expected[4] & 0x80);
        }
    });
    assert_eq!(allocations, 0);
}

#[test]
fn lz4_encoder_uses_only_the_borrowed_table_and_output() {
    use mirx::coding::Lz4;
    let mut table = [u32::MAX; Lz4::TABLE_LEN];
    let mut encoded = [0xad; 32];
    let mut output = [0; 128];
    let (_, allocations) = count_allocations(|| {
        let mut encoder = Lz4::new().encoder(&mut table).unwrap();
        let len = encoder.encoded_len(&[42; 128]).unwrap();
        assert!(
            encoder
                .encode_into(&[42; 128], &mut encoded[..len - 1])
                .is_err()
        );
        assert_eq!(encoded, [0xad; 32]);
        assert_eq!(encoder.encode_into(&[42; 128], &mut encoded), Ok(len));
        Lz4::new()
            .plan(&encoded[..len], 128)
            .unwrap()
            .decode_into(&mut output)
            .unwrap();
        assert_eq!(output, [42; 128]);
        assert!(encoded[len..].iter().all(|b| *b == 0xad));
    });
    assert_eq!(allocations, 0);
}

#[test]
fn lz4_preflight_and_history_replay_allocate_nothing() {
    use mirx::coding::Lz4;
    let input = [0x13, b'a', 1, 0, 0x50, b't', b'a', b'i', b'l', b'!'];
    let mut output = [0xad; 16];
    let (_, allocations) = count_allocations(|| {
        let codec = Lz4::from_record(Lz4::new().record()).unwrap();
        let plan = codec.plan(&input, 13).unwrap();
        assert!(plan.decode_into(&mut output[..12]).is_err());
        assert_eq!(output, [0xad; 16]);
        assert_eq!(plan.decode_into(&mut output), Ok(13));
    });
    assert_eq!(allocations, 0);
    assert_eq!(&output[..13], b"aaaaaaaatail!");
    assert_eq!(&output[13..], &[0xad; 3]);
}

#[test]
fn rle_planar_execution_allocates_neither_plane_tables_nor_staging() {
    use mirx::{
        coding::Rle,
        image::{GroupPlanes, UnitGroup},
    };
    #[repr(align(64))]
    struct Buffer([u8; 256]);
    let mut output = Buffer([0xad; 256]);
    let (_, allocations) = count_allocations(|| {
        let surface = SurfaceDescriptor::new(
            5,
            3,
            SampleLayout::NV12,
            ColorDescription::BT709_YUV_LIMITED,
        )
        .unwrap();
        let codec = Rle::new();
        let unit = UnitGroup::builder(surface, codec.record(), &[0x8b, 128])
            .with_planes(GroupPlanes::Plane(1))
            .build()
            .unwrap()
            .get(0)
            .unwrap();
        let plan = unit
            .decode_plan(
                SurfaceRequirements::new()
                    .with_base_alignment(mirx::ByteAlignment::new(64).unwrap())
                    .with_stride_multiple(64),
            )
            .unwrap();
        let decoded = plan.decode_into(&mut output.0).unwrap();
        assert!(decoded.plane(0).is_none());
        assert_eq!(
            decoded.plane(1).unwrap().row(1).unwrap(),
            Some(&[128; 6][..])
        );
    });
    assert_eq!(allocations, 0);
}

#[test]
fn rle_selection_encoding_and_validated_decode_allocate_nothing() {
    let input = [7; 384];
    let mut encoded = [0; 768];
    let mut output = [0; 384];
    let (_, allocations) = count_allocations(|| {
        for size in 1..=4 {
            let codec = mirx::coding::Rle::new().with_element_size(size).unwrap();
            let len = codec.encoded_len(&input).unwrap();
            assert_eq!(codec.encode_into(&input, &mut encoded).unwrap(), len);
            codec
                .plan(&encoded[..len], input.len())
                .unwrap()
                .decode_into(&mut output)
                .unwrap();
        }
    });
    assert_eq!(allocations, 0);
    assert_eq!(input, output);
}

#[test]
fn pixel_unit_plan_and_strided_execution_need_no_staging_allocation() {
    use mirx::image::UnitGroup;
    #[repr(align(64))]
    struct Buffer([u8; 256]);
    let mut output = Buffer([0xad; 256]);
    let (height, allocations) = count_allocations(|| {
        let surface =
            SurfaceDescriptor::new(20, 2, SampleLayout::RGBA8888, ColorDescription::SRGB).unwrap();
        let codec = mirx::coding::Pixel::new(surface.sample_layout()).unwrap();
        let unit = UnitGroup::builder(surface, codec.record(), &[39])
            .build()
            .unwrap()
            .get(0)
            .unwrap();
        let plan = unit
            .decode_plan(
                SurfaceRequirements::new()
                    .with_base_alignment(mirx::ByteAlignment::new(64).unwrap())
                    .with_stride_multiple(64),
            )
            .unwrap();
        let decoded = plan.decode_into(&mut output.0).unwrap();
        decoded.plane(0).unwrap().geometry().height()
    });
    assert_eq!(height, 2);
    assert_eq!(allocations, 0);
    assert_eq!(&output.0[..4], &[0, 0, 0, 255]);
    assert_eq!(&output.0[80..128], &[0; 48]);
}

#[test]
fn pixel_count_encode_plan_and_decode_allocate_nothing() {
    let samples = [11, 22, 33, 255, 11, 22, 33, 255, 12, 23, 34, 255];
    let mut encoded = [0; 32];
    let mut output = [0; 12];
    let (written, allocations) = count_allocations(|| {
        let codec = mirx::coding::Pixel::new(SampleLayout::RGBA8888).unwrap();
        let len = codec.encoded_len(&samples).unwrap();
        assert_eq!(codec.encode_into(&samples, &mut encoded).unwrap(), len);
        codec
            .plan(&encoded[..len], 3)
            .unwrap()
            .decode_into(&mut output)
            .unwrap()
    });
    assert_eq!(allocations, 0);
    assert_eq!(written, samples.len());
    assert_eq!(samples, output);
}

thread_local! {
    static TRACKING: Cell<bool> = const { Cell::new(false) };
    static ALLOCATION_COUNT: Cell<usize> = const { Cell::new(0) };
}

fn record_allocation() {
    let _ = TRACKING.try_with(|tracking| {
        if tracking.get() {
            let _ = ALLOCATION_COUNT.try_with(|count| count.set(count.get() + 1));
        }
    });
}

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `System` receives the unchanged allocation layout.
        let pointer = unsafe { System.alloc(layout) };
        record_allocation();
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `System` receives the unchanged allocation layout.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        record_allocation();
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: the pointer and layout came from this allocator's `System` delegate.
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the pointer and layout came from this allocator's `System` delegate.
        let pointer = unsafe { System.realloc(pointer, layout, new_size) };
        record_allocation();
        pointer
    }
}

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn count_allocations<T>(operation: impl FnOnce() -> T) -> (T, usize) {
    TRACKING.with(|tracking| tracking.set(false));
    ALLOCATION_COUNT.with(|count| count.set(0));
    TRACKING.with(|tracking| tracking.set(true));
    let result = operation();
    TRACKING.with(|tracking| tracking.set(false));
    let count = ALLOCATION_COUNT.with(Cell::get);
    (result, count)
}

fn typed_container() -> Vec<u8> {
    let pixels = [0x00, 0x40, 0x80, 0xff];
    let image = ImageAsset::new(2, 2, ColorFormat::A8, 2, Cow::Borrowed(&pixels))
        .encode_payload()
        .unwrap();
    let meta = Meta::from_entries(vec![MetaEntry::text("name", "fixture")])
        .encode_payload()
        .unwrap();
    let palette = Palette::from_colors(vec![Color::rgba(1, 2, 3, 4), Color::rgba(5, 6, 7, 8)])
        .encode_payload()
        .unwrap();
    let sequence = FrameSequence::new(1, 1_000, 40).unwrap();
    let surface = SurfaceDescriptor::new(2, 2, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let mut encoder = FramesEncoder::new(sequence, surface).unwrap();
    encoder.push(&pixels).unwrap();
    let frames = encoder.finish().unwrap();

    encode_chunks(&[
        (ChunkType::IMAGE.raw(), ChunkFlags::NONE.bits(), &image),
        (ChunkType::META.raw(), ChunkFlags::NONE.bits(), &meta),
        (ChunkType::PALETTE.raw(), ChunkFlags::NONE.bits(), &palette),
        (
            ChunkType::FRAMES.raw(),
            ChunkFlags::NONE.bits(),
            frames.payload(),
        ),
    ])
}

fn empty_media_payload() -> Vec<u8> {
    let mut bytes = vec![0; MEDIA_HEADER_LEN + MEDIA_CRC_LEN];
    bytes[0] = 1;
    let crc = mirx::crc32(&[1, 0, 0, 0, 0, 0, 0, 0]);
    bytes[4..8].copy_from_slice(&crc.to_le_bytes());
    bytes
}

fn raw_a8_media_payload() -> Vec<u8> {
    let surface = SurfaceDescriptor::new(1, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let section_count = 2u16;
    let surface_offset = MEDIA_HEADER_LEN + usize::from(section_count) * MEDIA_SECTION_LEN;
    let data_offset = surface_offset + SURFACE_RECORD_LEN;
    let mut bytes = vec![0; data_offset + 1 + MEDIA_CRC_LEN];
    bytes[0] = 1;
    bytes[2..4].copy_from_slice(&section_count.to_le_bytes());

    bytes[MEDIA_HEADER_LEN..MEDIA_HEADER_LEN + 2]
        .copy_from_slice(&mirx::media::MediaSectionKind::SURFACE.raw().to_le_bytes());
    bytes[MEDIA_HEADER_LEN + 2..MEDIA_HEADER_LEN + 4].copy_from_slice(&1u16.to_le_bytes());
    bytes[MEDIA_HEADER_LEN + 4..MEDIA_HEADER_LEN + 8]
        .copy_from_slice(&(surface_offset as u32).to_le_bytes());
    bytes[MEDIA_HEADER_LEN + 8..MEDIA_HEADER_LEN + 12]
        .copy_from_slice(&(SURFACE_RECORD_LEN as u32).to_le_bytes());

    let data_entry = MEDIA_HEADER_LEN + MEDIA_SECTION_LEN;
    bytes[data_entry..data_entry + 2]
        .copy_from_slice(&mirx::media::MediaSectionKind::DATA.raw().to_le_bytes());
    bytes[data_entry + 2..data_entry + 4].copy_from_slice(&1u16.to_le_bytes());
    bytes[data_entry + 4..data_entry + 8].copy_from_slice(&(data_offset as u32).to_le_bytes());
    bytes[data_entry + 8..data_entry + 12].copy_from_slice(&1u32.to_le_bytes());

    surface
        .encode_record_into(&mut bytes[surface_offset..data_offset])
        .unwrap();
    bytes[data_offset] = 0x7f;
    let crc_offset = bytes.len() - MEDIA_CRC_LEN;
    let crc = mirx::crc32(&bytes[data_offset..crc_offset]);
    bytes[crc_offset..].copy_from_slice(&crc.to_le_bytes());
    let mut metadata = bytes[..4].to_vec();
    metadata.extend_from_slice(&bytes[8..data_offset]);
    metadata.extend_from_slice(&bytes[crc_offset..]);
    let crc = mirx::crc32(&metadata);
    bytes[4..8].copy_from_slice(&crc.to_le_bytes());
    bytes
}

#[test]
fn common_media_inspection_allocates_nothing() {
    let bytes = empty_media_payload();
    let (observed, allocations) = count_allocations(|| {
        let media = MediaPayload::open(&bytes).unwrap();
        (media.header().section_count(), media.sections().count())
    });
    assert_eq!(observed, (0, 0));
    assert_eq!(allocations, 0);
}

#[test]
fn direct_directory_lookup_and_skips_allocate_nothing() {
    let bytes = raw_a8_media_payload();
    let (_, allocations) = count_allocations(|| {
        let media = MediaPayload::open(&bytes).unwrap();
        let first = media.get(0).unwrap();
        let second = media.get(1).unwrap();
        assert_eq!(first.index(), 0);
        assert_eq!(second.index(), 1);
        assert_eq!(media.sections().nth(1), Some(second));
        assert_eq!(media.sections().nth_back(1), Some(first));
        assert_eq!(media.sections().last(), Some(second));
        assert_eq!(media.sections().count(), 2);
        assert_eq!(media.get(usize::MAX), None);
    });
    assert_eq!(allocations, 0);
}

#[test]
fn coding_table_read_and_caller_buffer_encoding_allocate_nothing() {
    use mirx::media::{CodingId, CodingRecord, CodingTable};
    let records = [
        CodingRecord::new(CodingId::new(0x8000), 3, &[1, 2, 3]),
        CodingRecord::new(CodingId::new(0xffff), 0xffff, &[]),
    ];
    let mut encoded = [0; 23];
    let mut copy = [0; 23];
    let (result, allocations) = count_allocations(|| {
        let len = CodingTable::encode_into(&records, &mut encoded).unwrap();
        let table = CodingTable::open(&encoded[..len]).unwrap();
        assert!(table.iter().eq(records));
        assert_eq!(
            table.get(0).unwrap().params().as_ptr(),
            encoded[20..].as_ptr()
        );
        CodingTable::encode_into(&[table.get(0).unwrap(), table.get(1).unwrap()], &mut copy)
    });
    assert_eq!(result, Ok(23));
    assert_eq!(allocations, 0);
    assert_eq!(encoded, copy);
}

#[test]
fn unit_index_encoding_lookup_and_iteration_allocate_nothing() {
    use mirx::media::{UnitIndex, UnitIndexEncoding};
    let lengths = [3; 129];
    let mut offsets = [0; 520];
    let mut checkpointed = [0; 270];
    let (_, allocations) = count_allocations(|| {
        UnitIndexEncoding::Offsets
            .encode_into(&lengths, mirx::ByteAlignment::ONE, &mut offsets)
            .unwrap();
        UnitIndexEncoding::Lengths16
            .encode_into(&lengths, mirx::ByteAlignment::ONE, &mut checkpointed)
            .unwrap();
        for index in [
            UnitIndex::fixed(129, 3, mirx::ByteAlignment::ONE).unwrap(),
            UnitIndex::offsets(&offsets).unwrap(),
            UnitIndex::lengths16(129, &checkpointed, mirx::ByteAlignment::ONE).unwrap(),
        ] {
            assert_eq!(index.byte_len(), 387);
            assert_eq!(index.get(64), Some(192..195));
            assert_eq!(index.iter().nth(128), Some(384..387));
            assert_eq!(index.iter().nth_back(128), Some(0..3));
            assert!(index.iter().eq((0..129).map(|i| i * 3..i * 3 + 3)));
        }
    });
    assert_eq!(allocations, 0);
}

#[test]
fn shared_tile_and_chroma_region_resolution_allocate_nothing() {
    let surface = SurfaceDescriptor::new(
        319,
        181,
        SampleLayout::NV12,
        ColorDescription::BT709_YUV_LIMITED,
    )
    .unwrap();
    let (_, allocations) = count_allocations(|| {
        let grid = surface.tile_grid(64, 32).unwrap();
        assert_eq!(grid.len(), 30);
        for (ordinal, region) in grid.iter().enumerate() {
            assert_eq!(grid.get(ordinal), Some(region));
            assert_eq!(region.for_plane(surface, 0), Ok(region));
            let chroma = region.for_plane(surface, 1).unwrap();
            assert_eq!(chroma.x() * 2, region.x());
            assert_eq!(chroma.y() * 2, region.y());
        }
        let edge = grid.iter().next_back().unwrap();
        assert_eq!((edge.width(), edge.height()), (63, 21));
    });
    assert_eq!(allocations, 0);
}

#[test]
fn indexed_integrity_open_and_partial_verification_allocate_nothing() {
    use mirx::media::{IntegrityRange, IntegrityTable, MediaFlags, MediaSectionKind};
    let mut bytes = [0u8; 60];
    bytes[..4].copy_from_slice(&[1, MediaFlags::INDEXED_INTEGRITY.bits(), 2, 0]);
    for (index, (kind, offset, size)) in [
        (MediaSectionKind::INTEGRITY, 32u32, 24u32),
        (MediaSectionKind::DATA, 56, 4),
    ]
    .into_iter()
    .enumerate()
    {
        let entry = 8 + index * 12;
        bytes[entry..entry + 2].copy_from_slice(&kind.raw().to_le_bytes());
        bytes[entry + 2..entry + 4].copy_from_slice(&1u16.to_le_bytes());
        bytes[entry + 4..entry + 8].copy_from_slice(&offset.to_le_bytes());
        bytes[entry + 8..entry + 12].copy_from_slice(&size.to_le_bytes());
    }
    bytes[56..].copy_from_slice(&[1, 2, 3, 4]);
    let ranges = [
        IntegrityRange::new(56..58, mirx::crc32(&bytes[56..58])).unwrap(),
        IntegrityRange::new(58..60, mirx::crc32(&bytes[58..60])).unwrap(),
    ];
    IntegrityTable::encode_into(&ranges, &mut bytes[32..56]).unwrap();
    let mut metadata = bytes[..4].to_vec();
    metadata.extend_from_slice(&bytes[8..56]);
    bytes[4..8].copy_from_slice(&mirx::crc32(&metadata).to_le_bytes());
    let mut copy = [0; 24];
    let (_, allocations) = count_allocations(|| {
        let media = MediaPayload::open(&bytes).unwrap();
        assert_eq!(media.validate_data_range(56..57), Ok(2));
        media.validate_data().unwrap();
        let table = media.integrity().unwrap();
        assert!(table.iter().eq(ranges));
        IntegrityTable::encode_into(&[table.get(0).unwrap(), table.get(1).unwrap()], &mut copy)
            .unwrap();
    });
    assert_eq!(allocations, 0);
    assert_eq!(copy, bytes[32..56]);
}

#[test]
fn image_plane_geometry_allocates_nothing() {
    let (observed, allocations) = count_allocations(|| {
        SampleLayout::P010
            .planes(319, 181)
            .map(|plane| plane.minimum_stride().unwrap())
            .sum::<u32>()
    });
    assert_eq!(observed, 1_278);
    assert_eq!(allocations, 0);
}

#[test]
fn image_surface_record_round_trip_allocates_nothing() {
    let surface = SurfaceDescriptor::new(
        319,
        181,
        SampleLayout::NV12,
        ColorDescription::BT709_YUV_LIMITED,
    )
    .unwrap();
    let mut record = [0; SURFACE_RECORD_LEN];
    surface.encode_record_into(&mut record).unwrap();

    let (observed, allocations) =
        count_allocations(|| SurfaceDescriptor::from_record(&record).unwrap());
    assert_eq!(observed, surface);
    assert_eq!(allocations, 0);
}

#[test]
fn image_plane_memory_record_round_trip_allocates_nothing() {
    let plane = SampleLayout::RGBA8888.plane_geometry(319, 181, 0).unwrap();
    let memory = PlaneMemoryLayout::builder(plane)
        .with_allocation_extent(320, 192)
        .with_stride(1_280)
        .with_alignment(mirx::ByteAlignment::new(64).unwrap())
        .build()
        .unwrap();
    let mut record = [0; PLANE_RECORD_LEN];
    memory.encode_record_into(&mut record).unwrap();

    let (observed, allocations) =
        count_allocations(|| PlaneMemoryLayout::from_record(plane, &record).unwrap());
    assert_eq!(observed, memory);
    assert_eq!(allocations, 0);
}

#[test]
fn raw_image_open_and_plane_iteration_allocate_nothing() {
    let bytes = raw_a8_media_payload();
    let (observed, allocations) = count_allocations(|| {
        let image = RawImageView::open(&bytes).unwrap();
        let plane = image.planes().next().unwrap();
        (
            image.plane_count(),
            plane.bytes()[0],
            plane.bytes().as_ptr(),
        )
    });
    assert_eq!(observed.0, 1);
    assert_eq!(observed.1, 0x7f);
    assert_eq!(allocations, 0);
}

#[test]
fn raw_image_sizing_encoding_and_reopen_allocate_nothing() {
    let surface = SurfaceDescriptor::new(
        3,
        2,
        SampleLayout::NV12,
        ColorDescription::BT709_YUV_LIMITED,
    )
    .unwrap();
    let y = [0x10; 6];
    let uv = [0x80; 4];
    let planes: &[&[u8]] = &[&y, &uv];
    let asset = RawImageAsset::new(surface, planes);
    let needed = asset.encoded_len().unwrap();
    let mut output = vec![0xa5; needed + 11];

    let (observed, allocations) = count_allocations(|| {
        let view = asset.view().unwrap();
        let written = view.encode_into(&mut output).unwrap();
        assert!(view.matches_payload(&output[..written]).unwrap());
        let image = RawImageView::open(&output[..written]).unwrap();
        (
            written,
            image.plane_count(),
            image.plane(0).unwrap().bytes().as_ptr(),
            image.plane(1).unwrap().bytes().as_ptr(),
        )
    });

    assert_eq!(observed.0, needed);
    assert_eq!(observed.1, 2);
    assert_eq!(allocations, 0);
    assert_eq!(&output[needed..], &[0xa5; 11]);
}

#[test]
fn image_surface_memory_planning_allocates_nothing() {
    let surface = SurfaceDescriptor::new(
        319,
        181,
        SampleLayout::NV12,
        ColorDescription::BT709_YUV_LIMITED,
    )
    .unwrap();
    let requirements = SurfaceRequirements::new()
        .with_base_alignment(mirx::ByteAlignment::new(64).unwrap())
        .with_plane_alignment(mirx::ByteAlignment::new(64).unwrap())
        .with_width_multiple(64)
        .with_stride_multiple(64);

    let (observed, allocations) = count_allocations(|| {
        let plan = surface.memory_plan(requirements).unwrap();
        (
            plan.byte_len(),
            plan.buffer_requirements().base_alignment(),
            plan.planes().map(|plane| plane.stride()).sum::<u32>(),
        )
    });

    assert_eq!(
        observed,
        (92_864, mirx::ByteAlignment::new(64).unwrap(), 704)
    );
    assert_eq!(allocations, 0);
}

#[test]
fn decoded_surface_views_allocate_nothing_and_keep_plane_pointers() {
    let payload = raw_a8_media_payload();
    let image = RawImageView::open(&payload).unwrap();
    let pixel_pointer = image.plane(0).unwrap().bytes().as_ptr();
    let ((wire_view, authored_view), allocations) = count_allocations(|| {
        let view = image.view();
        let planes = [view.plane(0).unwrap().bytes()];
        let authored = RawImageAsset::new(view.surface(), &planes).view().unwrap();
        assert_eq!(view.planes().len(), 1);
        assert!(view.data_addresses_are_aligned());
        (view, authored)
    });
    assert_eq!(allocations, 0);
    assert_eq!(wire_view.plane(0).unwrap().bytes().as_ptr(), pixel_pointer);
    assert_eq!(
        authored_view.plane(0).unwrap().bytes().as_ptr(),
        pixel_pointer
    );
}

#[test]
fn borrowed_reads_and_caller_buffer_encoding_allocate_nothing() {
    let bytes = typed_container();
    let (observed, read_allocations) = count_allocations(|| {
        let reader = Reader::open(&bytes).unwrap();
        let limits = PayloadLimits::HOST;
        let mut observed = 0usize;
        for chunk in reader.chunks() {
            if let Some(image) = chunk.image().unwrap() {
                observed += image
                    .raw()
                    .unwrap()
                    .planes()
                    .map(|plane| plane.bytes().len())
                    .sum::<usize>();
            }
            if let Some(meta) = chunk.meta(&limits).unwrap() {
                observed += meta.entries().count();
            }
            if let Some(palette) = chunk.palette(&limits).unwrap() {
                observed += palette.colors().iter().count();
            }
            if let Some(frames) = chunk.frames(&limits).unwrap() {
                observed += frames.sequence().frame_count() as usize;
                observed += frames.group_count();
            }
        }
        observed
    });
    assert_eq!(observed, 9);
    assert_eq!(read_allocations, 0);

    let (document, document_open_allocations) =
        count_allocations(|| Document::open(&bytes).unwrap());
    assert_eq!(document_open_allocations, 1);
    let output_len = document.encoded_len(&EncodeOptions::new()).unwrap();
    let mut output = vec![0xa5; output_len + 7];
    let (written, encode_allocations) = count_allocations(|| {
        document
            .encode_into(&mut output, &EncodeOptions::new())
            .unwrap()
    });
    assert_eq!(written, output_len);
    assert_eq!(encode_allocations, 0);
    assert_eq!(&output[written..], &[0xa5; 7]);
    let source_reader = Reader::open(&bytes).unwrap();
    let output_reader = Reader::open(&output[..written]).unwrap();
    output_reader
        .validate_known_payloads(&PayloadLimits::HOST)
        .unwrap();
    assert!(
        source_reader
            .chunks()
            .zip(output_reader.chunks())
            .all(|(source, output)| {
                source.chunk_type() == output.chunk_type()
                    && source.flags() == output.flags()
                    && source.payload() == output.payload()
            })
    );

    let no_op_document = Document::open(&bytes).unwrap();
    let (finished, finish_allocations) = count_allocations(|| no_op_document.finish().unwrap());
    assert_eq!(finish_allocations, 0);
    assert!(matches!(finished, Cow::Borrowed(_)));
    assert_eq!(finished.as_ref(), bytes);
}

#[test]
fn sectioned_image_noop_edits_and_reencoding_allocate_nothing() {
    let surface = SurfaceDescriptor::new(
        2,
        2,
        SampleLayout::NV12,
        ColorDescription::BT709_YUV_LIMITED,
    )
    .unwrap();
    let image = RawImageAsset::new(surface, &[&[16; 4], &[128; 2]]);
    let mut authored = Document::new();
    let id = authored.push_image(&image).unwrap();
    authored.set_primary(id).unwrap();
    let bytes = authored.encode(&EncodeOptions::new()).unwrap();
    let mut document = Document::open(&bytes).unwrap();
    let id = document.chunks().next().unwrap().id();
    let mut out = vec![0; bytes.len()];
    let (_, allocations) = count_allocations(|| {
        document.get_mut(id).unwrap().replace_image(&image).unwrap();
        assert!(!document.is_dirty());
        document
            .encode_into(&mut out, &EncodeOptions::new())
            .unwrap();
        let view = Reader::open(&out)
            .unwrap()
            .chunks()
            .next()
            .unwrap()
            .image()
            .unwrap()
            .unwrap();
        assert_eq!(view.surface(), surface);
    });
    assert_eq!(allocations, 0);
    assert_eq!(out, bytes);
}

#[test]
fn raw_surface_transfer_uses_only_the_caller_buffer() {
    #[repr(align(64))]
    struct Aligned([u8; 256]);
    let surface = SurfaceDescriptor::new(
        2,
        2,
        SampleLayout::NV12,
        ColorDescription::BT709_YUV_LIMITED,
    )
    .unwrap();
    let source = RawImageAsset::new(surface, &[&[16; 4], &[128; 2]])
        .view()
        .unwrap();
    let plan = surface
        .memory_plan(
            SurfaceRequirements::new()
                .with_base_alignment(mirx::ByteAlignment::new(64).unwrap())
                .with_plane_alignment(mirx::ByteAlignment::new(64).unwrap())
                .with_stride_multiple(64),
        )
        .unwrap();
    let mut output = Aligned([0xa5; 256]);
    let (_, allocations) = count_allocations(|| {
        let copied = source.copy_into(&mut output.0, plan).unwrap();
        assert!(copied.data_addresses_are_aligned());
        assert_eq!(copied.plane(0).unwrap().bytes()[0], 16);
        assert_eq!(copied.plane(0).unwrap().bytes()[64], 16);
        assert_eq!(&copied.plane(1).unwrap().bytes()[..2], &[128; 2]);
    });
    assert_eq!(allocations, 0);
    assert_eq!(&output.0[192..], &[0xa5; 64]);
}

#[test]
fn logical_plane_rows_borrow_without_allocating() {
    use mirx::image::{ColorDescription, RawImageAsset, SampleLayout, SurfaceDescriptor};

    let surface = SurfaceDescriptor::new(
        2,
        2,
        SampleLayout::NV12,
        ColorDescription::BT709_YUV_LIMITED,
    )
    .unwrap();
    let (_, allocations) = count_allocations(|| {
        let image = RawImageAsset::new(surface, &[&[16; 4], &[128; 2]])
            .view()
            .unwrap();
        let y = image.plane(0).unwrap();
        assert_eq!(y.row(1).unwrap().unwrap().as_ptr(), y.bytes()[2..].as_ptr());
        assert_eq!(y.rows().unwrap().next_back(), Some(&[16, 16][..]));
        assert_eq!(image.plane(1).unwrap().rows().unwrap().count(), 1);
    });
    assert_eq!(allocations, 0);
}

#[test]
fn sparse_unit_selection_borrows_and_encodes_without_allocation() {
    use mirx::media::{UnitSelection, UnitSelectionEncoding};
    let cells = [0, 7, 255, 256, 511];
    let mut list = [0; 20];
    let mut bitmap = [0; 72];
    let (_, allocations) = count_allocations(|| {
        UnitSelectionEncoding::List
            .encode_into(512, &cells, &mut list)
            .unwrap();
        UnitSelectionEncoding::Bitmap
            .encode_into(512, &cells, &mut bitmap)
            .unwrap();
        for selection in [
            UnitSelection::list(512, &list).unwrap(),
            UnitSelection::bitmap(512, &bitmap).unwrap(),
        ] {
            assert!(selection.iter().eq(cells));
            assert_eq!(selection.get(4), Some(511));
            assert_eq!(selection.position(256), Some(3));
            assert_eq!(selection.iter().nth_back(2), Some(255));
        }
        assert_eq!(
            UnitSelection::all(u32::MAX).unwrap().iter().last(),
            Some(u32::MAX - 1)
        );
    });
    assert_eq!(allocations, 0);
}

#[test]
fn image_units_resolve_shared_metadata_without_allocation() {
    use mirx::image::UnitGroup;
    use mirx::media::{CodingId, CodingRecord, UnitIndex, UnitSelection};
    let surface = SurfaceDescriptor::new(
        5,
        3,
        SampleLayout::NV12,
        ColorDescription::BT709_YUV_LIMITED,
    )
    .unwrap();
    let cells = [0, 0, 0, 0, 5, 0, 0, 0];
    let data = [1, 2, 3, 4];
    let (_, allocations) = count_allocations(|| {
        let group =
            UnitGroup::builder(surface, CodingRecord::new(CodingId::new(19), 1, &[]), &data)
                .with_tiles(2, 2)
                .with_selection(UnitSelection::list(6, &cells).unwrap())
                .with_index(UnitIndex::fixed(2, 2, mirx::ByteAlignment::ONE).unwrap())
                .build()
                .unwrap();
        assert_eq!(group.iter().count(), 2);
        let unit = group.cell(5).unwrap();
        assert_eq!(unit.data().as_ptr(), data[2..].as_ptr());
        assert_eq!(unit.plane_region(1).unwrap().x(), 2);
        assert_eq!(unit.plane_region(1).unwrap().y(), 1);
        assert_eq!(group.iter().nth_back(1).unwrap().cell(), 0);
    });
    assert_eq!(allocations, 0);
}

#[test]
fn group_record_read_write_and_resolution_allocate_nothing() {
    use mirx::image::UnitGroupRecord;
    use mirx::media::CodingTable;
    let surface = SurfaceDescriptor::new(2, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let codings = [1, 0, 0, 0, 19, 0, 1, 0, 0, 0, 0, 0];
    let data = [1, 2, 3, 4];
    let mut bytes = [0; 36];
    let (_, allocations) = count_allocations(|| {
        UnitGroupRecord::new(0, 0..4)
            .unwrap()
            .with_tiles(1, 1)
            .encode_into(&mut bytes)
            .unwrap();
        let group = UnitGroupRecord::open(&bytes)
            .unwrap()
            .resolve(surface, CodingTable::open(&codings).unwrap(), &data, &[])
            .unwrap();
        assert_eq!(group.get(1).unwrap().data().as_ptr(), data[2..].as_ptr());
        assert_eq!(group.get(1).unwrap().region().x(), 1);
    });
    assert_eq!(allocations, 0);
}

#[test]
fn image_coverage_and_selection_windows_allocate_nothing() {
    use mirx::image::{CoverageBudget, UnitGroup};
    use mirx::media::{CodingId, CodingRecord, UnitSelection};
    let surface = SurfaceDescriptor::new(2, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let cells = [0; 4];
    let (_, allocations) = count_allocations(|| {
        let selection = UnitSelection::list(2, &cells).unwrap();
        assert_eq!(selection.range(0..1).unwrap().len(), 1);
        let group = UnitGroup::builder(
            surface,
            CodingRecord::new(CodingId::new(19), 1, &[]),
            &[1; 2],
        )
        .with_tiles(1, 1)
        .build()
        .unwrap();
        assert_eq!(
            surface.validate_coverage(&[group], &mut CoverageBudget::new(10)),
            Ok(())
        );
    });
    assert_eq!(allocations, 0);
}

#[test]
fn encoded_image_open_prepare_and_integrity_allocate_nothing() {
    use mirx::image::{CoverageBudget, EncodedImageView};
    let mut bytes = [0u8; 96];
    bytes[0] = 1;
    bytes[2] = 3;
    for (entry, kind, offset, size) in [(8, 1, 44u32, 32u32), (20, 3, 76, 12), (32, 5, 88, 4)] {
        bytes[entry] = kind;
        bytes[entry + 2] = 1;
        bytes[entry + 4..entry + 8].copy_from_slice(&offset.to_le_bytes());
        bytes[entry + 8..entry + 12].copy_from_slice(&size.to_le_bytes());
    }
    bytes[44] = 4;
    bytes[48] = 1;
    bytes[52] = 0x23;
    bytes[76] = 1;
    bytes[80] = 19;
    bytes[82] = 1;
    bytes[88..92].copy_from_slice(&[1, 2, 3, 4]);
    let checksum = mirx::crc32(&bytes[88..92]);
    bytes[92..96].copy_from_slice(&checksum.to_le_bytes());
    let mut metadata = [0; 88];
    metadata[..4].copy_from_slice(&bytes[..4]);
    metadata[4..84].copy_from_slice(&bytes[8..88]);
    metadata[84..].copy_from_slice(&bytes[92..]);
    bytes[4..8].copy_from_slice(&mirx::crc32(&metadata).to_le_bytes());
    let mut workspace = [None];
    let (_, allocations) = count_allocations(|| {
        let image = EncodedImageView::open(&bytes).unwrap();
        let groups = image
            .groups_into(&mut workspace, &mut CoverageBudget::new(10))
            .unwrap();
        assert_eq!(
            groups.get(0).unwrap().get(0).unwrap().data().as_ptr(),
            bytes[88..].as_ptr()
        );
        assert_eq!(groups.validate_unit(0, 0), Ok(4));
    });
    assert_eq!(allocations, 0);
}

#[test]
fn decoded_unit_memory_planning_allocates_nothing() {
    use mirx::image::UnitGroup;
    use mirx::media::{CodingId, CodingRecord};
    let surface = SurfaceDescriptor::new(
        5,
        3,
        SampleLayout::NV12,
        ColorDescription::BT709_YUV_LIMITED,
    )
    .unwrap();
    let (_, allocations) = count_allocations(|| {
        let unit = UnitGroup::builder(
            surface,
            CodingRecord::new(CodingId::new(19), 1, &[]),
            &[1; 6],
        )
        .with_tiles(2, 2)
        .build()
        .unwrap()
        .get(5)
        .unwrap();
        let plan = unit
            .memory_plan(
                SurfaceRequirements::new()
                    .with_plane_alignment(mirx::ByteAlignment::new(64).unwrap())
                    .with_stride_multiple(64),
            )
            .unwrap();
        assert_eq!(plan.byte_len(), 128);
        assert_eq!(plan.plane(1).unwrap().source_region().x(), 2);
        assert_eq!(plan.plane(1).unwrap().memory().data_offset(), 64);
        assert_eq!(plan.planes().count(), 2);
    });
    assert_eq!(allocations, 0);
}
