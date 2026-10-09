use mirui::prelude::*;
use mirui::ui::WidgetTexture;

#[component]
struct Consumer {
    #[depends_on(WidgetTexture)]
    source: u32,
}

fn main() {}
