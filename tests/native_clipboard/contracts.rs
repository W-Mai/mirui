use super::*;

fn one(mime: &'static str, bytes: &[u8]) -> ClipboardData {
    let mut item = ClipboardItem::with_capacity(1).unwrap();
    item.push(ClipboardFormat::new(mime, bytes.to_vec()))
        .unwrap();
    let mut data = ClipboardData::with_capacity(1, bytes.len()).unwrap();
    data.push(item).unwrap();
    data
}

pub(super) fn encoded_images_and_native_bitmap_import_share_the_png_representation() {
    objc2::rc::autoreleasepool(|_| {
        let board = NSPasteboard::pasteboardWithUniqueName();
        let png = include_bytes!("../../docs/assets/orbit-console.png");
        write_to(&board, &one("image/png", png)).unwrap();
        assert_eq!(
            read_from(&board).unwrap().items()[0].formats()[0].bytes(),
            png
        );

        let original = bitmap(&NSData::with_bytes(png)).unwrap();
        let tiff = original.TIFFRepresentation().unwrap();
        let native = NSPasteboardItem::new();
        assert!(native.setData_forType(&tiff, &NSString::from_str("public.tiff")));
        let items = NSArray::from_retained_slice(&[
            ProtocolObject::<dyn NSPasteboardWriting>::from_retained(native),
        ]);
        board.clearContents();
        assert!(board.writeObjects(&items));
        let data = read_from(&board).unwrap();
        let format = data.items()[0]
            .formats()
            .iter()
            .find(|format| format.mime() == "image/png")
            .unwrap();
        let restored = bitmap(&NSData::with_bytes(format.bytes())).unwrap();
        assert_eq!(restored.pixelsWide(), original.pixelsWide());
        assert_eq!(restored.pixelsHigh(), original.pixelsHigh());
        assert_eq!(
            write_to(&board, &one("image/png", b"not png")),
            Err(ClipboardError::InvalidData)
        );
        assert!(
            read_from(&board).unwrap().items()[0]
                .formats()
                .iter()
                .any(|format| format.mime() == "image/png")
        );
        board.clearContents();
    });
}

pub(super) fn uri_list_keeps_bytes_and_offers_native_file_paths() {
    objc2::rc::autoreleasepool(|_| {
        let board = NSPasteboard::pasteboardWithUniqueName();
        let bytes = b"# file\r\nfile:///tmp/clipboard%20one\r\n";
        let mut data = ClipboardData::with_capacity(2, 256).unwrap();
        for uri in [bytes.as_slice(), b"file:///tmp/two\r\n"] {
            let mut item = ClipboardItem::with_capacity(1).unwrap();
            item.push(ClipboardFormat::new("text/uri-list", uri.to_vec()))
                .unwrap();
            data.push(item).unwrap();
        }
        write_to(&board, &data).unwrap();
        assert_eq!(
            read_from(&board).unwrap().items()[0].formats()[0].bytes(),
            bytes
        );
        let classes = NSArray::from_slice(&[NSURL::class()]);
        // NSURL implements NSPasteboardReading; no options dictionary is supplied.
        let files = unsafe { board.readObjectsForClasses_options(&classes, None) }.unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(
            files
                .objectAtIndex(0)
                .downcast_ref::<NSURL>()
                .unwrap()
                .path()
                .unwrap()
                .to_string(),
            "/tmp/clipboard one"
        );
        let generation = board.changeCount();
        assert_eq!(
            write_to(
                &board,
                &one("text/uri-list", b"file:///tmp/one\r\nfile:///tmp/two\r\n")
            ),
            Err(ClipboardError::Unsupported)
        );
        assert_eq!(board.changeCount(), generation);
        write_to(&board, &one("text/uri-list", b"file:///tmp/a\r\n")).unwrap();
        let items = board.pasteboardItems().unwrap();
        assert_eq!(
            items
                .objectAtIndex(0)
                .stringForType(&NSString::from_str("public.file-url"))
                .unwrap()
                .to_string(),
            "file:///tmp/a"
        );
        assert_eq!(
            write_to(&board, &one("text/uri-list", b"relative/path")),
            Err(ClipboardError::InvalidData)
        );
        board.clearContents();
    });
}

pub(super) fn native_item_and_representation_boundaries_round_trip() {
    objc2::rc::autoreleasepool(|_| {
        let board = NSPasteboard::pasteboardWithUniqueName();
        let mut data = ClipboardData::with_capacity(2, 256).unwrap();
        let mut first = ClipboardItem::with_capacity(3).unwrap();
        first
            .push(ClipboardFormat::borrowed("text/plain", "中文\0🙂".as_bytes()))
            .unwrap();
        first
            .push(ClipboardFormat::borrowed("text/html", b"<b>bold</b>"))
            .unwrap();
        first
            .push(ClipboardFormat::borrowed(
                "application/x-mirui-test",
                &[0, 255, 1],
            ))
            .unwrap();
        data.push(first).unwrap();
        let mut second = ClipboardItem::with_capacity(1).unwrap();
        second
            .push(ClipboardFormat::borrowed("text/plain", b"second"))
            .unwrap();
        data.push(second).unwrap();
        write_to(&board, &data).unwrap();
        let restored = read_from(&board).unwrap();
        assert_eq!(restored.items().len(), 2);
        for (expected, actual) in data.items().iter().zip(restored.items()) {
            assert_eq!(expected.formats().len(), actual.formats().len());
            for format in expected.formats() {
                let actual = actual
                    .formats()
                    .iter()
                    .find(|actual| actual.mime() == format.mime())
                    .unwrap();
                assert_eq!(actual.bytes(), format.bytes());
            }
        }
        board.clearContents();
    });
}

pub(super) fn invalid_representation_does_not_replace_native_contents() {
    objc2::rc::autoreleasepool(|_| {
        let board = NSPasteboard::pasteboardWithUniqueName();
        write_to(&board, &ClipboardData::text("keep").unwrap()).unwrap();
        let generation = board.changeCount();
        static FORMATS: [ClipboardFormat; 2] = [
            ClipboardFormat::borrowed("text/plain", b"new"),
            ClipboardFormat::borrowed("text/html", &[255]),
        ];
        static ITEMS: [ClipboardItem; 1] = [ClipboardItem::borrowed(&FORMATS)];
        let data = ClipboardData::borrowed(&ITEMS);
        assert_eq!(write_to(&board, &data), Err(ClipboardError::InvalidData));
        assert_eq!(board.changeCount(), generation);
        assert_eq!(read_from(&board).unwrap().plain_text(), Ok("keep"));
        board.clearContents();
    });
}

pub(super) fn rtf_aliases_preserve_distinct_authored_representations() {
    objc2::rc::autoreleasepool(|_| {
        let board = NSPasteboard::pasteboardWithUniqueName();
        write_to(&board, &one("application/rtf", b"{\\rtf1 app}")).unwrap();
        let restored = read_from(&board).unwrap();
        assert!(
            restored.items()[0]
                .formats()
                .iter()
                .any(|format| format.mime() == "application/rtf"
                    && format.bytes() == b"{\\rtf1 app}")
        );
        let mut item = ClipboardItem::with_capacity(2).unwrap();
        item.push(ClipboardFormat::borrowed("text/rtf", b"{\\rtf1 text}"))
            .unwrap();
        item.push(ClipboardFormat::borrowed(
            "application/rtf",
            b"{\\rtf1 app}",
        ))
        .unwrap();
        let mut data = ClipboardData::with_capacity(1, 128).unwrap();
        data.push(item).unwrap();
        write_to(&board, &data).unwrap();
        let restored = read_from(&board).unwrap();
        assert!(restored.items()[0].formats().len() >= 2);
        for format in data.items()[0].formats() {
            assert!(
                restored.items()[0].formats().iter().any(
                    |actual| actual.mime() == format.mime() && actual.bytes() == format.bytes()
                )
            );
        }
        board.clearContents();
        assert!(matches!(
            read_from(&board),
            Err(ClipboardError::Unavailable)
        ));
    });
}
