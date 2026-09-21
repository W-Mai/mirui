#![cfg(target_arch = "wasm32")]

use gallery::{DEMOS, lookup_demo};
use wasm_bindgen::prelude::*;

extern crate alloc;

use alloc::rc::Rc;
use core::cell::RefCell;

type WebApp = gallery::mirui::app::App<gallery::ActiveSurface, gallery::ActiveFactory>;
const DEFAULT_DEMO: &str = "orbit_console";
const HOME_DEMO: &str = "marble_play";

thread_local! {
    static APP: RefCell<Option<Rc<RefCell<Option<WebApp>>>>> = const { RefCell::new(None) };
    static DARK: core::cell::Cell<bool> = const { core::cell::Cell::new(true) };
}

fn theme_ids_for(demo: &gallery::DemoEntry) -> (&'static str, &'static str) {
    if matches!(demo.category, "Showcase" | "Product") {
        (
            gallery::mirui::gallery::showcase_theme::DARK_ID,
            gallery::mirui::gallery::showcase_theme::LIGHT_ID,
        )
    } else {
        ("dark", "light")
    }
}

fn selected_theme_id(demo: &gallery::DemoEntry) -> &'static str {
    let (dark, light) = theme_ids_for(demo);
    if DARK.with(|value| value.get()) {
        dark
    } else {
        light
    }
}

fn build_app_for(demo: &gallery::DemoEntry, backend: gallery::ActiveSurface) -> WebApp {
    let mut app = gallery::assemble_app(backend, gallery::configured_factory());
    app.add_plugin(gallery::mirui::app::plugins::StdInstantClockPlugin);
    if demo.slug == "marble_play" {
        app.add_plugin(gallery::mirui::app::plugins::AudioPlugin::new(
            gallery::mirui::audio::WebAudioSink::new(),
            gallery::mirui::gallery::demos::marble_play::audio_bank(),
        ));
    }
    let root = {
        let mut setup = gallery::Setup { app: &mut app };
        (demo.setup)(&mut setup)
    };
    app.set_root(root);
    app.set_theme(selected_theme_id(demo)).unwrap();
    app
}

fn build_backend_parity_app(backend: gallery::ActiveSurface) -> WebApp {
    let mut app = gallery::assemble_app(backend, gallery::configured_factory());
    gallery::backend_parity::build(&mut app);
    app
}

fn prefers_dark() -> bool {
    web_sys::window()
        .and_then(|w| w.match_media("(prefers-color-scheme: dark)").ok().flatten())
        .map(|m| m.matches())
        .unwrap_or(true)
}

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();

    DARK.with(|d| d.set(prefers_dark()));
    let slug = read_demo_query().unwrap_or_else(|| HOME_DEMO.to_string());
    let backend = gallery::grab_canvas();
    let app = if backend_parity_enabled() {
        build_backend_parity_app(backend)
    } else {
        let demo = lookup_demo(&slug)
            .unwrap_or_else(|| lookup_demo(DEFAULT_DEMO).expect("default demo registered"));
        build_app_for(demo, backend)
    };
    let cell = Rc::new(RefCell::new(Some(app)));
    APP.with(|slot| *slot.borrow_mut() = Some(cell.clone()));
    gallery::mirui::app::Runner::<gallery::ActiveSurface, gallery::ActiveFactory>::drive_animation_frame(cell);
}

#[wasm_bindgen]
pub fn default_demo_slug() -> String {
    DEFAULT_DEMO.to_string()
}

#[wasm_bindgen]
pub fn home_demo_slug() -> String {
    HOME_DEMO.to_string()
}

#[wasm_bindgen]
pub fn set_theme(theme_id: &str) {
    let theme_id = match theme_id {
        "dark" => "dark",
        "light" => "light",
        gallery::mirui::gallery::showcase_theme::DARK_ID => {
            gallery::mirui::gallery::showcase_theme::DARK_ID
        }
        gallery::mirui::gallery::showcase_theme::LIGHT_ID => {
            gallery::mirui::gallery::showcase_theme::LIGHT_ID
        }
        _ => return,
    };
    DARK.with(|dark| dark.set(matches!(theme_id, "dark" | "showcase-dark")));
    let cell = APP.with(|slot| slot.borrow().clone());
    let Some(cell) = cell else { return };
    if let Some(app) = cell.borrow_mut().as_mut() {
        let _ = app.set_theme(theme_id);
    }
}

#[wasm_bindgen]
pub fn switch_demo(slug: &str) {
    let Some(demo) = lookup_demo(slug) else {
        return;
    };
    let cell = APP.with(|slot| slot.borrow().clone());
    let Some(cell) = cell else { return };
    let old = cell.borrow_mut().take();
    let Some(old) = old else { return };
    let backend = old.into_backend();
    let app = build_app_for(demo, backend);
    *cell.borrow_mut() = Some(app);
}

#[wasm_bindgen]
pub fn set_canvas_logical_size(width: u16, height: u16) {
    let cell = APP.with(|slot| slot.borrow().clone());
    let Some(cell) = cell else { return };
    if let Some(app) = cell.borrow().as_ref() {
        let size = (width > 0 && height > 0).then_some((width, height));
        app.backend.set_logical_size(size);
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

fn backend_parity_enabled() -> bool {
    let Some(search) = web_sys::window().and_then(|window| window.location().search().ok()) else {
        return false;
    };
    search.trim_start_matches('?').split('&').any(|pair| {
        let mut parts = pair.splitn(2, '=');
        parts.next() == Some("backend_parity") && parts.next() == Some("1")
    })
}

#[wasm_bindgen]
pub fn demo_source(slug: &str) -> String {
    lookup_demo(slug)
        .map(|d| d.source.to_string())
        .unwrap_or_default()
}

#[wasm_bindgen]
pub fn demo_source_focus(slug: &str) -> String {
    lookup_demo(slug)
        .map(|d| gallery::extract_focus(d.source))
        .unwrap_or_default()
}

#[wasm_bindgen]
pub fn demo_catalog_json() -> String {
    let mut out = String::from("[");
    for (index, demo) in DEMOS.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let (dark_theme, light_theme) = theme_ids_for(demo);
        let bound = |value: Option<u16>| {
            value
                .map(|value| value.to_string())
                .unwrap_or_else(|| "null".to_string())
        };
        out.push_str(&alloc::format!(
            "{{\"slug\":\"{}\",\"label\":\"{}\",\"category\":\"{}\",\"minWidth\":{},\"minHeight\":{},\"maxWidth\":{},\"maxHeight\":{},\"darkTheme\":\"{}\",\"lightTheme\":\"{}\"}}",
            json_escape(demo.slug),
            json_escape(demo.label),
            json_escape(demo.category),
            bound(demo.size.min_width),
            bound(demo.size.min_height),
            bound(demo.size.max_width),
            bound(demo.size.max_height),
            json_escape(dark_theme),
            json_escape(light_theme),
        ));
    }
    out.push(']');
    out
}

fn json_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_control() => {
                use core::fmt::Write;
                let _ = write!(out, "\\u{:04x}", ch as u32);
            }
            ch => out.push(ch),
        }
    }
    out
}
