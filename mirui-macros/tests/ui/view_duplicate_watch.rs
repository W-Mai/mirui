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

#[view]
fn paint(
    tile: component!(Tile),
    first: model!(tile.state, watch(value)),
    second: model!(tile.state, watch(value)),
) {
    let _ = (first, second);
}

fn main() {}
