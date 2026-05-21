use mirui::app::App;
use mirui::components::assets::*;
use mirui::components::image::Image;
use mirui::layout::*;
use mirui::surface::sdl::SdlSurface;
use mirui::types::{Color, Dimension};
use mirui::widget::builder::WidgetBuilder;
use mirui_macros::ui;

extern crate alloc;

fn main() {
    let backend = SdlSurface::new("mirui - enchants demo", 480, 320);
    let mut app = App::new(backend);
    app.with_default_widgets();

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(480),
            height: Dimension::px(320),
            ..Default::default()
        })
        .id();

    // Use enchants to attach Image component via DSL!
    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        content (direction: FlexDirection::Column, grow: 1.0) {
            header (bg_color: Color::rgb(88, 166, 255), height: 40, text: "Enchants Demo") {}
            img_widget (
                position: Position::Absolute,
                left: 200,
                top: 150,
                width: 16,
                height: 16,
                image: Image::new(&IMG_THUMBS_UP)
            ) {}
        }
    };

    app.set_root(root);
    app.run();
}
