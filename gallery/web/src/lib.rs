#![cfg(target_arch = "wasm32")]

use gallery::mirui::app::App;
use gallery::mirui::ecs::Entity;
use gallery::mirui::widget::builder::WidgetBuilder;
use gallery::{ActiveFactory, ActiveSurface, Setup};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();

    let demo = read_demo_query().unwrap_or_else(|| "dsl".to_string());
    match demo.as_str() {
        "rounded" => gallery::run("mirui - rounded + border", 480, 320, |setup| {
            spawn_with(setup, |app, parent| {
                gallery::mirui::gallery::demos::rounded::setup_app(app, parent)
            })
        }),
        "text" => gallery::run("mirui - text demo", 480, 320, |setup| {
            spawn_with(setup, |app, parent| {
                gallery::mirui::gallery::demos::text::setup_app(app, parent)
            })
        }),
        "components" => gallery::run("mirui - components demo", 480, 320, |setup| {
            spawn_with(setup, |app, parent| {
                gallery::mirui::gallery::demos::components::setup_app(app, parent)
            })
        }),
        "transform" => gallery::run("mirui - transform demo", 480, 320, |setup| {
            spawn_with(setup, |app, parent| {
                gallery::mirui::gallery::demos::transform::setup_app(app, parent)
            })
        }),
        "cover_flow" => {
            let (w, h) = gallery::mirui::gallery::demos::cover_flow::DEFAULT_VIEW;
            gallery::run("mirui - cover flow", w, h, |setup| {
                spawn_with(setup, |app, parent| {
                    gallery::mirui::gallery::demos::cover_flow::setup_app(app, parent)
                })
            })
        }
        "nested_scroll" => gallery::run("mirui - nested scroll", 480, 400, |setup| {
            spawn_with(setup, |app, parent| {
                gallery::mirui::gallery::demos::nested_scroll::setup_app(app, parent)
            })
        }),
        _ => gallery::run("mirui - DSL demo", 480, 320, |setup| {
            spawn_with(setup, |app, parent| {
                gallery::mirui::gallery::demos::dsl::setup_app(app, parent)
            })
        }),
    }
}

fn spawn_with<F>(setup: &mut Setup<'_>, f: F) -> Entity
where
    F: FnOnce(&mut App<ActiveSurface, ActiveFactory>, Entity),
{
    let parent = WidgetBuilder::new(&mut setup.app.world).id();
    f(setup.app, parent);
    parent
}

fn read_demo_query() -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    let trimmed = search.trim_start_matches('?');
    for pair in trimmed.split('&') {
        let mut parts = pair.splitn(2, '=');
        if parts.next() == Some("demo") {
            if let Some(value) = parts.next() {
                return js_sys::decode_uri_component(value)
                    .ok()
                    .map(|s| s.as_string().unwrap_or_default());
            }
        }
    }
    None
}
