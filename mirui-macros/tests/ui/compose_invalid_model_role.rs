use mirui::compose;

struct Game;

#[compose]
fn panel(game: model!(optional mut Game)) {
    let _ = game;
}

fn main() {}
