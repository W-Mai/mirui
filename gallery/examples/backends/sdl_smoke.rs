//! Smoke test for the SDL backend using the shared widget tree.

use mirui::components::Image;
use mirui::components::Text;
use mirui::components::assets::IMG_THUMBS_UP;
use mirui::prelude::*;
use mirui::surface::sdl::SdlSurface;

fn main() {
    let backend = SdlSurface::new("mirui sdl smoke", 480, 320);
    let mut app = App::new(backend);
    app.with_default_widgets().with_default_systems();

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

        Column (grow: 1.0) {
            View (
                bg_color: Color::rgb(88, 166, 255),
                height: 40,
                border_radius: 8,
                border_color: Color::rgb(255, 255, 255)
            ) {
                Text ("Hello sdl!")
            }
            Row (grow: 1.0) {
                View (bg_color: Color::rgb(63, 185, 80), grow: 1.0, border_radius: 6) {
                    Text ("OK")
                }
                View (bg_color: Color::rgb(248, 81, 73), grow: 1.0, border_radius: 6) {
                    Text ("Cancel")
                }
                View (bg_color: Color::rgb(210, 168, 255), grow: 1.0, border_radius: 6) {
                    Text ("Maybe")
                }
            }
            Image (
                texture: &IMG_THUMBS_UP,
                width: IMG_THUMBS_UP.width as i32 * 4,
                height: IMG_THUMBS_UP.height as i32 * 4
            )
            View (
                bg_color: Color::rgb(50, 50, 70),
                height: 30
            ) {
                Text ("sdl backend")
            }
        }
    };

    app.set_root(root);
    app.run();
}
