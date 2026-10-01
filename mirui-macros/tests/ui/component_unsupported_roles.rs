use mirui::prelude::*;

#[component]
struct ResourceField {
    value: res!(u8),
}

#[component]
struct QueryField {
    value: query!(u8),
}

#[component]
struct CommandsField {
    value: commands!(),
}

#[component]
struct ComponentField {
    value: component!(u8),
}

#[component]
struct EntityField {
    value: entity!(),
}

#[component]
struct DrawField {
    value: draw!(),
}

#[component]
struct RectField {
    value: rect!(),
}

#[component]
struct ViewField {
    value: view!(),
}

#[component]
struct MotionField {
    value: motion!(),
}

fn main() {}
