//! Hello-widget demo for the `web-canvas` backend. The `<canvas>`
//! element is matched by `id = "mirui"` in `index.html`; the Rust
//! side wraps it in `WebCanvasSurface` and drives `App::tick` from
//! `requestAnimationFrame`.

#![cfg(target_arch = "wasm32")]

use mirui::draw::web_canvas::WebCanvasRendererFactory;
use mirui::prelude::*;
use mirui::surface::web_canvas::WebCanvasSurface;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();

    let canvas = web_sys::window()
        .expect("window")
        .document()
        .expect("document")
        .get_element_by_id("mirui")
        .expect("canvas element with id=\"mirui\"")
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .expect("element is not <canvas>");

    let surface = WebCanvasSurface::new(canvas);
    let factory = WebCanvasRendererFactory::new();
    let mut app = App::with_factory(surface, factory);
    app.with_default_widgets().with_default_systems();

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(ColorToken::Surface)
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

        column (direction: FlexDirection::Column, grow: 1.0) {
            header (
                bg_color: ColorToken::Primary,
                text_color: ColorToken::OnPrimary,
                height: 48,
                text: "hello mirui — web-canvas",
                border_radius: 8
            ) {}
            content (bg_color: ColorToken::SurfaceVariant, grow: 1.0) {}
            footer (height: 32, text: "ECS + Canvas 2D") {}
        }
    };

    app.set_root(root);
    app.into_runner().start_animation_frame();
}
