fn main() {
    gallery::run("mirui — Marble Play", 480, 320, |setup| {
        #[cfg(all(any(feature = "sdl", feature = "sdl-gpu"), not(target_arch = "wasm32")))]
        setup.app.add_plugin(mirui::app::plugins::AudioPlugin::new(
            mirui::audio::SdlAudioSink::new(),
            mirui::gallery::demos::marble_play::audio_bank(),
        ));
        #[cfg(all(feature = "web-canvas", target_arch = "wasm32"))]
        setup.app.add_plugin(mirui::app::plugins::AudioPlugin::new(
            mirui::audio::WebAudioSink::new(),
            mirui::gallery::demos::marble_play::audio_bank(),
        ));
        let parent = setup.app.spawn_root().id();
        mirui::gallery::demos::marble_play::setup_app(setup.app, parent);
        parent
    });
}
