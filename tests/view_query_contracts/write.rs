use mirui::{component, view};

#[component]
struct Tile;

#[view(component = Tile)]
fn paint(rows: query!(mut Tile)) {
    let _ = rows;
}

fn main() {}
