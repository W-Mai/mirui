use mirui::{component, view};

#[component]
struct Tile;

#[view(component = Tile)]
fn missing_resource(resource: res!(MissingResource)) {}

#[view(component = Tile)]
fn missing_projection(model: model!(component.missing)) {}

fn main() {}
