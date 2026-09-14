use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use mirui::prelude::*;
use mirui::render::texture::{ColorFormat, Texture};
use mirui::render::{RenderError, Renderer};
use mirui::types::Viewport;
use mirui::ui::render_system;

pub fn capture<B, F>(
    app: &mut App<B, F>,
    root: Entity,
    viewport: Viewport,
    full: Rect,
) -> Result<Option<Texture<'static>>, RenderError>
where
    B: Surface,
    F: RendererFactory<B>,
{
    render_system::update_layout(&mut app.world, root, &viewport);
    let mut renderer = app.factory.make(&mut app.backend, &viewport);
    render_system::render(&app.world, root, &viewport, &mut renderer)?;
    renderer.prepare_readback(&full);
    renderer.sample_target_region(&full)
}

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
