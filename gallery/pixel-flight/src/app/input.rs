use crate::{gameplay::FlightModel, presentation::stage};
use core::cell::Cell;
use mirui::core::model::ModelHandle;
use mirui::ecs::DeltaTimeMs;
use mirui::input::event::{
    PointerCursor,
    input::Key,
    lifecycle::{HostFocusChanged, HostVisibilityChanged},
    press::{PressCancelReason, PressCancelled, Pressed, Released},
};
use mirui::ui::ComputedRect;

#[mirui::component]
pub(crate) struct FlightViewState {
    pub model: model!(FlightModel),
    pub(crate) observed_members: Cell<u8>,
}
#[mirui::view(component = FlightViewState, name = "Flight controls",systems = [animate::system(), stage::sync::system()])]
pub(super) fn flight_view_state() {}

# [mirui :: system (order = Animation)]
fn animate(
    stages: query!(FlightViewState, ComputedRect),
    cursor: res!(PointerCursor),
    delta: res!(DeltaTimeMs),
) {
    for (stage, rect) in stages {
        let rect = rect.0;
        let pointer = (cursor.present
            && cursor.x >= rect.x
            && cursor.y >= rect.y
            && cursor.x < rect.x + rect.w
            && cursor.y < rect.y + rect.h)
            .then(|| [(cursor.x - rect.x), (cursor.y - rect.y)]);
        stage
            .model
            .advance_viewport(delta.0.into(), [rect.w, rect.h], pointer);
    }
}
#[mirui::event]
pub(super) fn key(event: event!(Key), model: model!(FlightModel)) -> bool {
    let Some(physical) = event.physical else {
        return false;
    };
    let mut handled = false;
    model.update(|flight| {
        handled = flight.handle_key(physical, event.pressed, event.repeat == Some(true));
    });
    handled
}
#[mirui::event]
pub(super) fn focus(event: event!(HostFocusChanged), model: model!(FlightModel)) {
    model.set_focus(event.focused);
}
#[mirui::event]
pub(super) fn visibility(event: event!(HostVisibilityChanged), model: model!(FlightModel)) {
    model.set_visible(event.visible);
}
#[mirui::event]
pub(super) fn press(_event: event!(Pressed), model: model!(FlightModel)) {
    model.press();
}
#[mirui::event]
pub(super) fn release(_event: event!(Released), model: model!(FlightModel)) {
    model.release(false);
}
#[mirui::event]
pub(super) fn cancel(event: event!(PressCancelled), model: model!(FlightModel)) {
    if event.reason != PressCancelReason::Gesture {
        model.release(true);
    }
}
