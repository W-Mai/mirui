extern crate alloc;

mod audio;
mod board;
mod inspector;
mod scenes;
mod settings;
mod shell;
mod style;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::ecs::DeltaTimeMs;
use crate::gallery::play::marble::MarbleModel;
#[cfg(feature = "audio")]
use crate::gallery::play::marble::MarbleSound;
use crate::prelude::*;
use crate::text::{TextLayoutCapacity, WorkspaceCapacity};

pub const VIEWPORT: (u16, u16) = (480, 320);

const TEXT_LAYOUT_CAPACITY: TextLayoutCapacity = TextLayoutCapacity {
    layout_slots: 33,
    measurements: 33,
    lines: 32,
    runs: 32,
    glyphs: 384,
    carets: 448,
    workspace: WorkspaceCapacity {
        runs: 16,
        glyphs: 32,
        scratch_glyphs: 32,
        lines: 8,
    },
};

#[mirui_macros::system(order = ANIMATION, bind(model))]
fn marble_tick_system(model: &MarbleModel, delta: DeltaTimeMs) {
    model.advance_ms(delta.0);
}

#[cfg(feature = "audio")]
pub fn audio_bank() -> &'static crate::audio::AudioBank {
    audio::audio_bank()
}

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.require_text_layout_capacity(TEXT_LAYOUT_CAPACITY)
        .expect("register Marble text layout capacity");
    #[cfg(all(target_arch = "wasm32", feature = "web-canvas"))]
    app.prefer_text_raster_scratch(256, 8)
        .expect("register Marble text raster extent");
    #[cfg(feature = "std")]
    app.add_plugin(StdInstantClockPlugin);
    app.with_widget(board::board_render::view());
    app.with_widget(scenes::scene_board_render::view());
    app.with_widget(settings::settings_panel_render::view());
    let model = app.add_model(MarbleModel::new());
    #[cfg(feature = "audio")]
    {
        let audio = app.audio();
        app.on_effect(&model, move |sound: MarbleSound| {
            if let Some(audio) = &audio {
                audio::submit_sound(audio, sound);
            }
        })
        .expect("Marble sound consumer is registered once");
    }
    app.add_system(marble_tick_system::system(model.clone()));
    #[cfg(feature = "audio")]
    {
        let audio = app.audio();
        app.compose(parent, |cx| {
            shell::build_widgets(cx, model.clone(), audio.clone())
        });
    }
    #[cfg(not(feature = "audio"))]
    app.compose(parent, |cx| shell::build_widgets(cx, model.clone()));
    #[cfg(feature = "audio")]
    if let Some(audio) = app.audio() {
        let _ = audio.set_master_gain(107);
        let _ = audio.set_muted(true);
    }
}

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::fixed(480, 320);

#[cfg(test)]
mod tests;
