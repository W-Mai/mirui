#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;

gallery::register_demos! {
    ("orbit_console",        "Orbit Console",        "Showcase",    orbit_console,        1024, 640),
    ("layout_lab",           "Layout Lab",           "Showcase",    layout_lab,           1024, 720),
    ("typography_lab",       "Typography Lab",       "Showcase",    typography_lab,       1024, 720),
    ("curve_text",           "Kinetic Type",         "Showcase",    curve_text,            960, 540),
    ("curve_text_compact",   "Curve Text Compact",   "Showcase",    curve_text_compact,    128, 128, false),
    ("interaction_lab",      "Interaction Lab",      "Showcase",    interaction_lab,      1024, 720),
    ("kinetic_console",      "Kinetic Console",      "Showcase",    kinetic_console,      128, 128, false),

    ("signal_scope",         "Signal Scope",         "Product",     signal_scope,         800, 480),

    ("marble_play",          "Marble Play",          "Play",        marble_play,          480, 320),

    ("niche",                "niche slots (@name)",  "Basics",      niche,                480, 320),
    ("i18n",                 "i18n locale toggle",   "Basics",      i18n,                 480, 320),

    ("animation",            "tween + ping pong",    "Animation",   animation,            320, 180),
    ("three_body",           "three body",           "Animation",   three_body,           480, 320),
    ("life",                 "game of life",         "Animation",   life,                 640, 640),
    ("life_compact",         "life compact",          "Animation",   life_compact,         128, 128, false),
    ("particles",            "particles",            "Animation",   particles,            480, 320),
    ("butterfly",            "butterfly",            "Animation",   butterfly,            480, 480),
    ("shapes",               "shapes",               "Animation",   shapes,               480, 480),
    ("subpixel",             "subpixel motion",      "Animation",   subpixel,             480, 320),
    ("spatial_anim",         "spatial anim",         "Animation",   spatial_anim,         400, 300),
    ("transform",            "transform",            "Animation",   transform,            480, 320),
    ("image_flip",           "image flip 3d",        "Animation",   image_flip,           480, 320),
    ("flip_card",            "flip card",            "Animation",   flip_card,            480, 320),
    ("book_flip",            "book flip",            "Animation",   book_flip,            640, 360),

    ("effect_panels",        "effect panels",        "Effects",     effect_panels,        360, 560),
    ("effect_glass",         "effect glass",         "Effects",     effect_glass,         128, 128, false),
    ("offscreen",            "offscreen render",     "Effects",     offscreen,            360, 360),
    ("offscreen_modal",      "offscreen modal",      "Effects",     offscreen_modal,      360, 360),
    ("custom_view",          "custom view (Diamond)","Effects",     custom_view,          480, 200),
    ("vector_mandala",       "vector mandala",       "Effects",     vector_mandala,       512, 512),
    ("icon",                 "icon set",             "Effects",     icon,                 540, 320),
    ("composite",            "blit composite modes", "Effects",     composite,            720, 360),
    ("gradient",             "gradient paint",       "Effects",     gradient,             480, 320),
    ("stroke_styles",        "stroke styles",        "Effects",     stroke_styles,        480, 360),
    ("clip_path",            "clip path",            "Effects",     clip_path,            320, 320),
    ("fill_rules",           "fill rules",           "Effects",     fill_rules,           400, 240),
    ("blur_filter",          "blur filter",          "Effects",     blur_filter,          480, 240),
    ("render_showcase",      "render showcase",      "Effects",     render_showcase,      640, 660),

    ("pinch_rotate",         "pinch + rotate",       "Interaction", pinch_rotate,         480, 360),

    ("state_counter",        "reactive counter",     "State",       state_counter,        360, 240),
    ("state_computed",       "reactive computed",    "State",       state_computed,       360, 240),
    ("state_effect",         "reactive effect",      "State",       state_effect,         360, 260),
    ("state_form",           "reactive form",        "State",       state_form,           360, 280),
    ("state_todo",           "reactive todo",        "State",       state_todo,           320, 280),
    ("state_show",           "reactive if / match",  "State",       state_show,           320, 240),
    ("state_list",           "reactive walk list",   "State",       state_list,           320, 320),
    ("state_keyed",          "keyed walk reorder",   "State",       state_keyed,          320, 320),
    ("persistence_counter",  "persistence counter",  "State",       persistence_counter,  320, 240),

    ("scroll",               "scroll",               "Scroll",      scroll,               480, 320),
    ("nested_scroll",        "nested scroll",        "Scroll",      nested_scroll,        480, 400),
    ("lazy_list",            "lazy list",            "Scroll",      lazy_list,            320, 320),
    ("cover_flow",           "cover flow",           "Scroll",      cover_flow,           640, 360),

    ("slider_value_changed", "slider valueChanged",  "Components",  slider_value_changed, 720, 320),
    ("tabbar",               "tabbar",               "Components",  tabbar,               480, 320),
    ("text_input",           "text input",           "Components",  text_input,           480, 200),
    ("theme_swap",           "theme swap",           "Components",  theme_swap,           480, 320),
    ("widgets",              "widgets",              "Components",  widgets,              512, 512),
    ("widgets_compact",      "widgets compact",      "Components",  widgets_compact,      128, 128, false),
    ("builder_form",         "builder API (no DSL)", "Components",  builder_form,         320, 200),
}

extern crate alloc;

use alloc::rc::Rc;
use core::cell::RefCell;

type WebApp = gallery::mirui::app::App<gallery::ActiveSurface, gallery::ActiveFactory>;
const DEFAULT_DEMO: &str = "orbit_console";

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
    let slug = read_demo_query().unwrap_or_else(|| DEFAULT_DEMO.to_string());
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
        out.push_str(&alloc::format!(
            "<a href=\"?demo={slug}\" data-demo=\"{slug}\" data-w=\"{w}\" data-h=\"{h}\" data-upscale=\"{upscale}\" data-theme-dark=\"{dark_theme}\" data-theme-light=\"{light_theme}\">{label}</a>",
            slug = d.slug,
            label = d.label,
            w = d.width,
            h = d.height,
            upscale = d.allow_upscale,
        ));
    }
    if !prev_cat.is_empty() {
        out.push_str("</div>");
    }
    out
}
