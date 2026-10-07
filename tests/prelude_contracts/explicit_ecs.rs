#![deny(warnings)]

use mirui::ecs::World;
use mirui::prelude::*;

#[compose]
fn panel() {
    ui! {
        Column() {
            View() on Tap(|input| input.x > Fixed::ZERO)
        }
    };
}

fn main() {
    let mut world = World::new();
    let _ = world.spawn(Style::default());
    let mut app = App::headless(32, 32);
    let root = app.spawn_root().id();
    app.compose(root, panel);
}
