use mirui::prelude::draw::*;
use mirui::prelude::*;
use mirui::render::font::{Font, FontManager, FontToken};
use mirui::render::texture::{ColorFormat, Texture};
use mirui::types::Transform;
use mirui::ui::widgets::{Text, WidgetTransform};

pub const WIDTH: u16 = 640;
pub const HEIGHT: u16 = 390;

const FONT_BYTES: &[u8] = include_bytes!("../../src/gallery/demos/assets/misans_ui.mirx");
const FONT: FontToken = FontToken::Custom("text_backend_parity");
const RGBA: [u8; 24] = [
    255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255, 0, 255, 255, 255, 255, 0,
    255, 255,
];
const RGB565: [u8; 14] = [
    0x00, 0xf8, 0xe0, 0x07, 0x1f, 0x00, 0xaa, 0xbb, 0xe0, 0xff, 0xff, 0x07, 0x1f, 0xf8,
];
const RGB565_SWAPPED: [u8; 14] = [
    0xf8, 0x00, 0x07, 0xe0, 0x00, 0x1f, 0xaa, 0xbb, 0xff, 0xe0, 0x07, 0xff, 0xf8, 0x1f,
];

#[derive(Default)]
struct TextureParity;

fn sample_textures() -> [Texture<'static>; 3] {
    let rgba = Texture::from_static(&RGBA, 3, 2, ColorFormat::RGBA8888);
    let mut rgb565 = Texture::from_static(&RGB565, 3, 2, ColorFormat::RGB565);
    let mut swapped = Texture::from_static(&RGB565_SWAPPED, 3, 2, ColorFormat::RGB565Swapped);
    rgb565.stride = 8;
    swapped.stride = 8;
    [rgba, rgb565, swapped]
}

fn texture_parity_render(
    renderer: &mut dyn Renderer,
    _world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let textures = sample_textures();
    for (index, texture) in textures.iter().enumerate() {
        ctx.draw(
            renderer,
            &DrawCommand::Blit {
                pos: Point {
                    x: rect.x + Fixed::from_int(30 + index as i32 * 200),
                    y: rect.y,
                },
                size: Point {
                    x: Fixed::from_int(120),
                    y: Fixed::from_int(80),
                },
                transform: ctx.transform,
                quad: None,
                texture,
                opa: 255,
                radius: Fixed::ZERO,
                composite: CompositeMode::SourceOver,
            },
            ctx.clip,
        );
    }
}

pub fn build<B, F>(app: &mut App<B, F>) -> Entity
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.with_default_widgets().with_default_systems();
    app.with_widget(
        View::new("TextureParity", 60, texture_parity_render).with_filter::<TextureParity>(),
    );
    let font = Font::from_mirx(
        "MiSans UI",
        16,
        FONT_BYTES,
        &mirx::reader::PayloadLimits::HOST,
    )
    .expect("parse production UI font");
    app.world
        .resource::<FontManager>()
        .expect("FontManager")
        .add_static(FONT.cache_key(), font);

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(16, 18, 27))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(WIDTH.into()),
            height: Dimension::px(HEIGHT.into()),
            padding: Padding::all(Dimension::px(18)),
            ..Default::default()
        })
        .id();

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        Column (grow: 1.0, row_gap: 10) {
            Text (
                "14 px coverage · Ag AVATAR 0123",
                height: 30,
                font: FONT,
                font_size: 14,
                text_color: Color::rgb(146, 204, 255)
            )
            Text (
                "48 px SDF · Ag AVATAR",
                height: 68,
                font: FONT,
                font_size: 48,
                text_color: Color::rgb(255, 220, 140)
            )
            Text (
                id: "text_backend_affine",
                "36 px affine · mirui",
                height: 58,
                font: FONT,
                font_size: 36,
                text_color: Color::rgb(174, 246, 214)
            )
            Row (height: 30, column_gap: 20) {
                Text ("RGBA reference", width: 180, height: 30, font: FONT, font_size: 14)
                Text ("RGB565 · stride 8", width: 180, height: 30, font: FONT, font_size: 14)
                Text ("RGB565 swapped", width: 180, height: 30, font: FONT, font_size: 14)
            }
            TextureParity (height: 80)
        }
    };

    let affine = app
        .world
        .find_by_id("text_backend_affine")
        .expect("affine text id");
    app.world.insert(
        affine,
        WidgetTransform(
            Transform::translate(Fixed::from_int(8), Fixed::from_int(2))
                .compose(&Transform::rotate_deg(Fixed::from_int(-3))),
        ),
    );
    app.set_root(root);
    root
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padded_rgb565_samples_match_rgba_reference() {
        let [reference, normal, swapped] = sample_textures();
        for y in 0..2 {
            for x in 0..3 {
                let expected = reference.get_pixel(x, y);
                assert_eq!(normal.get_pixel(x, y), expected);
                assert_eq!(swapped.get_pixel(x, y), expected);
            }
        }
    }
}
