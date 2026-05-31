//! `?demo=<name>` selects which `gallery::demos::*::build` runs.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();

    let demo = read_demo_query().unwrap_or_else(|| "dsl".to_string());
    match demo.as_str() {
        "rounded" => gallery::run(
            "mirui - rounded + border",
            480,
            320,
            gallery::demos::rounded::build,
        ),
        "text" => gallery::run("mirui - text demo", 480, 320, gallery::demos::text::build),
        "components" => gallery::run(
            "mirui - components demo",
            480,
            320,
            gallery::demos::components::build,
        ),
        "transform" => gallery::run(
            "mirui - transform demo",
            480,
            320,
            gallery::demos::transform::build,
        ),
        "cover_flow" => {
            let (w, h) = gallery::demos::cover_flow::SIZE;
            gallery::run(
                "mirui - cover flow",
                w,
                h,
                gallery::demos::cover_flow::build,
            )
        }
        "nested_scroll" => {
            let (w, h) = gallery::demos::nested_scroll::SIZE;
            gallery::run(
                "mirui - nested scroll",
                w,
                h,
                gallery::demos::nested_scroll::build,
            )
        }
        // No `effect` route: the Canvas 2D renderer leaves
        // `read_target_region` / `modify_target_region` unimplemented.
        _ => gallery::run("mirui - DSL demo", 480, 320, gallery::demos::dsl::build),
    }
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
