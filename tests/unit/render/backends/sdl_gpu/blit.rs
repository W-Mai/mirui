use super::*;

#[cfg(all(feature = "perf", feature = "std"))]
#[test]
fn upload_evidence_counts_packed_payload_for_padded_and_swapped_sources() {
    use crate::core::perf::{Capture, RecordData, SubmissionOutcome, TransferKind};
    let _serial = crate::core::perf::tests::SERIAL.lock().unwrap();
    let canvas = sdl2::surface::Surface::new(4, 4, PixelFormatEnum::RGBA32)
        .unwrap()
        .into_canvas()
        .unwrap();
    let creator = canvas.texture_creator();
    for (format, pitch, payload) in [
        (ColorFormat::RGBA8888, 16usize, 24),
        (ColorFormat::RGB565Swapped, 8usize, 12),
    ] {
        let bytes = [0x83; 32];
        let last = pitch
            + crate::render::PlaneLayout::packed(3, 1, format)
                .unwrap()
                .required_bytes();
        let source = Texture::from_ref(&bytes[..last], 3, 2, format)
            .unwrap()
            .with_stride(pitch)
            .unwrap();
        let pixels = source.read_pixels().unwrap();
        let mut dst = creator
            .create_texture_streaming(SdlGpuRenderer::<Box<[u8]>>::texture_format(format), 3, 2)
            .unwrap();
        let mut capture = Capture::new(8192).unwrap();
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                SdlGpuRenderer::<Box<[u8]>>::upload_texture(&mut dst, &pixels, None).unwrap();
            }),
            0
        );
        let mut calls = 0;
        let mut spans = 0;
        capture.drain_events(|info, records| {
            if info.track != 0 {
                assert_eq!(info.status.dropped, 0);
            }
            for record in records {
                if let RecordData::Transfer {
                    kind: TransferKind::SdlTextureUpload,
                    width,
                    height,
                    payload_bytes,
                    outcome: SubmissionOutcome::Issued,
                    ..
                } = record.data
                {
                    assert_eq!((width, height, payload_bytes), (3, 2, Some(payload)));
                    calls += 1;
                }
                if record
                    .span()
                    .is_some_and(|span| span.name == "sdl.texture.upload")
                {
                    spans += 1;
                }
            }
        });
        assert_eq!((calls, spans), (1, 1));
    }
}

#[test]
fn texture_upload_skips_source_and_destination_padding() {
    let bytes = [
        0xf8, 0x00, 0x07, 0xe0, 0x00, 0x1f, 0xaa, 0xbb, 0xff, 0xe0, 0x07, 0xff, 0xf8, 0x1f,
    ];
    let mut src =
        Texture::from_ref(&bytes, 3, 2, ColorFormat::RGB565Swapped).expect("valid pixel storage");
    src.stride = 8;
    let src = src.read_pixels().unwrap();
    let mut dst = [0xcc; 20];
    assert!(SdlGpuRenderer::<Box<[u8]>>::write_rows(&src, &mut dst, 10, true).is_ok());
    assert_eq!(
        &dst[..10],
        &[0x00, 0xf8, 0xe0, 0x07, 0x1f, 0x00, 0xcc, 0xcc, 0xcc, 0xcc]
    );
    assert_eq!(
        &dst[10..],
        &[0xe0, 0xff, 0xff, 0x07, 0x1f, 0xf8, 0xcc, 0xcc, 0xcc, 0xcc]
    );
    assert!(SdlGpuRenderer::<Box<[u8]>>::write_rows(&src, &mut dst[..15], 10, true).is_err());
    assert!(SdlGpuRenderer::<Box<[u8]>>::write_rows(&src, &mut dst, 10, false).is_ok());
    assert_eq!(&dst[..6], &bytes[..6]);
    assert_eq!(&dst[10..16], &bytes[8..14]);
}
