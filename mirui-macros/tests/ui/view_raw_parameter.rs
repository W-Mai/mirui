use mirui::{component, view};

#[component]
struct Tile;

#[view(component = Tile)]
fn paint(tile: &Tile) {
    let _ = tile;
}

fn main() {}
