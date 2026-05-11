//! SDL GPU backend demo: drives the full `App + Backend + RendererFactory<B>`
//! pipeline against `SdlGpuBackend`. Exercises the GPU fast-paths
//! currently implemented — solid fills, 1-pixel borders, 1-pixel lines,
//! texture blits. Tessellated paths (rounded corners, thick strokes,
//! arcs, labels) are still `todo!()` so the scene sticks to these
//! primitives.

use mirui::app::App;
use mirui::backend::sdl_gpu::{SdlGpuBackend, SdlGpuFactory};
use mirui::components::assets::*;
use mirui::components::image::Image;
use mirui::layout::*;
use mirui::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};
use mirui::types::{Color, Dimension};
use mirui::widget::builder::WidgetBuilder;
use mirui::widget::{Children, Parent};

fn main() {
    let backend = SdlGpuBackend::new("mirui SDL GPU — primitives demo", 640, 480);
    let mut app = App::with_factory(backend, SdlGpuFactory::new());

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgba(30, 30, 46, 255))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::percent(100),
            height: Dimension::percent(100),
            justify: JustifyContent::Center,
            align: AlignItems::Center,
            ..Default::default()
        })
        .id();

    let solid = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgba(32, 160, 240, 255))
        .border(Color::rgba(240, 240, 255, 255), 1)
        .layout(LayoutStyle {
            width: Dimension::px(200),
            height: Dimension::px(80),
            ..Default::default()
        })
        .id();

    let rounded = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgba(240, 120, 60, 200))
        .border(Color::rgba(255, 255, 255, 255), 4)
        .border_radius(20)
        .layout(LayoutStyle {
            width: Dimension::px(200),
            height: Dimension::px(80),
            ..Default::default()
        })
        .id();

    let translucent = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgba(80, 240, 160, 128))
        .layout(LayoutStyle {
            width: Dimension::px(200),
            height: Dimension::px(80),
            ..Default::default()
        })
        .id();

    let label = WidgetBuilder::new(&mut app.world)
        .text("SDL GPU BACKEND")
        .text_color(Color::rgba(255, 255, 255, 255))
        .layout(LayoutStyle {
            width: Dimension::px(200),
            height: Dimension::px(20),
            ..Default::default()
        })
        .id();

    let img = WidgetBuilder::new(&mut app.world)
        .layout(LayoutStyle {
            width: Dimension::px(IMG_THUMBS_UP.width as i32),
            height: Dimension::px(IMG_THUMBS_UP.height as i32),
            ..Default::default()
        })
        .id();
    app.world.insert(img, Image::new(&IMG_THUMBS_UP));

    for child in [solid, rounded, translucent, label, img] {
        app.world.insert(child, Parent(root));
        if let Some(children) = app.world.get_mut::<Children>(root) {
            children.0.push(child);
        }
    }

    app.set_root(root);
    app.add_plugin(StdInstantClockPlugin)
        .add_plugin(FpsSummaryPlugin::default());

    use mirui::backend::{Backend, InputEvent};
    loop {
        let mut quit = false;
        loop {
            match app.backend.poll_event() {
                Some(InputEvent::Quit) => {
                    quit = true;
                    break;
                }
                Some(_) => {}
                None => break,
            }
        }
        if quit {
            break;
        }
        app.render();
    }
}
