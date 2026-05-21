use mirui::components::Image;
use mirui::components::assets::*;
use mirui::prelude::*;
use mirui::surface::sdl::SdlSurface;

extern crate alloc;

fn main() {
    let backend = SdlSurface::new("mirui - image demo", 480, 320);
    let mut app = App::new(backend);
    app.with_default_widgets();

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(480),
            height: Dimension::px(320),
            justify: JustifyContent::Center,
            align: AlignItems::Center,
            ..Default::default()
        })
        .id();

    let img_widget = WidgetBuilder::new(&mut app.world)
        .layout(LayoutStyle {
            width: Dimension::px(IMG_THUMBS_UP.width as i32),
            height: Dimension::px(IMG_THUMBS_UP.height as i32),
            ..Default::default()
        })
        .id();
    app.world.insert(img_widget, Image::new(&IMG_THUMBS_UP));

    // Add as child of root
    use mirui::widget::{Children, Parent};
    app.world.insert(img_widget, Parent(root));
    if let Some(children) = app.world.get_mut::<Children>(root) {
        children.0.push(img_widget);
    }

    app.set_root(root);
    app.run();
}
