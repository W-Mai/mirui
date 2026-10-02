use mirui::{component, model, view};

#[model]
struct State {
    #[observe]
    value: u8,
}

#[model]
impl State {}

#[component]
struct Tile {
    state: model!(State),
}

#[view(component = Tile)]
fn paint(
    first: model!(component.state, watch(value)),
    second: model!(component.state, watch(value)),
) {
    let _ = (first, second);
}

fn main() {}
