use mirui::{component, model, view};

const ROOT_OFFSET: u8 = 2;

trait Palette {
    type Theme: 'static;
}

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

    pub struct Marker;

    impl super::Palette for Marker {
        type Theme = mirui::ui::Theme;
    }

    const PRIORITY: u8 = 42;

    #[mirui::system]
    fn update() {}

    fn attach(_: &mut mirui::ecs::World, _: mirui::ecs::Entity) {}

    #[mirui::event]
    fn gesture(_tap: event!(mirui::input::event::gesture::Tap)) -> bool { false }

    fn local_offset() -> u8 {
        3
    }

    #[view(component = super::Tile, name = "Behavior", attach = self::attach,
        input = self::gesture::callback(), systems = [self::update::system()])]
    #[cfg_attr(all(), cfg(all()), doc = "A behavior-only widget.")]
    pub fn behavior<T: super::Palette>(
        state: model!(component.state, watch(value)),
        optional: model!(optional component.optional, watch(value)),
        theme: res!(<T as super::Palette>::Theme),
    ) where
        self::Marker: super::Palette,
    {
    }

    #[view(component = super::Tile, priority = self::PRIORITY, invalidation = Layout, attach = self::attach, input = self::gesture::callback(), systems = [self::update::system()])]
    #[allow(clippy::let_and_return)]
    #[cfg_attr(all(), cfg(all()), doc = "A typed paint callback.", inline)]
    pub fn paint<T: super::Palette, const OFFSET: u8>(
        tile: component!(),
        state: model!(component.state, watch(value)),
        optional: model!(optional component.optional, watch(value)),
        theme: res!(<T as super::Palette>::Theme),
        fallback: res!(optional <T as super::Palette>::Theme),
        entity: entity!(),
        paint: paint!(),
    ) where
        self::Marker: super::Palette,
    {
        let value = state.value + super::ROOT_OFFSET + self::local_offset() + OFFSET;
        let optional_value = optional.map(|state| state.value);
        let _: Option<&<T as super::Palette>::Theme> = fallback;
        let _ = (tile, value, optional_value, theme, entity, paint);
    }
}

fn main() {
    let _ = nested::paint::view::<nested::Marker, 1>();
    let _ = nested::behavior::view::<nested::Marker>();
}
