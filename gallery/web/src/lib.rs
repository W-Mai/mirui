#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;

gallery::register_demos! {
    ("orbit_console",        "Orbit Console",        "Showcase",    orbit_console),
    ("layout_lab",           "Layout Lab",           "Showcase",    layout_lab),
    ("typography_lab",       "Typography Lab",       "Showcase",    typography_lab),
    ("curve_text",           "Kinetic Type",         "Showcase",    curve_text),
    ("curve_text_compact",   "Curve Text Compact",   "Showcase",    curve_text_compact),
    ("interaction_lab",      "Interaction Lab",      "Showcase",    interaction_lab),
    ("kinetic_console",      "Kinetic Console",      "Showcase",    kinetic_console),

    ("signal_scope",         "Signal Scope",         "Product",     signal_scope),

    ("marble_play",          "Marble Play",          "Play",        marble_play),
    ("lumen_lab",            "Lumen Lab",            "Play",        lumen_lab),
    ("pixel_loom",           "Pixel Loom",           "Play",        pixel_loom),
    ("moss_study",           "Moss Study",           "Play",        moss_study),
    ("pocket_post",          "Pocket Post",          "Play",        pocket_post),
    ("logic_circuit",        "Logic Circuit",        "Play",        logic_circuit),
    ("module_factory",       "Module Factory",       "Play",        module_factory),
    ("orbital_mission",      "Orbital Mission",      "Play",        orbital_mission),
    ("tidal_atlas",          "Tidal Atlas",          "Play",        tidal_atlas),
    ("echo_walker",          "Echo Walker",          "Play",        echo_walker),
    ("twin_beacons",         "Twin Beacons",         "Play",        twin_beacons),
    ("folding_ark",          "Folding Ark",          "Play",        folding_ark),
    ("atlas_restoration",    "Atlas Restoration",    "Play",        atlas_restoration),

    ("niche",                "niche slots (@name)",  "Basics",      niche),
    ("i18n",                 "i18n locale toggle",   "Basics",      i18n),

    ("animation",            "tween + ping pong",    "Animation",   animation),
    ("three_body",           "three body",           "Animation",   three_body),
    ("life",                 "game of life",         "Animation",   life),
    ("life_compact",         "life compact",          "Animation",   life_compact),
    ("particles",            "particles",            "Animation",   particles),
    ("butterfly",            "butterfly",            "Animation",   butterfly),
    ("shapes",               "shapes",               "Animation",   shapes),
    ("subpixel",             "subpixel motion",      "Animation",   subpixel),
    ("spatial_anim",         "spatial anim",         "Animation",   spatial_anim),
    ("transform",            "transform",            "Animation",   transform),
    ("image_flip",           "image flip 3d",        "Animation",   image_flip),
    ("flip_card",            "flip card",            "Animation",   flip_card),
    ("book_flip",            "book flip",            "Animation",   book_flip),

    ("effect_panels",        "effect panels",        "Effects",     effect_panels),
    ("effect_glass",         "effect glass",         "Effects",     effect_glass),
    ("offscreen",            "offscreen render",     "Effects",     offscreen),
    ("offscreen_modal",      "offscreen modal",      "Effects",     offscreen_modal),
    ("custom_view",          "custom view (Diamond)","Effects",     custom_view),
    ("vector_mandala",       "vector mandala",       "Effects",     vector_mandala),
    ("icon",                 "icon set",             "Effects",     icon),
    ("composite",            "blit composite modes", "Effects",     composite),
    ("gradient",             "gradient paint",       "Effects",     gradient),
    ("stroke_styles",        "stroke styles",        "Effects",     stroke_styles),
    ("clip_path",            "clip path",            "Effects",     clip_path),
    ("fill_rules",           "fill rules",           "Effects",     fill_rules),
    ("blur_filter",          "blur filter",          "Effects",     blur_filter),
    ("render_showcase",      "render showcase",      "Effects",     render_showcase),

    ("pinch_rotate",         "pinch + rotate",       "Interaction", pinch_rotate),

    ("state_counter",        "reactive counter",     "State",       state_counter),
    ("state_computed",       "reactive computed",    "State",       state_computed),
    ("state_effect",         "reactive effect",      "State",       state_effect),
    ("state_form",           "reactive form",        "State",       state_form),
    ("state_todo",           "reactive todo",        "State",       state_todo),
    ("state_show",           "reactive if / match",  "State",       state_show),
    ("state_list",           "reactive walk list",   "State",       state_list),
    ("state_keyed",          "keyed walk reorder",   "State",       state_keyed),
    ("persistence_counter",  "persistence counter",  "State",       persistence_counter),

    ("scroll",               "scroll",               "Scroll",      scroll),
    ("nested_scroll",        "nested scroll",        "Scroll",      nested_scroll),
    ("lazy_list",            "lazy list",            "Scroll",      lazy_list),
    ("cover_flow",           "cover flow",           "Scroll",      cover_flow),

    ("slider_value_changed", "slider valueChanged",  "Components",  slider_value_changed),
    ("tabbar",               "tabbar",               "Components",  tabbar),
    ("text_input",           "text input",           "Components",  text_input),
    ("theme_swap",           "theme swap",           "Components",  theme_swap),
    ("widgets",              "widgets",              "Components",  widgets),
    ("widgets_compact",      "widgets compact",      "Components",  widgets_compact),
    ("builder_form",         "builder API (no DSL)", "Components",  builder_form),
}

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
pub fn nav_html() -> String {
    let mut out = String::new();
    let mut prev_cat = "";
    for d in DEMOS {
        if d.category != prev_cat {
            if !prev_cat.is_empty() {
                out.push_str("</div>");
            }
            out.push_str(&alloc::format!(
                "<h3 class=\"cat\">{}</h3><div class=\"cat-row\">",
                d.category
            ));
            prev_cat = d.category;
        }
        let (dark_theme, light_theme) = theme_ids_for(d);
        gallery::push_demo_nav_link(&mut out, d, dark_theme, light_theme);
    }
    if !prev_cat.is_empty() {
        out.push_str("</div>");
    }
    out
}
