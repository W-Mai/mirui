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
struct PaintField {
    value: paint!(),
}

#[component]
struct MotionField {
    value: motion!(),
}

fn main() {}
