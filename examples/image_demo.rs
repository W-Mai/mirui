use alloc::vec::Vec;
use mirui::app::App;
use mirui::backend::sdl::SdlBackend;
use mirui::components::assets::*;
use mirui::components::image::Image;
use mirui::layout::*;
use mirui::types::Color;
use mirui::widget::builder::WidgetBuilder;

extern crate alloc;

fn main() {
    let backend = SdlBackend::new("mirui - image demo", 480, 320);
    let mut app = App::new(backend);

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Some(480),
            height: Some(320),
            justify: JustifyContent::Center,
            align: AlignItems::Center,
            ..Default::default()
        })
        .id();

    let img_widget = WidgetBuilder::new(&mut app.world)
        .layout(LayoutStyle {
            width: Some(IMG_THUMBS_UP_WIDTH),
            height: Some(IMG_THUMBS_UP_HEIGHT),
            ..Default::default()
        })
        .id();
    app.world.insert(
        img_widget,
        Image::new(
            Vec::from(IMG_THUMBS_UP),
            IMG_THUMBS_UP_WIDTH,
            IMG_THUMBS_UP_HEIGHT,
        ),
    );

    // Add as child of root
    use mirui::widget::{Children, Parent};
    app.world.insert(img_widget, Parent(root));
    if let Some(children) = app.world.get_mut::<Children>(root) {
        children.0.push(img_widget);
    }

    app.set_root(root);
    app.run();
}
