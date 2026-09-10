use mirui::prelude::*;
use mirui::render::font::{Font, FontManager, FontToken};
use mirui::types::Transform;
use mirui::ui::widgets::{Text, WidgetTransform};

pub const WIDTH: u16 = 640;
pub const HEIGHT: u16 = 260;

const FONT_BYTES: &[u8] = include_bytes!("../../src/gallery/demos/assets/misans_ui.mirx");
const FONT: FontToken = FontToken::Custom("text_backend_parity");

pub fn build<B, F>(app: &mut App<B, F>) -> Entity
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.with_default_widgets().with_default_systems();
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

        Column (grow: 1.0, row_gap: 8) {
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
