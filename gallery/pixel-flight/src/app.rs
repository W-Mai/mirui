use crate::{
    Profile,
    gameplay::{FlightModel, FlightModelHandle},
};
use mirui::{
    input::event::{
        input::Key,
        lifecycle::{HostFocusChanged, HostVisibilityChanged},
    },
    prelude::*,
};
use mirui_sprite_stack::SpriteStackPlugin;

pub(crate) mod input;
mod view;
use input::{flight_view_state, focus, key, visibility};
use view::build_widgets;

#[doc = " Install the flight scene and its standard keyboard and pointer controls."]
pub fn setup_app<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
    parent: Entity,
) -> FlightModelHandle {
    setup_profile(app, parent, Profile::Desktop)
}

pub fn setup_compact_app<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
    parent: Entity,
) -> FlightModelHandle {
    setup_profile(app, parent, Profile::Compact)
}

fn setup_profile<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
    parent: Entity,
    profile: Profile,
) -> FlightModelHandle {
    if profile == Profile::Compact {
        app.require_text_layout_capacity(mirui::text::TextLayoutCapacity {
            layout_slots: 3,
            measurements: 8,
            lines: 6,
            runs: 6,
            glyphs: 32,
            carets: 40,
            workspace: mirui::text::WorkspaceCapacity {
                runs: 4,
                glyphs: 16,
                scratch_glyphs: 16,
                lines: 4,
            },
        })
        .expect("register compact flight hint layout");
    }
    let (width, height) = app.viewport().logical_size();
    let model = app
        .add_model(if profile == Profile::Desktop {
            FlightModel::new(width, height)
        } else {
            FlightModel::with_profile(width, height, profile)
        })
        .finish();
    app.add_plugin(SpriteStackPlugin);
    app.with_widget(flight_view_state::view());
    app.on_event(Key::EVENT, key::callback(model.clone()));
    app.on_event(HostFocusChanged::EVENT, focus::callback(model.clone()));
    app.on_event(
        HostVisibilityChanged::EVENT,
        visibility::callback(model.clone()),
    );
    app.compose(parent, build_widgets(model.clone(), profile));
    model
}
#[cfg(test)]
#[path = "../tests/unit/app.rs"]
mod tests;
