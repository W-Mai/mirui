mod bank;
mod bus;
mod mixer;
mod sink;
mod state;

#[cfg(all(feature = "sdl-audio", not(target_arch = "wasm32")))]
mod sdl;
#[cfg(all(feature = "web-audio", target_arch = "wasm32"))]
mod web;

pub use bank::{AudioBank, AudioCue, AudioTone, CueId, NoteEvent, Score, Waveform};
pub use bus::{AudioBus, AudioCommand, AudioOutputState};
pub use mixer::{AudioMixer, MixerError};
pub use sink::{AudioSink, SilentAudioSink};
pub use state::{AudioState, AudioStateSignal};

#[cfg(all(feature = "sdl-audio", not(target_arch = "wasm32")))]
pub use sdl::SdlAudioSink;
#[cfg(all(feature = "web-audio", target_arch = "wasm32"))]
pub use web::WebAudioSink;
