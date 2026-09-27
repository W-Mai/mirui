use alloc::rc::Rc;

use crate::app::plugin::Plugin;
use crate::app::{App, RendererFactory};
use crate::audio::{AudioBank, AudioBus, AudioOutputState, AudioSink, SharedAudioCore};
use crate::ecs::World;
use crate::surface::{InputEvent, Surface};

/// Routes a fixed-capacity [`AudioBus`] into one platform audio sink.
///
/// **Inserts**
/// - resource: `AudioHandle`
/// - resource: `AudioStateSignal`
/// - hooks:    `on_event` / `pre_render` / `on_suspend` / `on_resume` / `on_quit`
pub struct AudioPlugin<S: AudioSink, const N: usize = 32> {
    sink: S,
    bank: &'static AudioBank,
    core: Option<Rc<SharedAudioCore<N>>>,
    failed: bool,
}

impl<S: AudioSink> AudioPlugin<S, 32> {
    pub const fn new(sink: S, bank: &'static AudioBank) -> Self {
        Self {
            sink,
            bank,
            core: None,
            failed: false,
        }
    }
}

impl<S: AudioSink, const N: usize> AudioPlugin<S, N> {
    pub const fn with_capacity(sink: S, bank: &'static AudioBank) -> Self {
        Self {
            sink,
            bank,
            core: None,
            failed: false,
        }
    }

    fn mark_failure(&mut self) {
        self.failed = true;
        if let Some(core) = &self.core {
            core.record_failure();
        }
    }

    fn sync_state(&self) {
        if !self.failed
            && let Some(core) = &self.core
        {
            core.set_output_state(self.sink.state());
        }
    }

    fn unlock_if_needed(&mut self) {
        if matches!(
            self.sink.state(),
            AudioOutputState::Starting | AudioOutputState::Locked
        ) {
            if self.sink.unlock().is_err() {
                self.mark_failure();
            } else {
                self.failed = false;
                self.sync_state();
            }
        }
    }
}

impl<B, F, S, const N: usize> Plugin<B, F> for AudioPlugin<S, N>
where
    B: Surface,
    F: RendererFactory<B>,
    S: AudioSink + 'static,
{
    fn build(&mut self, app: &mut App<B, F>) {
        assert!(
            app.audio().is_none(),
            "only one audio output plugin can be installed"
        );
        let mut bus = AudioBus::<N>::new();
        if self.sink.start(self.bank).is_err() {
            self.failed = true;
            bus.record_failure();
        } else {
            bus.set_state(self.sink.state());
        }
        let core = SharedAudioCore::new(bus);
        app.world.insert_resource(SharedAudioCore::handle(&core));
        app.world.insert_resource(core.state_signal());
        self.core = Some(core);
    }

    fn on_event(&mut self, _world: &mut World, event: &InputEvent) -> bool {
        let unlock = matches!(
            event,
            InputEvent::PointerDown { .. } | InputEvent::Key { pressed: true, .. }
        );
        if unlock {
            self.unlock_if_needed();
        }
        false
    }

    fn on_host_interaction(&mut self, _world: &mut World) {
        self.unlock_if_needed();
    }

    fn pre_render(&mut self, _world: &mut World) {
        if self.failed {
            return;
        }
        if self.sink.update().is_err() {
            self.mark_failure();
            return;
        }
        self.sync_state();
        if self.sink.state() != AudioOutputState::Ready {
            return;
        }
        loop {
            let command = self.core.as_ref().and_then(|core| core.pop());
            let Some(command) = command else { break };
            if self.sink.submit(command).is_err() {
                self.mark_failure();
                break;
            }
        }
    }

    fn on_suspend(&mut self, _world: &mut World) {
        self.sink.suspend();
        self.sync_state();
    }

    fn on_resume(&mut self, _world: &mut World) {
        if self.sink.resume().is_err() {
            self.mark_failure();
        } else {
            self.failed = false;
            self.sync_state();
        }
    }

    fn on_quit(&mut self, _world: &mut World) {
        self.sink.stop();
        self.failed = false;
        self.sync_state();
    }
}

impl<S: AudioSink, const N: usize> Drop for AudioPlugin<S, N> {
    fn drop(&mut self) {
        self.sink.stop();
    }
}

#[cfg(test)]
mod tests {
    use alloc::rc::Rc;
    use core::cell::RefCell;
    use core::convert::Infallible;

    use super::*;
    use crate::app::SwRendererFactory;
    use crate::audio::{AudioCommand, AudioCue, CueId, NoteEvent, Score, Waveform};
    use crate::core::reactive::Effect;
    use crate::surface::framebuf::FramebufSurface;
    use crate::types::{Fixed, PhysicalRect};

    const CUE: CueId = CueId::new(1);
    static NOTES: [NoteEvent; 1] = [NoteEvent::new(0, 1, 60, 255, Waveform::Square)];
    static CUES: [AudioCue; 1] = [AudioCue::new(CUE, Score::new(10, 1, false, &NOTES))];
    static BANK: AudioBank = AudioBank::new(&CUES);

    #[derive(Default)]
    struct Trace {
        unlocks: u8,
        commands: u8,
        suspends: u8,
        resumes: u8,
        stops: u8,
        state: AudioOutputState,
    }

    struct MockSink(Rc<RefCell<Trace>>);

    impl AudioSink for MockSink {
        type Error = Infallible;

        fn start(&mut self, _bank: &'static AudioBank) -> Result<(), Self::Error> {
            self.0.borrow_mut().state = AudioOutputState::Locked;
            Ok(())
        }

        fn unlock(&mut self) -> Result<(), Self::Error> {
            let mut trace = self.0.borrow_mut();
            trace.unlocks += 1;
            trace.state = AudioOutputState::Ready;
            Ok(())
        }

        fn submit(&mut self, _command: AudioCommand) -> Result<(), Self::Error> {
            self.0.borrow_mut().commands += 1;
            Ok(())
        }

        fn suspend(&mut self) {
            let mut trace = self.0.borrow_mut();
            trace.suspends += 1;
            trace.state = AudioOutputState::Suspended;
        }

        fn resume(&mut self) -> Result<(), Self::Error> {
            let mut trace = self.0.borrow_mut();
            trace.resumes += 1;
            trace.state = AudioOutputState::Ready;
            Ok(())
        }

        fn stop(&mut self) {
            let mut trace = self.0.borrow_mut();
            trace.stops += 1;
            trace.state = AudioOutputState::Stopped;
        }

        fn state(&self) -> AudioOutputState {
            self.0.borrow().state
        }
    }

    struct RejectSubmitSink(Rc<RefCell<Trace>>);

    impl AudioSink for RejectSubmitSink {
        type Error = &'static str;

        fn start(&mut self, _bank: &'static AudioBank) -> Result<(), Self::Error> {
            self.0.borrow_mut().state = AudioOutputState::Ready;
            Ok(())
        }

        fn submit(&mut self, _command: AudioCommand) -> Result<(), Self::Error> {
            self.0.borrow_mut().commands += 1;
            Err("output rejected command")
        }

        fn state(&self) -> AudioOutputState {
            AudioOutputState::Ready
        }
    }

    fn app() -> App<FramebufSurface<impl FnMut(&[u8], PhysicalRect)>, SwRendererFactory> {
        let surface = FramebufSurface::new(16, 16, |_bytes, _area| {});
        App::with_factory(surface, SwRendererFactory::new())
    }

    #[test]
    fn plugin_unlocks_without_consuming_input_and_drains_commands() {
        let trace = Rc::new(RefCell::new(Trace::default()));
        let mut app = app();
        app.add_plugin(AudioPlugin::<_, 4>::with_capacity(
            MockSink(trace.clone()),
            &BANK,
        ));
        app.audio().unwrap().play(CUE);
        let mut plugins = core::mem::take(&mut app.plugins);
        assert!(!plugins[0].on_event(
            &mut app.world,
            &InputEvent::PointerDown {
                id: 0,
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
        ));
        plugins[0].pre_render(&mut app.world);
        app.plugins = plugins;
        assert_eq!(trace.borrow().unlocks, 1);
        assert_eq!(trace.borrow().commands, 1);
    }

    #[test]
    fn cloned_controls_expire_with_the_app() {
        let trace = Rc::new(RefCell::new(Trace::default()));
        let mut app = app();
        app.add_plugin(AudioPlugin::<_, 4>::with_capacity(MockSink(trace), &BANK));
        let audio = app.audio().unwrap();
        let clone = audio.clone();
        assert_eq!(clone.state().unwrap().output, AudioOutputState::Locked);
        assert_eq!(
            clone.state_signal().unwrap().get().output,
            AudioOutputState::Locked
        );
        assert!(clone.set_muted(true));
        assert!(audio.state().unwrap().muted);
        assert_eq!(audio.state().unwrap().output, AudioOutputState::Locked);
        drop(app);
        assert!(audio.state().is_none());
        assert!(audio.state_signal().is_none());
        assert!(!clone.play(CUE));
    }

    #[test]
    fn shared_controls_report_locked_output_and_bidirectional_mute() {
        let trace = Rc::new(RefCell::new(Trace::default()));
        let mut app = app();
        app.add_plugin(AudioPlugin::<_, 4>::with_capacity(
            MockSink(trace.clone()),
            &BANK,
        ));
        let outside = app.audio().unwrap();
        let inside = app.audio().unwrap();
        assert_eq!(outside.state().unwrap().output, AudioOutputState::Locked);
        assert!(!outside.state().unwrap().muted);
        assert!(outside.set_muted(true));
        assert!(inside.state().unwrap().muted);
        assert!(inside.set_muted(false));
        assert!(!outside.state().unwrap().muted);
        assert_eq!(trace.borrow().commands, 0);

        app.notify_host_interaction();
        let mut plugins = core::mem::take(&mut app.plugins);
        plugins[0].pre_render(&mut app.world);
        app.plugins = plugins;
        assert_eq!(outside.state().unwrap().output, AudioOutputState::Ready);
        assert_eq!(trace.borrow().commands, 1);
    }

    #[test]
    fn rejected_output_command_remains_failed_until_explicit_recovery() {
        let trace = Rc::new(RefCell::new(Trace::default()));
        let mut app = app();
        app.add_plugin(AudioPlugin::<_, 4>::with_capacity(
            RejectSubmitSink(trace.clone()),
            &BANK,
        ));
        let audio = app.audio().unwrap();
        assert!(audio.play(CUE));
        let mut plugins = core::mem::take(&mut app.plugins);
        plugins[0].pre_render(&mut app.world);
        assert_eq!(audio.state().unwrap().output, AudioOutputState::Failed);
        plugins[0].pre_render(&mut app.world);
        assert_eq!(audio.state().unwrap().output, AudioOutputState::Failed);
        assert_eq!(trace.borrow().commands, 1);
        app.plugins = plugins;

        app.suspend();
        app.resume();
        assert_eq!(audio.state().unwrap().output, AudioOutputState::Ready);
    }

    #[test]
    #[should_panic(expected = "only one audio output plugin can be installed")]
    fn rejects_a_second_audio_output() {
        let trace = Rc::new(RefCell::new(Trace::default()));
        let mut app = app();
        app.add_plugin(AudioPlugin::<_, 4>::with_capacity(
            MockSink(trace.clone()),
            &BANK,
        ));
        app.add_plugin(AudioPlugin::<_, 8>::with_capacity(MockSink(trace), &BANK));
    }

    #[test]
    fn lifecycle_reaches_sink() {
        let trace = Rc::new(RefCell::new(Trace::default()));
        let mut app = app();
        app.add_plugin(AudioPlugin::<_, 4>::with_capacity(
            MockSink(trace.clone()),
            &BANK,
        ));
        app.suspend();
        app.resume();
        drop(app);
        let trace = trace.borrow();
        assert_eq!(trace.suspends, 1);
        assert_eq!(trace.resumes, 1);
        assert_eq!(trace.stops, 1);
    }

    #[test]
    fn host_interaction_unlocks_without_dispatching_widget_input() {
        let trace = Rc::new(RefCell::new(Trace::default()));
        let mut app = app();
        app.add_plugin(AudioPlugin::<_, 4>::with_capacity(
            MockSink(trace.clone()),
            &BANK,
        ));
        app.notify_host_interaction();
        assert_eq!(trace.borrow().unlocks, 1);
        assert_eq!(trace.borrow().commands, 0);
    }

    #[test]
    fn render_publishes_audio_state_before_layout() {
        let trace = Rc::new(RefCell::new(Trace::default()));
        let mut app = app();
        app.with_default_widgets();
        app.add_plugin(AudioPlugin::<_, 4>::with_capacity(MockSink(trace), &BANK));
        let audio = app.audio().unwrap();
        let state = audio.state_signal().unwrap();
        let observed = Rc::new(RefCell::new(AudioOutputState::Starting));
        let observed_for_effect = Rc::clone(&observed);
        let _effect = Effect::new(move || {
            let output = state.get().output;
            assert_eq!(audio.state().unwrap().output, output);
            *observed_for_effect.borrow_mut() = output;
        });

        let _root = app.spawn_root().id();
        app.notify_host_interaction();
        app.render().unwrap();

        assert_eq!(*observed.borrow(), AudioOutputState::Ready);
    }
}
