use super::{Score, ScoreLimits, Session, SessionHandle, composition, strips};
use crate::prelude::*;

pub(super) fn report(result: Result<(), super::CommandError>) {
    if let Err(error) = result {
        crate::warn!("prompter control rejected: {error:?}");
    }
}

#[crate::system(order = Animation)]
fn advance(session: model!(Session), clock: res!(MonoClock)) {
    session.advance(clock.now_ns() / 1000);
}

#[crate::event]
pub(super) fn toggle(session: model!(Session), clock: res!(MonoClock)) {
    session.toggle(clock.now_ns() / 1000);
}

pub fn install<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
    parent: Entity,
) -> SessionHandle {
    app.with_fonts(crate::render::font::FontManager::new(
        crate::core::cache::MaxSize::Unbound,
        super::font::open(),
    ));
    let session = app
        .add_model(Session::new(
            Score::parse(super::PRACTICE, ScoreLimits::DEVICE).expect("bundled practice score"),
        ))
        .finish();
    let width = app.backend.viewport().logical_size().0;
    app.with_widget(strips::rhythm_strip::view());
    app.add_system(advance::system(session.clone()));
    app.compose(
        parent,
        composition::build_widgets(session.clone(), if width < 240 { 1 } else { 2 }),
    );
    session
}

#[cfg(feature = "std")]
pub fn setup_app<B: Surface, F: RendererFactory<B>>(app: &mut App<B, F>, parent: Entity) {
    app.compose(parent, crate::gallery::demos::clock::ensure_clock());
    install(app, parent);
}
