use mirui::prelude::*;

struct Game;

#[component]
enum Board {
    Ready,
    Running { game: model!(Game) },
}

fn main() {}
