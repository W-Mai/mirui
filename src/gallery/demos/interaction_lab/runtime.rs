#[cfg(feature = "std")]
use super::composition::build_widgets;
#[cfg(any(feature = "std", test))]
use super::state::InteractionModel;
#[cfg(feature = "std")]
use crate::prelude::plugin::InputFeedbackPlugin;
#[cfg(any(feature = "std", test))]
use crate::prelude::*;
#[cfg(any(feature = "std", test))]
use crate::ui::UserState;

#[cfg(any(feature = "std", test))]
#[derive(Clone, Copy)]
pub(super) struct InteractionNodes {
    pub(super) error: Entity,
    pub(super) disabled: Entity,
    pub(super) drag: Entity,
}

#[cfg(any(feature = "std", test))]
fn sync_user_state(
    world: &mut World,
    entity: Entity,
    enabled: bool,
    state: fn() -> UserState,
    matches: fn(&UserState) -> bool,
) {
    if !world.is_alive(entity) {
        return;
    }
    let already = world.get::<UserState>(entity).is_some_and(matches);
    if already == enabled {
        return;
    }
    if enabled {
        world.insert(entity, state());
    } else {
        world.remove::<UserState>(entity);
    }
    world.invalidate(entity);
}

#[cfg(any(feature = "std", test))]
#[mirui_macros::system]
pub(super) fn sync_interaction_user_states(world: &mut World) {
    let (Some(state), Some(nodes)) = (
        world
            .resource::<InteractionModel>()
            .map(|model| model.state.get_untracked()),
        world.resource::<InteractionNodes>().copied(),
    ) else {
        return;
    };
    sync_user_state(
        world,
        nodes.error,
        state.errored,
        || UserState::Errored,
        |value| matches!(value, UserState::Errored),
    );
    sync_user_state(
        world,
        nodes.disabled,
        state.disabled,
        || UserState::Disabled,
        |value| matches!(value, UserState::Disabled),
    );
    crate::ui::set_position(
        world,
        nodes.drag,
        Fixed::from_int(24) + state.drag_x,
        Fixed::from_int(27) + state.drag_y,
    );
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    crate::gallery::showcase_theme::install(&mut app.world);
    if app.world.resource::<InteractionModel>().is_none() {
        app.world.insert_resource(InteractionModel::default());
    }
    app.add_plugin(InputFeedbackPlugin::new())
        .add_system(sync_interaction_user_states::system());
    app.compose(parent, build_widgets);
}
