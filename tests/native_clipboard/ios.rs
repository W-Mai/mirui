use super::*;
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGImageAlphaInfo, CGImageByteOrderInfo,
};
use objc2_ui_kit::{UIGraphicsPopContext, UIGraphicsPushContext};

fn pixels(image: &UIImage, _mtm: MainThreadMarker) -> Vec<u8> {
    let size = unsafe { image.size() };
    let scale = unsafe { image.scale() };
    let width = (size.width * scale) as usize;
    let height = (size.height * scale) as usize;
    let mut bytes = vec![0; width * height * 4];
    let color_space = CGColorSpace::new_device_rgb().unwrap();
    // The bitmap context is destroyed before its exclusively borrowed storage is returned.
    let context = unsafe {
        CGBitmapContextCreate(
            bytes.as_mut_ptr().cast(),
            width,
            height,
            8,
            width * 4,
            Some(&color_space),
            CGImageByteOrderInfo::Order32Big.0 | CGImageAlphaInfo::PremultipliedLast.0,
        )
    }
    .unwrap();
    CGContext::translate_ctm(Some(&context), 0.0, height as f64);
    CGContext::scale_ctm(Some(&context), scale, -scale);
    unsafe { UIGraphicsPushContext(&context) };
    image.drawInRect(CGRect::new(CGPoint::ZERO, size));
    unsafe { UIGraphicsPopContext() };
    drop(context);
    bytes
}

fn single(mime: &'static str, bytes: &[u8]) -> ClipboardData {
    let mut item = ClipboardItem::with_capacity(1).unwrap();
    item.push(ClipboardFormat::new(mime, bytes.to_vec()))
        .unwrap();
    let mut data = ClipboardData::with_capacity(1, bytes.len()).unwrap();
    data.push(item).unwrap();
    data
}

fn assert_snapshot(actual: &ClipboardData, expected: &ClipboardData) {
    assert_eq!(actual.items().len(), expected.items().len());
    for (actual, expected) in actual.items().iter().zip(expected.items()) {
        for format in expected.formats() {
            assert!(
                actual
                    .formats()
                    .iter()
                    .any(|f| f.mime() == format.mime() && f.bytes() == format.bytes()),
                "missing {}",
                format.mime()
            );
        }
    }
}

pub(super) fn verify(mtm: MainThreadMarker) {
    let mut first = ClipboardItem::with_capacity(5).unwrap();
    for (mime, bytes) in [
        ("text/plain", "中文\0🙂".as_bytes()),
        ("text/html", b"<b>html</b>".as_slice()),
        ("text/rtf", b"{\\rtf1 text}"),
        ("application/rtf", b"{\\rtf1 app}"),
        ("application/x-mirui-test", &[0, 255, 0]),
    ] {
        first.push(ClipboardFormat::borrowed(mime, bytes)).unwrap();
    }
    let mut data = ClipboardData::with_capacity(2, 512).unwrap();
    data.push(first).unwrap();
    let mut second = ClipboardItem::with_capacity(1).unwrap();
    second
        .push(ClipboardFormat::borrowed(
            "text/uri-list",
            b"# file\r\nfile:///tmp/a%20b\r\n",
        ))
        .unwrap();
    data.push(second).unwrap();
    let prepared = prepare(&data, mtm).unwrap();
    assert_snapshot(&decode(&prepared, mtm).unwrap(), &data);
    let file = prepared
        .objectAtIndex(1)
        .objectForKey(&NSString::from_str("public.file-url"))
        .unwrap();
    assert_eq!(
        file.downcast_ref::<NSURL>()
            .unwrap()
            .path()
            .unwrap()
            .to_string(),
        "/tmp/a b"
    );

    let kind = NSString::from_str("public.utf16-external-plain-text");
    let bytes = NSData::with_bytes(&[0xff, 0xfe, b'A', 0, 0x3d, 0xd8, 0x42, 0xde])
        .into_super()
        .into_super();
    let native = NSDictionary::from_retained_objects(&[&*kind], &[bytes]);
    let items = NSArray::from_retained_slice(&[native]);
    assert_eq!(decode(&items, mtm).unwrap().plain_text(), Ok("A🙂"));

    let png_bytes = include_bytes!("../../docs/assets/orbit-console.png");
    let image_data = single("image/png", png_bytes);
    assert_snapshot(
        &decode(&prepare(&image_data, mtm).unwrap(), mtm).unwrap(),
        &image_data,
    );
    let original = UIImage::imageWithData(&NSData::with_bytes(png_bytes)).unwrap();
    let cg_image = unsafe { original.CGImage() }.unwrap();
    for orientation in [
        UIImageOrientation::Up,
        UIImageOrientation::Down,
        UIImageOrientation::Left,
        UIImageOrientation::Right,
        UIImageOrientation::UpMirrored,
        UIImageOrientation::DownMirrored,
        UIImageOrientation::LeftMirrored,
        UIImageOrientation::RightMirrored,
    ] {
        let rotated = UIImage::imageWithCGImage_scale_orientation(&cg_image, 1.0, orientation);
        let expected_size = unsafe { rotated.size() };
        let expected_pixels = pixels(&rotated, mtm);
        let native = NSDictionary::from_retained_objects(
            &[&*NSString::from_str("com.apple.uikit.image")],
            &[rotated.into_super().into_super()],
        );
        let data = decode(&NSArray::from_retained_slice(&[native]), mtm).unwrap();
        let bytes = data.items()[0].formats()[0].bytes();
        let decoded = UIImage::imageWithData(&NSData::with_bytes(bytes)).unwrap();
        assert_eq!(
            unsafe { decoded.imageOrientation() },
            UIImageOrientation::Up
        );
        assert_eq!(unsafe { decoded.size() }, expected_size);
        assert_eq!(
            pixels(&decoded, mtm),
            expected_pixels,
            "orientation {orientation:?}"
        );
    }

    let board = UIPasteboard::pasteboardWithUniqueName();
    write_to(&board, &data, mtm).unwrap();
    assert_snapshot(&read_from(&board, mtm).unwrap(), &data);
    for invalid in [
        single("image/png", b"invalid"),
        single("text/html", &[255]),
        single("text/uri-list", b"relative"),
        single("text/uri-list", b"file:///tmp/a\nfile:///tmp/b"),
    ] {
        assert!(write_to(&board, &invalid, mtm).is_err());
        assert_snapshot(&read_from(&board, mtm).unwrap(), &data);
    }
    write_to(&board, &image_data, mtm).unwrap();
    let restored = read_from(&board, mtm).unwrap();
    let bytes = restored.items()[0]
        .formats()
        .iter()
        .find(|format| format.mime() == "image/png")
        .unwrap()
        .bytes();
    let restored = UIImage::imageWithData(&NSData::with_bytes(bytes)).unwrap();
    assert_eq!(unsafe { restored.size() }, unsafe { original.size() });
    assert_eq!(pixels(&restored, mtm), pixels(&original, mtm));
    let rtf = single("text/rtf", b"{\\rtf1\\ansi\\ansicpg1252 caf\xe9}");
    write_to(&board, &rtf, mtm).unwrap();
    assert_snapshot(&read_from(&board, mtm).unwrap(), &rtf);
    // The named board is isolated from the simulator's general clipboard.
    let name = unsafe { board.name() };
    UIPasteboard::removePasteboardWithName(&name);
}
