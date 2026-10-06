use mirui::core::model::{BindType, BindingOwner, SharedValue};
use mirui::prelude::*;

#[derive(Clone)]
struct GameHandle;

impl SharedValue for GameHandle {
    fn binding_owner(&self) -> BindingOwner {
        BindingOwner::unbound()
    }
}

#[derive(Clone)]
struct Game;

impl BindType for Game {
    type Shared = GameHandle;
}

#[component]
struct ConditionalModel<T> {
    #[cfg(all())]
    game: model!(T),
    #[cfg_attr(all(), cfg(all()))]
    alternate: model!(optional T),
    #[cfg(any())]
    unavailable: model!(UnavailableModel),
    #[cfg_attr(all(), cfg(any()))]
    unavailable_alternate: model!(UnavailableAlternateModel),
}

#[component]
#[derive(Clone)]
struct ConditionalClone<T> {
    game: model!(Game),
    #[cfg(all())]
    payload: T,
    #[cfg_attr(all(), cfg(all()))]
    alternate: T,
    #[cfg(any())]
    unavailable: UnavailablePayload,
    #[cfg_attr(all(), cfg(any()))]
    unavailable_alternate: UnavailableAlternatePayload,
}

fn assert_component<T: Component>() {}

fn main() {
    assert_component::<ConditionalModel<Game>>();
    assert_component::<ConditionalClone<u8>>();

    let model = ConditionalModel::<Game> {
        game: GameHandle,
        alternate: Some(GameHandle),
    };
    let clone = ConditionalClone {
        game: GameHandle,
        payload: 1_u8,
        alternate: 2_u8,
    };
    let _ = (model, clone.clone());
}
