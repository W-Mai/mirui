mod composition;
mod editors;
mod keyboard;
mod model;

#[cfg(test)]
mod tests;

use crate::prelude::*;
pub use composition::build_widgets;

pub fn setup_app<B: Surface, F: RendererFactory<B>>(app: &mut App<B, F>, parent: Entity) {
    app.add_plugin(crate::gallery::play::font::PlayFontPlugin);
    #[cfg(feature = "std")]
    app.add_plugin(crate::app::plugins::StdInstantClockPlugin);
    app.with_widget(editors::search_view::view())
        .with_widget(editors::command_view::view())
        .with_widget(editors::shared_view::view());
    let model = app.add_model(model::EditorLab::default());
    app.with_input_method(InputMethod::software((
        keyboard::session_changed::callback(model.keyboard()),
        keyboard::editing_key::callback(model.keyboard()),
    )));
    app.compose(parent, build_widgets(model));
}

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::fixed(440, 760);
