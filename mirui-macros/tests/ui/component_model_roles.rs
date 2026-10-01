use mirui::core::model::BindType;
use mirui::prelude::*;

#[model]
struct Game {
    score: u8,
}

#[model]
impl Game {
    fn score(&self) -> u8 {
        self.score
    }
}

#[component]
#[derive(Clone)]
struct Board<M>
where
    M: BindType + 'static,
{
    game: model!(Game),
    selection: model!(optional Game),
    secondary: model!(M),
    label: u8,
}

#[component]
struct PlainTuple(u8);

#[component]
enum PlainState {
    Ready,
    Value(u8),
}

fn assert_component<T: Component>() {}

fn main() {
    assert_component::<Board<Game>>();
    assert_component::<PlainTuple>();
    assert_component::<PlainState>();

    let mut app = App::headless(16, 16);
    let game = app.add_model(Game { score: 0 });
    assert_eq!(game.score(), 0);
    let board = Board::<Game> {
        game: game.clone(),
        selection: Some(game.clone()),
        secondary: game,
        label: 7,
    };
    let _copy = board.clone();
    let _ = PlainTuple(1);
    let _ = PlainState::Ready;
    let _ = PlainState::Value(2);
}
