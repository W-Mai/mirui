use mirui::{component, model, view};

const ROOT_OFFSET: u8 = 2;

#[model]
struct State {
    #[observe]
    value: u8,
}

#[model]
impl State {}

#[component]
struct Tile {
    state: model!(State),
    optional: model!(optional State),
}

mod nested {
    use super::*;

    fn local_offset() -> u8 {
        3
    }

    #[view(component = Tile, invalidation = Layout)]
    #[allow(clippy::let_and_return)]
    #[cfg_attr(all(), inline)]
    pub fn paint(
        tile: component!(),
        state: model!(component.state, watch(value)),
        optional: model!(optional component.optional, watch(value)),
        theme: res!(mirui::ui::Theme),
        entity: entity!(),
        paint: paint!(),
    ) {
        let value = state.value + super::ROOT_OFFSET + self::local_offset();
        let optional_value = optional.map(|state| state.value);
        let _ = (tile, value, optional_value, theme, entity, paint);
    }
}

fn main() {
    let _ = nested::paint::view();
}
