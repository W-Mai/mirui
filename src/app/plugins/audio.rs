use crate::app::plugin::Plugin;
use crate::app::{App, RendererFactory};
use crate::audio::{
    AudioBank, AudioBus, AudioOutputState, AudioSink, AudioState, AudioStateSignal,
};
use crate::ecs::World;
use crate::surface::{InputEvent, Surface};

/// Routes a fixed-capacity [`AudioBus`] into one platform audio sink.
///
/// **Inserts**
/// - resource: `AudioBus<N>`
/// - resource: `AudioStateSignal`
/// - hooks:    `on_event` / `pre_render` / `on_suspend` / `on_resume` / `on_quit`
pub struct AudioPlugin<S: AudioSink, const N: usize = 32> {
    sink: S,
    bank: &'static AudioBank,
}

impl<S: AudioSink> AudioPlugin<S, 32> {
    pub const fn new(sink: S, bank: &'static AudioBank) -> Self {
        Self { sink, bank }
    }
}

impl<S: AudioSink, const N: usize> AudioPlugin<S, N> {
    pub const fn with_capacity(sink: S, bank: &'static AudioBank) -> Self {
        Self { sink, bank }
    }

    fn mark_failure(world: &mut World) {
        if let Some(bus) = world.resource_mut::<AudioBus<N>>() {
            bus.record_failure();
        }
        Self::publish_state(world);
    }

    fn publish_state(world: &World) {
        let Some(snapshot) = world.resource::<AudioBus<N>>().map(AudioState::from_bus) else {
            return;
        };
        if let Some(signal) = world.resource::<AudioStateSignal>() {
            signal.publish(snapshot);
        }
    }

    fn sync_state(&self, world: &mut World) {
        if let Some(bus) = world.resource_mut::<AudioBus<N>>() {
            bus.set_state(self.sink.state());
        }
        Self::publish_state(world);
    }

    fn unlock_if_needed(&mut self, world: &mut World) {
        if matches!(
            self.sink.state(),
            AudioOutputState::Starting | AudioOutputState::Locked
        ) {
            if self.sink.unlock().is_err() {
                Self::mark_failure(world);
            } else {
                self.sync_state(world);
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
        let mut bus = AudioBus::<N>::new();
        if self.sink.start(self.bank).is_err() {
            bus.record_failure();
        } else {
            bus.set_state(self.sink.state());
        }
        app.world.insert_resource(bus);
        let snapshot = app
            .world
            .resource::<AudioBus<N>>()
            .map(AudioState::from_bus)
            .expect("AudioPlugin inserts its bus before publishing state");
        app.world.insert_resource(AudioStateSignal::new(snapshot));
    }

    fn on_event(&mut self, world: &mut World, event: &InputEvent) -> bool {
        let unlock = matches!(
            event,
            InputEvent::PointerDown { .. } | InputEvent::Key { pressed: true, .. }
        );
        if unlock {
            self.unlock_if_needed(world);
        }
        false
    }

    fn on_host_interaction(&mut self, world: &mut World) {
        self.unlock_if_needed(world);
    }

    fn pre_render(&mut self, world: &mut World) {
        if self.sink.update().is_err() {
            Self::mark_failure(world);
            return;
        }
        self.sync_state(world);
        if self.sink.state() != AudioOutputState::Ready {
            return;
        }
        loop {
            let command = world.resource_mut::<AudioBus<N>>().and_then(AudioBus::pop);
            let Some(command) = command else { break };
            if self.sink.submit(command).is_err() {
                Self::mark_failure(world);
                break;
            }
        }
    }

    fn on_suspend(&mut self, world: &mut World) {
        self.sink.suspend();
        self.sync_state(world);
    }

    fn on_resume(&mut self, world: &mut World) {
        if self.sink.resume().is_err() {
            Self::mark_failure(world);
        } else {
            self.sync_state(world);
        }
    }

    fn on_quit(&mut self, world: &mut World) {
        self.sink.stop();
        self.sync_state(world);
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
        app.world.resource_mut::<AudioBus<4>>().unwrap().play(CUE);
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
        let state = app.world.resource::<AudioStateSignal>().unwrap().clone();
        let observed = Rc::new(RefCell::new(AudioOutputState::Starting));
        let observed_for_effect = Rc::clone(&observed);
        let _effect = Effect::new(move || {
            *observed_for_effect.borrow_mut() = state.get().output;
        });

        let _root = app.spawn_root().id();
        app.notify_host_interaction();
        app.render().unwrap();

        assert_eq!(*observed.borrow(), AudioOutputState::Ready);
    }
}
