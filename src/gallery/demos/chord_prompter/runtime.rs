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
    let fonts = crate::render::font::FontManager::new(
        crate::core::cache::MaxSize::Unbound,
        super::font::body(),
    );
    fonts.add_static(super::font::CHORD.cache_key(), super::font::chord());
    app.with_fonts(fonts);
    let session = app
        .add_model(Session::new(
            Score::parse(super::PRACTICE, ScoreLimits::DEVICE).expect("bundled practice score"),
        ))
        .finish();
    app.with_widget(strips::rhythm_strip::view());
    app.add_system(advance::system(session.clone()));
    app.compose(parent, composition::build_widgets(session.clone()));
    session
}

#[cfg(feature = "std")]
pub fn setup_app<B: Surface, F: RendererFactory<B>>(app: &mut App<B, F>, parent: Entity) {
    app.compose(parent, crate::gallery::demos::clock::ensure_clock());
    install(app, parent);
}
