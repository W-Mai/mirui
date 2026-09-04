use super::*;
use crate::image::{PlaneAccessError, PlaneMemoryFlags};

#[repr(align(64))]
struct Buffer([u8; 1024]);

#[test]
fn repeated_cells_share_geometry_and_keep_allocation_rows_out_of_samples() {
    for layout in [
        SampleLayout::A1,
        SampleLayout::A2,
        SampleLayout::A4,
        SampleLayout::A8,
    ] {
        let map = GlyphMap::glyph_major(5, 3, 3).unwrap();
        let cell = SurfaceDescriptor::new(5, 3, layout, ColorDescription::NONE).unwrap();
        let memory = PlaneMemoryLayout::builder(cell.plane(0).unwrap())
            .with_allocation_extent(9, 5)
            .with_stride(16)
            .with_data_offset(64)
            .with_alignment(64)
            .build()
            .unwrap();
        let mut data = Buffer([0x5a; 1024]);
        let end = 64 + 2 * 128 + 80;
        for i in 0..3 {
            for row in 0..3 {
                let start = 64 + i * 128 + row * 16;
                data.0[start..start + 5].fill((i + 1) as u8 * 0x33);
            }
        }
        let glyphs = RawGlyphs::builder(map, layout)
            .with_memory_layout(memory)
            .build(&data.0[..end])
            .unwrap();
        assert_eq!(glyphs.len(), 3);
        assert!(!glyphs.is_empty());
        assert_eq!(glyphs.byte_len(), end as u32);
        assert_eq!(glyphs.sample_layout(), layout);
        assert_eq!(glyphs.memory_layout(), memory);
        assert!(glyphs.data_addresses_are_aligned());
        assert!(glyphs.file_address_is_aligned(128));
        assert!(!glyphs.file_address_is_aligned(1));
        assert!(!glyphs.file_address_is_aligned(u32::MAX - 63));
        let mut out = Buffer([0xa5; 1024]);
        for i in 0..3 {
            let glyph = glyphs.get(i).unwrap();
            let plane = glyph.storage().plane(0).unwrap();
            assert_eq!(plane.memory().data_offset(), 64 + i as u32 * 128);
            assert_eq!(plane.bytes().as_ptr(), data.0[64 + i * 128..].as_ptr());
            assert_eq!(glyph.region(), Region::new(0, 0, 5, 3).unwrap());
            assert_eq!(plane.rows().unwrap().len(), 3);
            let plan = glyph
                .memory_plan(
                    SurfaceRequirements::new()
                        .with_base_alignment(64)
                        .with_stride_multiple(64),
                )
                .unwrap();
            let decoded = glyph.copy_into(&mut out.0, plan).unwrap();
            let geometry = decoded.surface().plane(0).unwrap();
            let row_len = geometry.minimum_stride().unwrap() as usize;
            let sample = (i + 1) as u8 * 0x33;
            let tail = match layout {
                SampleLayout::A1 => 0xf8,
                SampleLayout::A2 => 0xc0,
                SampleLayout::A4 => 0xf0,
                SampleLayout::A8 => 0xff,
                _ => unreachable!(),
            };
            for row in decoded.plane(0).unwrap().rows().unwrap() {
                assert!(row[..row_len - 1].iter().all(|b| *b == sample));
                assert_eq!(row[row_len - 1], sample & tail);
            }
            for row in 0..3 {
                assert!(
                    out.0[row * 64 + row_len..(row + 1) * 64]
                        .iter()
                        .all(|b| *b == 0)
                );
            }
            assert!(out.0[192..].iter().all(|b| *b == 0xa5));
        }
        assert_eq!(glyphs.get(3), None);
        assert_eq!(glyphs.get(usize::MAX), None);
    }
}

#[test]
fn atlas_regions_share_samples_and_do_not_retain_map_or_source_lifetimes() {
    let data = [0x12, 0x34, 0x50, 0x67, 0x89, 0xa0];
    let glyph = {
        let regions = [
            Region::new(1, 0, 3, 2).unwrap(),
            Region::new(1, 0, 3, 2).unwrap(),
        ];
        let map = GlyphMap::atlas(5, 2, &regions).unwrap();
        let glyphs = RawGlyphs::builder(map, SampleLayout::A4)
            .build(&data)
            .unwrap();
        assert_eq!(glyphs.get(0), glyphs.get(1));
        glyphs.get(0).unwrap()
    };
    assert_eq!(
        glyph.storage().plane(0).unwrap().bytes().as_ptr(),
        data.as_ptr()
    );
    let mut output = [0xa5; 8];
    let decoded = {
        let local_data = data;
        let regions = [glyph.region()];
        let map = GlyphMap::atlas(5, 2, &regions).unwrap();
        let local = RawGlyphs::builder(map, SampleLayout::A4)
            .build(&local_data)
            .unwrap()
            .get(0)
            .unwrap();
        let plan = local.memory_plan(SurfaceRequirements::new()).unwrap();
        local.copy_into(&mut output, plan).unwrap()
    };
    assert_eq!(
        decoded.plane(0).unwrap().row(0).unwrap(),
        Some(&[0x23, 0x40][..])
    );
    assert_eq!(
        decoded.plane(0).unwrap().row(1).unwrap(),
        Some(&[0x78, 0x90][..])
    );
    assert_eq!(&output[4..], &[0xa5; 4]);
}

#[test]
fn exact_spans_and_shared_memory_validation_reject_invalid_storage() {
    let map = GlyphMap::glyph_major(1, 1, 2).unwrap();
    for layout in [
        SampleLayout::I4,
        SampleLayout::RGBA8888,
        SampleLayout::NV12,
        SampleLayout::new(0xefff),
    ] {
        assert_eq!(
            RawGlyphs::builder(map, layout).build(&[]).unwrap_err(),
            GlyphStorageError::UnsupportedLayout(layout)
        );
    }
    for actual in [0, 1, 3] {
        assert_eq!(
            RawGlyphs::builder(map, SampleLayout::A8)
                .build(&[0; 3][..actual])
                .unwrap_err(),
            GlyphStorageError::DataSizeMismatch {
                expected: 2,
                actual
            }
        );
    }
    let plane = SampleLayout::A8.plane_geometry(1, 1, 0).unwrap();
    let memory = PlaneMemoryLayout::builder(plane)
        .with_alignment(64)
        .build()
        .unwrap();
    assert_eq!(
        RawGlyphs::builder(map, SampleLayout::A8)
            .with_memory_layout(memory)
            .build(&[0; 64])
            .unwrap_err(),
        GlyphStorageError::DataSizeMismatch {
            expected: 65,
            actual: 64
        }
    );
    let wrong = GlyphMap::glyph_major(2, 1, 2).unwrap();
    assert!(matches!(
        RawGlyphs::builder(wrong, SampleLayout::A8)
            .with_memory_layout(memory)
            .build(&[]),
        Err(GlyphStorageError::Memory(_))
    ));
    let large = GlyphMap::glyph_major(1, 1, u32::MAX as usize).unwrap();
    assert_eq!(
        RawGlyphs::builder(large, SampleLayout::A8)
            .with_memory_layout(memory)
            .build(&[])
            .unwrap_err(),
        GlyphStorageError::SizeOverflow
    );
    let huge = GlyphMap::glyph_major(u32::MAX, 1, 1).unwrap();
    let memory =
        PlaneMemoryLayout::builder(SampleLayout::A8.plane_geometry(u32::MAX, 1, 0).unwrap())
            .with_alignment(64)
            .build()
            .unwrap();
    assert_eq!(
        RawGlyphs::builder(huge, SampleLayout::A8)
            .with_memory_layout(memory)
            .build(&[])
            .unwrap_err(),
        GlyphStorageError::DataSizeMismatch {
            expected: u32::MAX,
            actual: 0
        }
    );
}

#[test]
fn actual_address_unknown_flags_and_output_errors_are_independent() {
    let data = Buffer([0x7f; 1024]);
    let map = GlyphMap::glyph_major(3, 1, 2).unwrap();
    let plane = SampleLayout::A8.plane_geometry(3, 1, 0).unwrap();
    let memory = PlaneMemoryLayout::builder(plane)
        .with_alignment(64)
        .build()
        .unwrap();
    let glyphs = RawGlyphs::builder(map, SampleLayout::A8)
        .with_memory_layout(memory)
        .build(&data.0[1..68])
        .unwrap();
    assert!(glyphs.file_address_is_aligned(0));
    assert!(!glyphs.data_addresses_are_aligned());
    let glyph = glyphs.get(1).unwrap();
    assert!(!glyph.storage().plane(0).unwrap().address_is_aligned());
    let plan = glyph
        .memory_plan(SurfaceRequirements::new().with_base_alignment(64))
        .unwrap();
    let mut output = Buffer([0xa5; 1024]);
    assert!(glyph.copy_into(&mut output.0[1..], plan).is_err());
    assert!(glyph.copy_into(&mut output.0[..2], plan).is_err());
    let other = glyph
        .storage()
        .surface()
        .region_plan(Region::new(0, 0, 2, 1).unwrap(), SurfaceRequirements::new())
        .unwrap();
    assert_eq!(
        glyph.copy_into(&mut output.0, other),
        Err(SurfaceCopyError::SurfaceMismatch)
    );
    assert_eq!(output.0, [0xa5; 1024]);
    glyph.copy_into(&mut output.0, plan).unwrap();
    assert_eq!(&output.0[..3], &[0x7f; 3]);
    let flagged = PlaneMemoryLayout::builder(plane)
        .with_flags(PlaneMemoryFlags::from_bits_retain(1))
        .build()
        .unwrap();
    let group = RawGlyphs::builder(map, SampleLayout::A8)
        .with_memory_layout(flagged)
        .build(&data.0[..6])
        .unwrap();
    let glyph = group.get(0).unwrap();
    assert!(matches!(
        glyph.storage().plane(0).unwrap().rows(),
        Err(PlaneAccessError::UnsupportedFlags(1))
    ));
    let before = output.0;
    assert!(matches!(
        glyph.copy_into(&mut output.0, plan),
        Err(SurfaceCopyError::UnsupportedPlaneFlags { index: 0, flags: 1 })
    ));
    assert_eq!(output.0, before);
}

#[test]
fn empty_cells_and_empty_atlas_regions_do_not_create_sample_work() {
    let map = GlyphMap::glyph_major(7, 9, 0).unwrap();
    let glyphs = RawGlyphs::builder(map, SampleLayout::A1)
        .build(&[])
        .unwrap();
    assert!(glyphs.is_empty());
    assert_eq!(glyphs.byte_len(), 0);
    assert_eq!(glyphs.get(0), None);
    assert!(glyphs.file_address_is_aligned(u32::MAX));
    assert!(glyphs.data_addresses_are_aligned());
    let regions = [Region::new(0, u32::MAX, 0, 0).unwrap()];
    let map = GlyphMap::atlas(0, u32::MAX, &regions).unwrap();
    let glyph = RawGlyphs::builder(map, SampleLayout::A8)
        .build(&[])
        .unwrap()
        .get(0)
        .unwrap();
    let plan = glyph.memory_plan(SurfaceRequirements::new()).unwrap();
    assert_eq!(plan.memory_plan().byte_len(), 0);
    let mut out = [0xa5; 4];
    glyph.copy_into(&mut out, plan).unwrap();
    assert_eq!(out, [0xa5; 4]);
    let map = GlyphMap::atlas(2, 2, &[]).unwrap();
    let glyphs = RawGlyphs::builder(map, SampleLayout::A8)
        .build(&[1; 4])
        .unwrap();
    assert!(glyphs.is_empty());
    assert_eq!(glyphs.byte_len(), 4);
}
