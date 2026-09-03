use std::alloc::{GlobalAlloc, Layout, System};
use std::borrow::Cow;
use std::cell::Cell;

use mirx::image::{
    ColorDescription, PLANE_RECORD_LEN, PlaneMemoryLayout, SURFACE_RECORD_LEN, SampleLayout,
    SurfaceDescriptor,
};
use mirx::media::{MEDIA_CRC_LEN, MEDIA_HEADER_LEN, MEDIA_SECTION_LEN, MediaPayload};
use mirx::{
    AtlasFrames, ChunkFlags, ChunkType, Color, ColorFormat, Document, EncodeOptions, Frame,
    FramesAsset, ImageAsset, Meta, MetaEntry, Palette, PayloadLimits, Reader, encode_chunks,
};

struct TrackingAllocator;

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
    bytes[4..6].copy_from_slice(&(MEDIA_SECTION_LEN as u16).to_le_bytes());
    bytes[8..12].copy_from_slice(&(MEDIA_HEADER_LEN as u32).to_le_bytes());
    let payload_len = bytes.len() as u32;
    bytes[12..16].copy_from_slice(&payload_len.to_le_bytes());
    let crc_offset = bytes.len() - MEDIA_CRC_LEN;
    let crc = mirx::crc32(&bytes[..crc_offset]);
    bytes[crc_offset..].copy_from_slice(&crc.to_le_bytes());
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
fn borrowed_reads_and_caller_buffer_encoding_allocate_nothing() {
    let bytes = typed_container();
    let (observed, read_allocations) = count_allocations(|| {
        let reader = Reader::open(&bytes).unwrap();
        let limits = PayloadLimits::HOST;
        let mut observed = 0usize;
        for chunk in reader.chunks() {
            if let Some(image) = chunk.image().unwrap() {
                observed += image.main().len();
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
