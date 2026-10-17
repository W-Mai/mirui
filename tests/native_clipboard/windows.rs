use super::*;
use core::num::NonZeroIsize;
use raw_window_handle::{HandleError, Win32WindowHandle, WindowHandle};
use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, HWND_MESSAGE};

struct Window(HWND);

impl Window {
    fn new() -> Self {
        let window = unsafe {
            CreateWindowExW(
                0,
                windows_sys::w!("STATIC"),
                windows_sys::w!("Mirui clipboard contracts"),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                core::ptr::null_mut(),
                core::ptr::null_mut(),
                core::ptr::null(),
            )
        };
        assert!(!window.is_null());
        Self(window)
    }
}

impl HasWindowHandle for Window {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let raw = Win32WindowHandle::new(NonZeroIsize::new(self.0 as isize).unwrap());
        // This test owns the window until all borrowed handles have been released.
        Ok(unsafe { WindowHandle::borrow_raw(raw.into()) })
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        assert_ne!(unsafe { DestroyWindow(self.0) }, 0);
    }
}

fn data(formats: &[(&'static str, &[u8])]) -> ClipboardData {
    let mut item = ClipboardItem::with_capacity(formats.len()).unwrap();
    for &(mime, bytes) in formats {
        item.push(ClipboardFormat::new(mime, bytes.to_vec()))
            .unwrap();
    }
    let mut data =
        ClipboardData::with_capacity(1, formats.iter().map(|(_, b)| b.len()).sum()).unwrap();
    data.push(item).unwrap();
    data
}

fn bytes<'a>(data: &'a ClipboardData, mime: &str) -> &'a [u8] {
    assert_eq!(data.items().len(), 1);
    data.items()[0]
        .formats()
        .iter()
        .find(|f| f.mime() == mime)
        .unwrap()
        .bytes()
}

pub(super) fn image_and_file_conversions() {
    let mut dib = bitmap::bgra_storage(2, 2).unwrap();
    let colors = [0, 0, 255, 255, 0, 255, 0, 128, 255, 0, 0, 255, 0, 0, 0, 0];
    dib[124..].copy_from_slice(&colors);
    let png = images::dib_to_png(&dib).unwrap();
    assert_eq!(framing::png(&png).unwrap(), png);
    let restored = images::png_to_dib(&png).unwrap();
    assert_eq!(&restored[124..], &colors);
    assert!(images::png_to_dib(b"not png").is_err());

    let uri = "file:///C:/folder/a%2520b%20%E4%B8%AD.png\r\nfile://server/share/a%23b.txt\r\n";
    let native = uris::to_files(uri.as_bytes()).unwrap().unwrap();
    assert_eq!(uris::from_files(&native).unwrap(), uri.as_bytes());
    let paths = files::Paths::read(&native).unwrap();
    let decoded: Vec<_> = paths
        .iter()
        .map(|path| {
            String::from_utf8(
                unicode::decode_units(
                    path.chunks_exact(2)
                        .map(|w| u16::from_le_bytes([w[0], w[1]])),
                )
                .unwrap(),
            )
            .unwrap()
        })
        .collect();
    assert_eq!(
        decoded,
        [r"C:\folder\a%20b 中.png", r"\\server\share\a#b.txt"]
    );
    assert!(
        uris::to_files(b"https://example.org/a\r\n")
            .unwrap()
            .is_none()
    );
    assert!(uris::to_files(b"file:///C:/a?query").is_err());
    assert!(uris::to_files(b"relative/file").is_err());
    assert!(matches!(
        memory::Memory::copy(&[], true),
        Err(ClipboardError::Unsupported)
    ));
}

fn put_native(window: &Window, format: u32, bytes: &[u8]) {
    let memory = memory::Memory::copy(bytes, false).unwrap();
    let session = memory::Session::open(window.0).unwrap();
    session.clear().unwrap();
    let result = session.put(format, memory);
    session.finish(result).unwrap();
}

pub(super) fn system_exchange() {
    let window = Window::new();
    let mut dib = bitmap::bgra_storage(2, 1).unwrap();
    dib[124..].copy_from_slice(&[0, 0, 255, 255, 0, 255, 0, 128]);
    let png = images::dib_to_png(&dib).unwrap();
    let original = data(&[
        ("text/plain", "中文🙂\r\nnext".as_bytes()),
        ("text/html", "<b>中文</b>".as_bytes()),
        ("text/rtf", br"{\rtf1 text}"),
        ("application/rtf", br"{\rtf1 alternate}"),
        ("application/x-mirui-contract", &[0, 255, 1, 0, 0]),
        ("image/png", &png),
        (
            "text/uri-list",
            b"# file\r\nfile:///C:/folder/a%2520b.txt\r\n",
        ),
    ]);
    write(&window, &original).unwrap();
    let restored = read().unwrap();
    for format in original.items()[0].formats() {
        assert_eq!(bytes(&restored, format.mime()), format.bytes());
    }

    static FORMATS: [ClipboardFormat; 1] = [ClipboardFormat::borrowed("text/plain", b"new")];
    static ITEMS: [ClipboardItem; 2] = [
        ClipboardItem::borrowed(&FORMATS),
        ClipboardItem::borrowed(&FORMATS),
    ];
    assert_eq!(
        write(&window, &ClipboardData::borrowed(&ITEMS)),
        Err(ClipboardError::Unsupported)
    );
    assert_eq!(
        bytes(&read().unwrap(), "text/plain"),
        bytes(&original, "text/plain")
    );

    let invalid = data(&[("text/plain", b"replacement"), ("image/png", b"not png")]);
    assert!(write(&window, &invalid).is_err());
    assert_eq!(
        bytes(&read().unwrap(), "text/plain"),
        bytes(&original, "text/plain")
    );
    let invalid = data(&[("application/x-mirui-contract", &[])]);
    assert_eq!(write(&window, &invalid), Err(ClipboardError::Unsupported));
    assert_eq!(
        bytes(&read().unwrap(), "text/plain"),
        bytes(&original, "text/plain")
    );

    let session = memory::Session::open(window.0).unwrap();
    assert!(matches!(
        std::thread::spawn(read).join().unwrap(),
        Err(ClipboardError::Unavailable)
    ));
    session.finish(Ok(())).unwrap();

    let mut dib = bitmap::bgra_storage(2, 1).unwrap();
    dib[124..].copy_from_slice(&[0, 0, 255, 255, 0, 255, 0, 128]);
    put_native(&window, DIBV5, &dib);
    let restored = read().unwrap();
    assert_eq!(
        &images::png_to_dib(bytes(&restored, "image/png")).unwrap()[124..],
        &dib[124..]
    );

    let uris = b"file:///C:/folder/a%2520b.txt\r\nfile:///C:/folder/a%2520b.txt\r\n";
    put_native(&window, HDROP, &uris::to_files(uris).unwrap().unwrap());
    assert_eq!(bytes(&read().unwrap(), "text/uri-list"), uris);
    let html = b"<p>native fragment</p>";
    put_native(
        &window,
        register("HTML Format").unwrap(),
        &html::encode(html).unwrap(),
    );
    assert_eq!(bytes(&read().unwrap(), "text/html"), html);
    put_native(
        &window,
        register("Rich Text Format").unwrap(),
        b"{\\rtf1 native}\0\0",
    );
    assert_eq!(bytes(&read().unwrap(), "text/rtf"), br"{\rtf1 native}");
    write(
        &window,
        &ClipboardData::text("Mirui clipboard contracts passed").unwrap(),
    )
    .unwrap();
}
