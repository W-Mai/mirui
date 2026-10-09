use mirui::prelude::*;

#[component]
struct Consumer(
    #[cfg(any())] u8,
    #[depends_on(mirui::ui::WidgetTexture)] Entity,
);

fn main() {}
