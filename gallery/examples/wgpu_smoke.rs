//! End-to-end smoke for the wgpu backend: `App` with the default
//! widget systems registered, three rectangles rendered through the
//! widget tree, exits when the window is closed. Visual check —
//! header / row / footer must show up, not just a blank window.

use mirui::components::Image;
use mirui::components::assets::IMG_THUMBS_UP;
use mirui::draw::wgpu_render::WgpuRendererFactory;
use mirui::prelude::*;
use mirui::surface::wgpu_surface::WgpuSurface;

fn main() {
    let backend = WgpuSurface::new("mirui wgpu smoke", 480, 320);
    let factory = WgpuRendererFactory::new();
    let mut app = App::with_factory(backend, factory);
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

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        content (direction: FlexDirection::Column, grow: 1.0) {
            header (
                bg_color: Color::rgb(88, 166, 255),
                height: 40,
                text: "Hello wgpu!",
                border_radius: 8
            ) {}
            row (direction: FlexDirection::Row, grow: 1.0) {
                btn1 (bg_color: Color::rgb(63, 185, 80), grow: 1.0, text: "OK", border_radius: 6) {}
                btn2 (
                    bg_color: Color::rgb(248, 81, 73),
                    grow: 1.0,
                    text: "Cancel",
                    border_radius: 6
                ) {}
                btn3 (
                    bg_color: Color::rgb(210, 168, 255),
                    grow: 1.0,
                    text: "Maybe",
                    border_radius: 6
                ) {}
            }
            thumb (
                width: IMG_THUMBS_UP.width as i32 * 4,
                height: IMG_THUMBS_UP.height as i32 * 4
            ) [
                Image::new(&IMG_THUMBS_UP),
            ] {}
            footer (
                bg_color: Color::rgb(50, 50, 70),
                height: 30,
                text: "wgpu backend"
            ) {}
        }
    };

    app.set_root(root);
    app.run();
}
