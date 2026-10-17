extern crate alloc;

pub use mirui::ui;
#[path = "../src/surface/host_clipboard/windows/bitmap.rs"]
mod bitmap;

#[path = "../src/surface/host_clipboard/windows/files.rs"]
mod files;

#[path = "../src/surface/host_clipboard/windows/html.rs"]
mod html;

#[path = "../src/surface/host_clipboard/windows/unicode.rs"]
mod unicode;
