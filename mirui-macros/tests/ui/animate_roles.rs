use mirui::anim::{Motion, Tween};
use mirui::app::App;
use mirui::ecs::{Component, Entity};
use mirui::types::Fixed;
use mirui::{animate, component, model};

#[model]
struct Game {
    value: Fixed,
}

#[model]
impl Game {
    fn set_value(&mut self, value: Fixed) {
        self.value = value;
    }
}

#[component]
struct Target {
    value: Fixed,
}

struct Marker;

impl Component for Marker {}

#[animate(component = AnimateTarget)]
#[doc = "A typed motion component."]
#[cfg_attr(all(), cfg(all()), doc = "Motion callback.", inline)]
fn apply_target(
    game: model!(Game),
    entity: entity!(),
    value: motion!(),
    target: component!(mut Target),
    marker: component!(Marker),
    commands: commands!(),
) {
    let _ = marker;
    target.value = value;
    game.set_value(value);
    commands.remove::<Marker>(entity);
}

#[animate(component = DisabledAnimation)]
#[cfg_attr(all(), cfg(any()), doc = "Disabled animation.", inline)]
fn disabled(target: component!(UnknownType), value: motion!()) {
    unresolved_when_enabled(target, value);
}

fn assert_component<T: Component>() {}

fn main() {
    assert_component::<AnimateTarget>();
    let mut app = App::headless(32, 32);
    let game = app.add_model(Game { value: Fixed::ZERO }).finish();
    let _ = AnimateTarget::system(game);
    let _ = AnimateTarget(Motion::from(Tween::ease_to(Fixed::ZERO, Fixed::ONE, 100)));
    let _: Option<Entity> = None;
}
