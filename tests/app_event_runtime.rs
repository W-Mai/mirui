use std::collections::VecDeque;

use mirui::app::plugin::Plugin;
use mirui::app::plugins::AutoSuspendPlugin;
use mirui::app::{App, RendererFactory};
use mirui::core::cache::InspectCaches;
use mirui::ecs::World;
use mirui::input::event::contract::Event;
use mirui::input::event::input::{CharInput, InputEvent, Key, Rotary, Wheel};
use mirui::input::event::lifecycle::{
    HostInteraction, MemoryWarning, Quit, Resumed, Started, Suspended,
};
use mirui::render::texture::Texture;
use mirui::surface::framebuf::FramebufSurface;
use mirui::surface::{DisplayInfo, FramebufferAccess, Surface};
use mirui::types::{Fixed, PhysicalRect};

struct QueuedSurface {
    framebuffer: FramebufSurface<fn(&[u8], PhysicalRect)>,
    events: VecDeque<InputEvent>,
}

impl QueuedSurface {
    fn new(events: impl IntoIterator<Item = InputEvent>) -> Self {
        Self {
            framebuffer: FramebufSurface::new(8, 8, |_, _| {}),
            events: events.into_iter().collect(),
        }
    }
}

impl InspectCaches for QueuedSurface {}

impl Surface for QueuedSurface {
    fn display_info(&self) -> DisplayInfo {
        self.framebuffer.display_info()
    }

    fn physical_size(&self) -> (u32, u32) {
        self.framebuffer.physical_size()
    }

    fn flush(&mut self, area: PhysicalRect) {
        self.framebuffer.flush(area);
    }

    fn poll_event(&mut self) -> Option<InputEvent> {
        self.events.pop_front()
    }
}

impl FramebufferAccess for QueuedSurface {
    fn framebuffer(&mut self) -> Texture<'_> {
        self.framebuffer.framebuffer()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Step {
    #[default]
    Empty,
    StartHook,
    Started,
    SuspendHook,
    Suspended,
    ResumeHook,
    Resumed,
    HostHook,
    HostInteraction,
    MemoryHook,
    MemoryWarning,
    QuitHook,
    Quit,
    Key(Key),
    CharInput(CharInput),
    Wheel(Wheel),
    Rotary(Rotary),
}

#[derive(Default)]
struct Trace {
    steps: [Step; 32],
    len: usize,
}

impl Trace {
    fn push(&mut self, step: Step) {
        self.steps[self.len] = step;
        self.len += 1;
    }

    fn steps(&self) -> &[Step] {
        &self.steps[..self.len]
    }
}

/// Records lifecycle hooks before authored notifications.
///
/// **Inserts**
/// - resource: Trace
/// - hooks: on_start, on_suspend, on_resume, on_host_interaction, on_memory_warning, on_quit
struct TraceHooks;

impl<B: Surface, F: RendererFactory<B>> Plugin<B, F> for TraceHooks {
    fn build(&mut self, app: &mut App<B, F>) {
        app.world.insert_resource(Trace::default());
    }

    fn on_start(&mut self, world: &mut World) {
        world.resource_mut::<Trace>().unwrap().push(Step::StartHook);
    }

    fn on_suspend(&mut self, world: &mut World) {
        world
            .resource_mut::<Trace>()
            .unwrap()
            .push(Step::SuspendHook);
    }

    fn on_resume(&mut self, world: &mut World) {
        world
            .resource_mut::<Trace>()
            .unwrap()
            .push(Step::ResumeHook);
    }

    fn on_host_interaction(&mut self, world: &mut World) {
        world.resource_mut::<Trace>().unwrap().push(Step::HostHook);
    }

    fn on_memory_warning(&mut self, world: &mut World) {
        world
            .resource_mut::<Trace>()
            .unwrap()
            .push(Step::MemoryHook);
    }

    fn on_quit(&mut self, world: &mut World) {
        world.resource_mut::<Trace>().unwrap().push(Step::QuitHook);
    }
}

fn app(events: impl IntoIterator<Item = InputEvent>) -> App<QueuedSurface> {
    let mut app = App::new(QueuedSurface::new(events));
    app.add_plugin(TraceHooks);
    app
}

#[mirui::event]
fn notice(trace: res!(mut Trace), step: Step) {
    trace.push(step);
}

fn install_notices(app: &mut App<QueuedSurface>) {
    app.on_event(Started::EVENT, notice::callback(Step::Started))
        .on_event(Suspended::EVENT, notice::callback(Step::Suspended))
        .on_event(Resumed::EVENT, notice::callback(Step::Resumed))
        .on_event(
            HostInteraction::EVENT,
            notice::callback(Step::HostInteraction),
        )
        .on_event(MemoryWarning::EVENT, notice::callback(Step::MemoryWarning))
        .on_event(Quit::EVENT, notice::callback(Step::Quit));
}

#[mirui::event]
fn key(input: event!(Key), trace: res!(mut Trace)) -> bool {
    trace.push(Step::Key(*input));
    false
}

#[mirui::event]
fn char_input(input: event!(CharInput), trace: res!(mut Trace)) -> bool {
    trace.push(Step::CharInput(*input));
    false
}

#[mirui::event]
fn wheel(input: event!(Wheel), trace: res!(mut Trace)) -> bool {
    trace.push(Step::Wheel(*input));
    false
}

#[mirui::event]
fn rotary(input: event!(Rotary), trace: res!(mut Trace)) -> bool {
    trace.push(Step::Rotary(*input));
    false
}

#[test]
fn rootless_app_dispatches_all_four_typed_input_kinds_once() {
    let key_input = Key {
        code: 42,
        pressed: true,
    };
    let text = CharInput { ch: '界' };
    let wheel_input = Wheel {
        dx: Fixed::from_int(2),
        dy: Fixed::from_int(-3),
        x: Fixed::from_int(4),
        y: Fixed::from_int(5),
    };
    let rotary_input = Rotary { id: 2, delta: -7 };
    let mut app = app([
        key_input.into(),
        text.into(),
        wheel_input.into(),
        rotary_input.into(),
    ]);
    app.on_event(Key::EVENT, key::callback())
        .on_event(CharInput::EVENT, char_input::callback())
        .on_event(Wheel::EVENT, wheel::callback())
        .on_event(Rotary::EVENT, rotary::callback());
    assert!(app.root.is_none());
    assert!(!app.tick());
    assert!(!app.tick());
    assert_eq!(
        app.world.resource::<Trace>().unwrap().steps(),
        &[
            Step::StartHook,
            Step::Key(key_input),
            Step::CharInput(text),
            Step::Wheel(wheel_input),
            Step::Rotary(rotary_input),
        ],
    );
}

#[test]
fn lifecycle_notifications_follow_hooks_and_ignore_duplicate_transitions() {
    let mut app = app([]);
    install_notices(&mut app);
    app.suspend();
    app.suspend();
    app.resume();
    app.resume();
    app.notify_host_interaction();
    app.handle_memory_warning();
    assert_eq!(
        app.world.resource::<Trace>().unwrap().steps(),
        &[
            Step::SuspendHook,
            Step::Suspended,
            Step::ResumeHook,
            Step::Resumed,
            Step::HostHook,
            Step::HostInteraction,
            Step::MemoryHook,
            Step::MemoryWarning,
        ],
    );
}

#[test]
fn run_and_tick_start_and_quit_only_once() {
    for use_run in [false, true] {
        let mut app = app([InputEvent::Quit, InputEvent::Quit]);
        install_notices(&mut app);
        if use_run {
            app.run();
            app.run();
        } else {
            assert!(app.tick());
            assert!(app.tick());
        }
        assert_eq!(
            app.world.resource::<Trace>().unwrap().steps(),
            &[Step::StartHook, Step::Started, Step::QuitHook, Step::Quit],
        );
    }
}

/// Consumes user input while allowing other plugins to handle lifecycle requests.
///
/// **Inserts**
/// - hooks: on_event
struct ConsumeInput;

impl<B: Surface, F: RendererFactory<B>> Plugin<B, F> for ConsumeInput {
    fn build(&mut self, _: &mut App<B, F>) {}

    fn on_event(&mut self, _: &mut World, _: &InputEvent) -> bool {
        true
    }
}

#[test]
fn plugin_consumption_blocks_user_callbacks_but_not_lifecycle_requests() {
    let input = InputEvent::Key {
        code: 42,
        pressed: true,
    };
    let mut app = app([input.clone(), InputEvent::AppSuspend]);
    install_notices(&mut app);
    app.on_event(Key::EVENT, key::callback());
    app.add_plugin(ConsumeInput);
    app.add_plugin(AutoSuspendPlugin);
    assert!(!app.tick());
    assert!(app.is_suspended());
    app.backend
        .events
        .extend([input, InputEvent::AppResume, InputEvent::Quit]);
    assert!(app.tick());
    assert!(!app.is_suspended());
    assert!(app.tick());
    assert_eq!(
        app.world.resource::<Trace>().unwrap().steps(),
        &[
            Step::StartHook,
            Step::Started,
            Step::SuspendHook,
            Step::Suspended,
            Step::ResumeHook,
            Step::Resumed,
            Step::QuitHook,
            Step::Quit,
        ],
    );
}

#[cfg(all(feature = "std", not(target_arch = "wasm32")))]
const RUNNER_COMPLETED: &str = "MIRUI_APP_EVENT_RUNNER_CALLBACK_COMPLETED";

#[cfg(all(feature = "std", not(target_arch = "wasm32")))]
#[mirui::event]
fn runner_quit(trace: res!(mut Trace)) {
    trace.push(Step::Quit);
    assert_eq!(
        trace.steps(),
        &[Step::StartHook, Step::Started, Step::QuitHook, Step::Quit]
    );
    println!("{RUNNER_COMPLETED}");
}

#[cfg(all(feature = "std", not(target_arch = "wasm32")))]
#[test]
fn runner_starts_and_quits_once() {
    const CHILD: &str = "MIRUI_APP_EVENT_RUNNER_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let mut app = app([InputEvent::Quit]);
        app.on_event(Started::EVENT, notice::callback(Step::Started))
            .on_event(Quit::EVENT, runner_quit::callback());
        app.into_runner().run_blocking();
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "runner_starts_and_quits_once", "--nocapture"])
        .env(CHILD, "1")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains(RUNNER_COMPLETED),
        "typed Quit callback did not complete: {output:?}",
    );
}

#[test]
fn resume_delivers_the_remaining_input_burst_in_the_same_tick() {
    let input = Key {
        code: 19,
        pressed: true,
    };
    let mut app = app([InputEvent::AppResume, input.into()]);
    app.add_plugin(AutoSuspendPlugin);
    app.on_event(Resumed::EVENT, notice::callback(Step::Resumed))
        .on_event(Key::EVENT, key::callback());
    app.suspend();
    assert!(!app.tick());
    assert!(!app.is_suspended());
    assert_eq!(
        app.world.resource::<Trace>().unwrap().steps(),
        &[
            Step::SuspendHook,
            Step::StartHook,
            Step::ResumeHook,
            Step::Resumed,
            Step::Key(input),
        ]
    );
    assert!(!app.tick());
    assert_eq!(app.world.resource::<Trace>().unwrap().steps().len(), 5);
}

#[mirui::model]
struct InstallModel {
    value: u32,
}

#[mirui::model]
impl InstallModel {
    fn increment(&mut self) {
        self.value += 1;
    }
    fn value(&self) -> u32 {
        self.value
    }
}

#[mirui::event]
fn captured_models(first: model!(InstallModel), second: model!(InstallModel)) -> bool {
    first.increment();
    second.increment();
    true
}

#[mirui::event]
fn conflicting_access(left: res!(Trace), right: res!(mut Trace)) -> bool {
    let _ = (left, right);
    panic!("rejected access plan reached its body")
}

struct InstallWarnings {
    thread: std::thread::ThreadId,
    messages: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl mirui::core::log::Sink for InstallWarnings {
    fn enabled(&self, level: mirui::core::log::Level, target: &'static str) -> bool {
        level == mirui::core::log::Level::Warn
            && target == "mirui::event::install"
            && std::thread::current().id() == self.thread
    }

    fn emit(&self, event: &mirui::core::log::Event<'_>) {
        self.messages
            .lock()
            .unwrap()
            .push(format!("{}", event.args));
    }
}

#[test]
fn rejected_model_and_access_bindings_warn_once_and_leave_valid_callbacks_running() {
    let input = Key {
        code: 23,
        pressed: true,
    };
    let mut app = app([input.into()]);
    let local = app.add_model(InstallModel { value: 0 });
    let mut foreign_app = App::headless(8, 8);
    let foreign = foreign_app.add_model(InstallModel { value: 0 });
    let ended = {
        let mut owner = App::headless(8, 8);
        owner.add_model(InstallModel { value: 0 })
    };
    let warnings = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    mirui::core::log::install_sink(Box::new(InstallWarnings {
        thread: std::thread::current().id(),
        messages: warnings.clone(),
    }));
    app.on_event(
        Key::EVENT,
        captured_models::callback(foreign.clone(), foreign.clone()),
    )
    .on_event(
        Key::EVENT,
        captured_models::callback(local.clone(), foreign.clone()),
    )
    .on_event(Key::EVENT, captured_models::callback(ended.clone(), ended))
    .on_event(Key::EVENT, conflicting_access::callback())
    .on_event(Key::EVENT, key::callback());
    assert!(!app.tick());
    assert!(!app.tick());
    assert_eq!(local.value(), 0);
    assert_eq!(foreign.value(), 0);
    assert_eq!(
        app.world.resource::<Trace>().unwrap().steps(),
        &[Step::StartHook, Step::Key(input)]
    );
    let warnings = warnings.lock().unwrap();
    if cfg!(any(
        feature = "log-max-level-off",
        feature = "log-max-level-error"
    )) {
        assert!(warnings.is_empty());
        return;
    }
    assert_eq!(warnings.len(), 4);
    for reason in [
        "ForeignOwner",
        "MixedOwners",
        "RegistrationEnded",
        "Conflict",
    ] {
        assert!(
            warnings.iter().any(|message| message.contains(reason)),
            "missing {reason}: {warnings:?}"
        );
    }
    assert!(warnings.iter().all(|message| message.contains("callback")));
}
