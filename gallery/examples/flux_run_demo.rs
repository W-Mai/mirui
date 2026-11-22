fn main() {
    let compact = std::env::args().any(|argument| argument == "--compact");
    let autoplay = std::env::args().any(|argument| argument == "--auto");
    let (width, height) = if compact {
        mirui_flux_run::COMPACT_VIEWPORT
    } else {
        mirui_flux_run::VIEWPORT
    };
    gallery::run("mirui — Flux Run", width, height, |setup| {
        #[cfg(all(any(feature = "sdl", feature = "sdl-gpu"), not(target_arch = "wasm32")))]
        setup.app.add_plugin(mirui::app::plugins::AudioPlugin::new(
            mirui::audio::SdlAudioSink::new(),
            mirui_flux_run::audio_bank(),
        ));
        let parent = setup.app.spawn_root().id();
        let model = if compact {
            mirui_flux_run::setup_compact_app(setup.app, parent)
        } else {
            mirui_flux_run::setup_app(setup.app, parent)
        };
        if autoplay {
            model.set_autoplay(true);
        }
        parent
    });
}
