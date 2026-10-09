use mirui::prelude::*;

#[component]
struct Consumer {
    #[retain_previous(mirui::ui::WidgetTexture)]
    source: Entity,
}

fn main() {}
