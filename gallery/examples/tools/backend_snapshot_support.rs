use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use mirui::render::texture::{ColorFormat, Texture};

pub fn write_png(path: &Path, texture: &Texture<'_>) {
    assert_eq!(texture.format, ColorFormat::RGBA8888);
    assert_eq!(texture.stride, usize::from(texture.width) * 4);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create output directory");
    }
    let file = BufWriter::new(File::create(path).expect("create snapshot"));
    let mut encoder = png::Encoder::new(file, texture.width.into(), texture.height.into());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("write PNG header");
    writer
        .write_image_data(texture.buf.as_slice())
        .expect("write PNG pixels");
}
