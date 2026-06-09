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
}

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();

    let slug = read_demo_query().unwrap_or_else(|| "dsl".to_string());
    let demo =
        lookup_demo(&slug).unwrap_or_else(|| lookup_demo("dsl").expect("dsl demo registered"));
    let title = alloc::format!("mirui — {}", demo.label);
    gallery::run(&title, demo.width, demo.height, demo.setup);
}

extern crate alloc;

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
