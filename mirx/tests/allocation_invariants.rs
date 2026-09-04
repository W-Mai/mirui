use std::alloc::{GlobalAlloc, Layout, System};
use std::borrow::Cow;
use std::cell::Cell;

use mirx::image::{
    ColorDescription, PLANE_RECORD_LEN, PlaneMemoryLayout, RawImageAsset, RawImageView,
    SURFACE_RECORD_LEN, SampleLayout, SurfaceDescriptor, SurfaceRequirements,
};
use mirx::media::{MEDIA_CRC_LEN, MEDIA_HEADER_LEN, MEDIA_SECTION_LEN, MediaPayload};
use mirx::{
    AtlasFrames, ChunkFlags, ChunkType, Color, ColorFormat, Document, EncodeOptions, Frame,
    FramesAsset, ImageAsset, Meta, MetaEntry, Palette, PayloadLimits, Reader, encode_chunks,
};

struct TrackingAllocator;

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
            .with_input_alignment(64);
        let size = asset.encoded_len().unwrap();
        assert_eq!(asset.encode_into(&mut bytes), Ok(size));
        assert_eq!(asset.matches_payload(&bytes[..size]), Ok(true));
        let view = EncodedImageView::open_at(&bytes[..size], 0).unwrap();
        let groups = view
            .groups_into(&mut slots, &mut CoverageBudget::new(100))
            .unwrap();
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
            let len = encoding.encode_into(&lengths, 64, &mut bytes).unwrap();
            let index = if encoding == UnitIndexEncoding::Lengths16 {
                UnitIndex::lengths16(65, &bytes[..len], 64)
            } else {
                UnitIndex::lengths32(65, &bytes[..len], 64)
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
                    .with_base_alignment(64)
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
                    .with_base_alignment(64)
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
    let frames = FramesAsset::Atlas(AtlasFrames::new(
        ImageAsset::new(2, 2, ColorFormat::A8, 2, Cow::Borrowed(&pixels)),
        vec![Frame {
            source_x: 0,
            source_y: 0,
            width: 2,
            height: 2,
            target_x: 0,
            target_y: 0,
            duration_ticks: 0,
        }],
    ))
    .encode_payload()
    .unwrap();

    encode_chunks(&[
        (ChunkType::IMAGE.raw(), ChunkFlags::NONE.bits(), &image),
        (ChunkType::META.raw(), ChunkFlags::NONE.bits(), &meta),
        (ChunkType::PALETTE.raw(), ChunkFlags::NONE.bits(), &palette),
        (ChunkType::FRAMES.raw(), ChunkFlags::NONE.bits(), &frames),
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
            .encode_into(&lengths, 1, &mut offsets)
            .unwrap();
        UnitIndexEncoding::Lengths16
            .encode_into(&lengths, 1, &mut checkpointed)
            .unwrap();
        for index in [
            UnitIndex::fixed(129, 3, 1).unwrap(),
            UnitIndex::offsets(&offsets).unwrap(),
            UnitIndex::lengths16(129, &checkpointed, 1).unwrap(),
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
        .with_alignment(64)
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
        .with_base_alignment(64)
        .with_plane_alignment(64)
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

    assert_eq!(observed, (92_864, 64, 704));
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
                observed += frames.frames().count();
                observed += frames.atlas().main().len();
            }
        }
        observed
    });
    assert_eq!(observed, 12);
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
                .with_base_alignment(64)
                .with_plane_alignment(64)
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
fn shared_font_codepoints_validate_and_search_without_decoding_an_array() {
    let bytes = [0x41, 0, 0, 0, 0x2d, 0x4e, 0, 0, 0, 0xf6, 1, 0];
    let (_, allocations) = count_allocations(|| {
        let table = mirx::FontCodepoints::open(&bytes).unwrap();
        assert_eq!(table.binary_search('中'), Ok(1));
        assert_eq!(table.binary_search('B'), Err(1));
        assert_eq!(table.iter().next_back(), Some('😀'));
        assert_eq!(table.as_bytes().as_ptr(), bytes.as_ptr());
    });
    assert_eq!(allocations, 0);
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
                .with_index(UnitIndex::fixed(2, 2, 1).unwrap())
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
                    .with_plane_alignment(64)
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
