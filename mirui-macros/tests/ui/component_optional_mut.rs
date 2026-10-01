use mirui::prelude::*;

struct Game;

#[component]
struct Board {
    game: model!(optional mut Game),
}

fn main() {}
