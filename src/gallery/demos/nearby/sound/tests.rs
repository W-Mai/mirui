use super::*;
use crate::app::plugins::AudioPlugin;
use crate::audio::{AudioCommand, AudioMixer, AudioSink};
use crate::gallery::demos::nearby::{
    launcher::{MenuAction, Overlay},
    model, setup_app,
};
use crate::prelude::*;
use alloc::{rc::Rc, vec::Vec};
use core::{cell::RefCell, convert::Infallible};

#[derive(Clone)]
struct RecordingSink(Rc<RefCell<Vec<AudioCommand>>>);

impl AudioSink for RecordingSink {
    type Error = Infallible;
    fn start(&mut self, _: &'static AudioBank) -> Result<(), Self::Error> {
        Ok(())
    }
    fn submit(&mut self, command: AudioCommand) -> Result<(), Self::Error> {
        self.0.borrow_mut().push(command);
        Ok(())
    }
}

type TestApp = App<crate::surface::framebuf::FramebufSurface<fn(&[u8], PhysicalRect)>>;

fn fixture() -> (TestApp, model::DisplayStateHandle, RecordingSink) {
    let mut app = App::headless(640, 480);
    app.with_default_widgets().with_default_systems();
    let sink = RecordingSink(Rc::new(RefCell::new(Vec::with_capacity(128))));
    app.add_plugin(AudioPlugin::new(sink.clone(), audio_bank()));
    let root = app.spawn_root().id();
    let state = setup_app(&mut app, root);
    app.world.remove_resource::<crate::ecs::MonoClock>();
    app.add_resource(crate::ecs::DeltaTimeMs(16));
    app.render().unwrap();
    sink.0.borrow_mut().clear();
    (app, state, sink)
}

fn played(sink: &RecordingSink) -> Vec<CueId> {
    sink.0
        .borrow()
        .iter()
        .filter_map(|command| match command {
            AudioCommand::Play { cue, .. } => Some(*cue),
            _ => None,
        })
        .collect()
}

#[test]
fn every_cue_is_finite_audible_and_unclipped() {
    let bank = audio_bank();
    assert_eq!(bank.cues().len(), Cue::Restored as usize + 1);
    for cue in bank.cues() {
        let mut mixer = AudioMixer::<4>::new(bank, 48_000).unwrap();
        mixer
            .apply(AudioCommand::Play {
                cue: cue.id,
                gain: 255,
            })
            .unwrap();
        let mut pcm = alloc::vec![0; 24_000];
        mixer.render_mono(&mut pcm);
        assert!(pcm.iter().any(|sample| *sample != 0));
        assert!(pcm.iter().all(|sample| sample.unsigned_abs() < 30_000));
        assert!(pcm[16_800..].iter().all(|sample| *sample == 0));
        assert_eq!(mixer.active_voice_count(), 0);
    }
}

#[test]
fn feedback_is_bounded_and_state_notices_replace_navigation_with_no_allocations() {
    let mut feedback = Feedback::default();
    let allocations = crate::test_support::tracked_allocations(|| {
        feedback.emit(Cue::Focus);
        feedback.emit(Cue::Confirm);
        feedback.emit(Cue::Ready);
        assert_eq!(feedback.take().unwrap().cue, Some(Cue::Ready));
        assert!(feedback.take().is_none());
        feedback.emit(Cue::Focus);
        assert_eq!(feedback.take().unwrap().cue, Some(Cue::Focus));
        feedback.emit(Cue::Focus);
        assert!(feedback.take().is_none());
        feedback.advance(65);
        feedback.emit(Cue::Focus);
        assert_eq!(feedback.take().unwrap().cue, Some(Cue::Focus));
        feedback.configure(false, 20);
        assert_eq!(
            feedback.take().unwrap().preferences,
            Some((false, gain(20)))
        );
    });
    assert_eq!(allocations, 0);
}

#[test]
fn nested_launcher_effects_play_once_and_paint_or_unchanged_selection_stays_quiet() {
    let (mut app, state, sink) = fixture();
    let library = state.launcher();
    library.select(1);
    app.render_dirty().unwrap();
    assert_eq!(played(&sink), [Cue::Focus.id()]);
    sink.0.borrow_mut().clear();
    library.select(1);
    library.animate(200);
    state.resize(480);
    app.render_dirty().unwrap();
    app.render().unwrap();
    assert!(played(&sink).is_empty());
    library.show_overlay(Overlay::System);
    app.render_dirty().unwrap();
    assert_eq!(played(&sink), [Cue::Confirm.id()]);
}

#[test]
fn sound_and_volume_controls_share_the_host_audio_state_and_expire_with_the_app() {
    let (mut app, state, sink) = fixture();
    let library = state.launcher();
    let audio = app.audio().unwrap();
    assert_eq!(audio.state().unwrap().master_gain, gain(60));
    library.toggle_sound();
    assert!(audio.state().unwrap().muted);
    app.render().unwrap();
    assert!(played(&sink).is_empty());
    library.advance_audio(150);
    library.toggle_sound();
    assert!(!audio.state().unwrap().muted);
    app.render().unwrap();
    assert_eq!(played(&sink), [Cue::Confirm.id()]);
    audio.set_muted(true);
    library.show_overlay(Overlay::System);
    assert_eq!(library.item(1).unwrap().value, "关");
    audio.set_master_gain(gain(25));
    library.show_overlay(Overlay::Controls);
    assert_eq!(library.item(0).unwrap().value, "25%");
    library.adjust(MenuAction::VolumeUp);
    assert_eq!(audio.state().unwrap().master_gain, gain(35));
    assert!(audio.state().unwrap().muted);
    drop(app);
    assert!(audio.state().is_none());
}

#[test]
fn connection_state_edges_reach_the_same_audio_consumer_without_replaying_frames() {
    use crate::gallery::demos::nearby::data::Phase;
    let (mut app, state, sink) = fixture();
    state.launch_game("双人冒险".into(), true);
    state.launcher().advance_audio(150);
    state.activate(0);
    app.render().unwrap();
    sink.0.borrow_mut().clear();
    state.advance();
    state.advance();
    state.advance();
    assert_eq!(state.snapshot().phase, Phase::ApprovalPending);
    app.render().unwrap();
    assert_eq!(played(&sink), [Cue::JoinRequest.id()]);
    sink.0.borrow_mut().clear();
    state.confirm();
    assert_eq!(state.snapshot().phase, Phase::HostReady);
    app.render().unwrap();
    assert_eq!(played(&sink), [Cue::Ready.id()]);
    sink.0.borrow_mut().clear();
    state.advance();
    app.render().unwrap();
    assert!(played(&sink).is_empty());
    state.confirm();
    app.render().unwrap();
    assert_eq!(played(&sink), [Cue::Start.id()]);
}

#[test]
fn discovery_refresh_does_not_announce_the_same_room_twice() {
    let (mut app, state, sink) = fixture();
    state.open_network();
    app.render().unwrap();
    assert_eq!(played(&sink), [Cue::RoomFound.id()]);
    sink.0.borrow_mut().clear();
    state.refresh_rooms();
    app.render().unwrap();
    assert_eq!(played(&sink), [Cue::Refresh.id()]);
    sink.0.borrow_mut().clear();
    state.advance();
    app.render().unwrap();
    assert!(played(&sink).is_empty());
}

#[test]
fn host_mute_updates_the_mounted_settings_without_a_model_write() {
    let (mut app, state, _) = fixture();
    state.launcher().show_overlay(Overlay::System);
    app.render().unwrap();
    let audio = app.audio().unwrap();
    audio.set_muted(true);
    app.render_dirty().unwrap();
    assert!(
        app.world
            .query::<crate::ui::widgets::Text>()
            .iter()
            .any(|(entity, text)| {
                app.world.has::<crate::ui::ComputedRect>(entity) && text.resolve(None) == "关"
            })
    );
}

#[test]
fn failed_connections_and_restored_exits_emit_only_on_their_state_edges() {
    use crate::gallery::demos::nearby::data::Phase;
    let (mut app, state, sink) = fixture();
    state.launch_game("双人冒险".into(), true);
    state.next_scenario();
    state.activate(0);
    app.render().unwrap();
    sink.0.borrow_mut().clear();
    state.advance();
    assert_eq!(state.snapshot().phase, Phase::Failed);
    app.render().unwrap();
    assert_eq!(played(&sink), [Cue::Error.id()]);
    sink.0.borrow_mut().clear();
    state.advance();
    app.render().unwrap();
    assert!(played(&sink).is_empty());
    state.next_scenario();
    state.activate(0);
    state.back_page();
    state.advance();
    assert_eq!(state.snapshot().phase, Phase::RestoreFailed);
    app.render().unwrap();
    sink.0.borrow_mut().clear();
    state.confirm();
    app.render().unwrap();
    sink.0.borrow_mut().clear();
    state.advance();
    app.render().unwrap();
    assert_eq!(played(&sink), [Cue::Restored.id()]);
}
