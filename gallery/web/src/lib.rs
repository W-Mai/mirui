#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;

gallery::register_demos! {
    ("hello",                "hello",                "Basics",      hello,                480, 320),
    ("dsl",                  "dsl walk + if",        "Basics",      dsl,                  480, 320),
    ("text",                 "text labels",          "Basics",      text,                 480, 320),
    ("rounded",              "rounded + border",     "Basics",      rounded,              480, 320),
    ("absolute",             "absolute position",    "Basics",      absolute,             480, 320),
    ("walk",                 "walk + conditional",   "Basics",      walk,                 480, 320),
    ("image",                "image",                "Basics",      image,                480, 320),
    ("enchants",             "enchants",             "Basics",      enchants,             480, 320),
    ("app_demo",             "app demo",             "Basics",      app_demo,             480, 320),

    ("animation",            "tween + ping pong",    "Animation",   animation,            320, 180),
    ("three_body",           "three body",           "Animation",   three_body,           480, 320),
    ("life",                 "game of life",         "Animation",   life,                 640, 640),
    ("particles",            "particles",            "Animation",   particles,            480, 320),
    ("butterfly",            "butterfly",            "Animation",   butterfly,            480, 480),
    ("shapes",               "shapes",               "Animation",   shapes,               480, 480),
    ("subpixel",             "subpixel motion",      "Animation",   subpixel,             480, 320),
    ("spatial_anim",         "spatial anim",         "Animation",   spatial_anim,         400, 300),
    ("transform",            "transform",            "Animation",   transform,            480, 320),
    ("image_flip",           "image flip 3d",        "Animation",   image_flip,           480, 320),
    ("flip_card",            "flip card",            "Animation",   flip_card,            480, 320),
    ("book_flip",            "book flip",            "Animation",   book_flip,            640, 360),

    ("effect_panels",        "effect panels",        "Effects",     effect_panels,        480, 360),
    ("effect_glass",         "effect glass",         "Effects",     effect_glass,         128, 128),
    ("offscreen",            "offscreen render",     "Effects",     offscreen,            360, 360),
    ("offscreen_modal",      "offscreen modal",      "Effects",     offscreen_modal,      360, 360),
    ("custom_view",          "custom view (Diamond)","Effects",     custom_view,          480, 200),

    ("click",                "click colors",         "Interaction", click,                480, 320),
    ("toggle",               "toggle",               "Interaction", toggle,               640, 320),
    ("on_handlers",          "on handlers",          "Interaction", on_handlers,          640, 320),
    ("gesture",              "gesture",              "Interaction", gesture,              320, 240),
    ("hover_tour",           "hover tour",           "Interaction", hover_tour,           720, 360),
    ("input_feedback",       "input feedback",       "Interaction", input_feedback,       640, 360),
    ("interactive_states",   "interactive states",   "Interaction", interactive_states,   720, 420),
    ("disabled",             "disabled",             "Interaction", disabled,             480, 320),
    ("pinch_rotate",         "pinch + rotate",       "Interaction", pinch_rotate,         480, 360),

    ("scroll",               "scroll",               "Scroll",      scroll,               480, 320),
    ("nested_scroll",        "nested scroll",        "Scroll",      nested_scroll,        480, 400),
    ("lazy_list",            "lazy list",            "Scroll",      lazy_list,            320, 320),
    ("cover_flow",           "cover flow",           "Scroll",      cover_flow,           640, 360),

    ("slider_switch",        "slider + switch",      "Components",  slider_switch,        320, 200),
    ("slider_value_changed", "slider valueChanged",  "Components",  slider_value_changed, 720, 320),
    ("tabbar",               "tabbar",               "Components",  tabbar,               480, 320),
    ("tabbar_selection",     "tabbar selection",     "Components",  tabbar_selection,     640, 320),
    ("text_input",           "text input",           "Components",  text_input,           480, 200),
    ("components",           "components",           "Components",  components,           480, 320),
    ("theme_swap",           "theme swap",           "Components",  theme_swap,           480, 320),
    ("widgets",              "widgets",              "Components",  widgets,              512, 512),
    ("builder_form",         "builder API (no DSL)", "Components",  builder_form,         320, 200),
}

extern crate alloc;

use alloc::rc::Rc;
use core::cell::RefCell;

type WebApp = gallery::mirui::app::App<gallery::ActiveSurface, gallery::ActiveFactory>;

thread_local! {
    static APP: RefCell<Option<Rc<RefCell<Option<WebApp>>>>> = const { RefCell::new(None) };
    static DARK: core::cell::Cell<bool> = const { core::cell::Cell::new(true) };
}

fn current_theme() -> gallery::mirui::widget::Theme {
    if DARK.with(|d| d.get()) {
        gallery::mirui::widget::Theme::dark()
    } else {
        gallery::mirui::widget::Theme::light()
    }
}

fn build_app_for(demo: &gallery::DemoEntry, backend: gallery::ActiveSurface) -> WebApp {
    let mut app = gallery::assemble_app(backend, gallery::ActiveFactory::default());
    set_canvas_size(demo.width, demo.height);
    let root = {
        let mut setup = gallery::Setup { app: &mut app };
        (demo.setup)(&mut setup)
    };
    app.set_root(root);
    // Global toggle wins over any theme a demo set in its own setup_app.
    app.set_theme(current_theme());
    app
}

fn set_canvas_size(w: u16, h: u16) {
    if let Some(canvas) = web_sys::window()
        .and_then(|win| win.document())
        .and_then(|doc| doc.get_element_by_id("mirui"))
    {
        let style: web_sys::HtmlElement = wasm_bindgen::JsCast::unchecked_into(canvas);
        let _ = style
            .style()
            .set_property("width", &alloc::format!("{w}px"));
        let _ = style
            .style()
            .set_property("height", &alloc::format!("{h}px"));
    }
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
    let slug = read_demo_query().unwrap_or_else(|| "dsl".to_string());
    let demo =
        lookup_demo(&slug).unwrap_or_else(|| lookup_demo("dsl").expect("dsl demo registered"));
    let app = build_app_for(demo, gallery::grab_canvas());
    let cell = Rc::new(RefCell::new(Some(app)));
    APP.with(|slot| *slot.borrow_mut() = Some(cell.clone()));
    gallery::mirui::app::Runner::<gallery::ActiveSurface, gallery::ActiveFactory>::drive_animation_frame(cell);
}

#[wasm_bindgen]
pub fn set_theme(dark: bool) {
    DARK.with(|d| d.set(dark));
    let cell = APP.with(|slot| slot.borrow().clone());
    let Some(cell) = cell else { return };
    if let Some(app) = cell.borrow_mut().as_mut() {
        app.set_theme(current_theme());
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
        out.push_str(&alloc::format!(
            "<a href=\"?demo={slug}\" data-demo=\"{slug}\" data-w=\"{w}\" data-h=\"{h}\">{label}</a>",
            slug = d.slug,
            label = d.label,
            w = d.width,
            h = d.height,
        ));
    }
    if !prev_cat.is_empty() {
        out.push_str("</div>");
    }
    out
}
