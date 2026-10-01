use mirui::compose;

struct Game;

#[compose]
fn panel((game,): model!(Game)) {
    let _ = game;
}

fn main() {}
