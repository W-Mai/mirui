use std::collections::VecDeque;

use mirui::app::plugin::Plugin;
use mirui::app::plugins::AutoSuspendPlugin;
use mirui::app::{App, RendererFactory};
use mirui::core::cache::InspectCaches;
use mirui::ecs::World;
use mirui::input::event::contract::Event;
use mirui::input::event::input::{InputEvent, Key, KeyModifiers, Rotary, TextCommitted, Wheel};
use mirui::input::event::lifecycle::{
    HostFocusChanged, HostInteraction, HostVisibilityChanged, MemoryWarning, Quit, Resumed,
    SafeAreaChanged, Started, Suspended, ViewportChanged,
};
use mirui::input::event::press::{PressCancelReason, PressCancelled, PressSelectors};
use mirui::render::texture::Texture;
use mirui::surface::framebuf::FramebufSurface;
use mirui::surface::{DisplayInfo, FramebufferAccess, SafeAreaInsets, Surface};
use mirui::types::{Fixed, PhysicalRect, Viewport};

struct QueuedSurface {
    framebuffer: FramebufSurface<fn(&[u8], PhysicalRect)>,
    events: VecDeque<InputEvent>,
    viewport: Viewport,
    safe_area: SafeAreaInsets,
    pending_geometry: Option<(Viewport, SafeAreaInsets)>,
}

impl QueuedSurface {
    fn new(events: impl IntoIterator<Item = InputEvent>) -> Self {
        Self {
            framebuffer: FramebufSurface::new(8, 8, |_, _| {}),
            events: events.into_iter().collect(),
            viewport: Viewport::new(8, 8, Fixed::ONE),
            safe_area: SafeAreaInsets::default(),
            pending_geometry: None,
        }
    }
}

impl InspectCaches for QueuedSurface {}

impl Surface for QueuedSurface {
    fn display_info(&self) -> DisplayInfo {
        let (width, height) = self.viewport.logical_size();
        DisplayInfo {
            width,
            height,
            scale: self.viewport.scale(),
            ..self.framebuffer.display_info()
        }
    }

    fn physical_size(&self) -> (u32, u32) {
        let (width, height) = self.viewport.physical_size();
        (u32::from(width), u32::from(height))
    }

    fn viewport(&self) -> Viewport {
        self.viewport
    }

    fn safe_area_insets(&self) -> SafeAreaInsets {
        self.safe_area
    }

    fn flush(&mut self, area: PhysicalRect) -> Result<(), mirui::render::RenderError> {
        self.framebuffer.flush(area)
    }

    fn poll_event(&mut self) -> Option<InputEvent> {
        if let Some((viewport, safe_area)) = self.pending_geometry.take() {
            self.viewport = viewport;
            self.safe_area = safe_area;
        }
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
    TextCommitted(usize),
    Wheel(Wheel),
    Rotary(Rotary),
    Viewport(ViewportChanged),
    SafeArea(SafeAreaChanged),
    HostFocus(bool),
    HostVisibility(bool),
    DragStarted,
    DragCancelled,
    DragEnded,
    LayoutChanged,
    Tap,
    Pressed,
    Released,
    PressCancelled(PressCancelReason),
}

#[mirui::event]
fn record_tap(trace: res!(mut Trace)) {
    trace.push(Step::Tap);
}

#[mirui::model]
struct InputGate {
    #[observe]
    visible: bool,
}

#[mirui::model]
struct HitLayout {
    #[observe]
    expanded: bool,
}

#[mirui::model]
impl HitLayout {
    fn expand(&mut self) {
        self.expanded = true;
    }
}

#[mirui::event]
fn expand_before_next_hit(layout: model!(HitLayout), trace: res!(mut Trace)) {
    layout.expand();
    trace.push(Step::LayoutChanged);
}

#[mirui::compose]
fn moving_targets(layout: model!(HitLayout)) -> mirui::ecs::Entity {
    mirui::ui! {
        Column (width: 8, height: 8, bg_color: mirui::types::Color::rgb(0, 0, 0)) {
            View (
                width: 8,
                height: ${ if layout.expanded() { 4 } else { 2 } },
                bg_color: mirui::types::Color::rgb(0, 0, 255)
            ) on Tap(expand_before_next_hit::callback(layout.clone()))
            View (width: 8, height: 2, bg_color: mirui::types::Color::rgb(0, 255, 0)) on Tap(record_tap::callback())
        }
    }
}

#[test]
fn later_input_hits_layout_changed_by_an_earlier_callback_in_the_same_burst() {
    use mirui::input::event::sim::{SimCommand, SimulatedInput, sim_input_system};
    for simulated in [false, true] {
        let mut app = app([]);
        app.with_default_widgets();
        let root = app.spawn_root().id();
        let layout = app.add_model(HitLayout { expanded: false }).finish();
        app.compose(root, moving_targets(layout));
        app.render().unwrap();
        let events = [
            InputEvent::PointerDown {
                kind: mirui::input::event::input::PointerKind::Mouse,
                id: 0,
                x: 1.into(),
                y: 1.into(),
            },
            InputEvent::PointerUp {
                kind: mirui::input::event::input::PointerKind::Mouse,
                id: 0,
                x: 1.into(),
                y: 1.into(),
            },
            InputEvent::PointerDown {
                kind: mirui::input::event::input::PointerKind::Mouse,
                id: 0,
                x: 1.into(),
                y: 5.into(),
            },
            InputEvent::PointerUp {
                kind: mirui::input::event::input::PointerKind::Mouse,
                id: 0,
                x: 1.into(),
                y: 5.into(),
            },
        ];
        if simulated {
            app.add_resource(SimulatedInput::new(
                events
                    .into_iter()
                    .map(|event| SimCommand::at(0, event))
                    .collect(),
            ));
            app.add_system(sim_input_system::system());
        } else {
            app.backend.events.extend(events);
        }
        app.tick();
        assert_eq!(
            app.world.resource::<Trace>().unwrap().steps(),
            &[Step::StartHook, Step::LayoutChanged, Step::Tap],
            "simulated={simulated}"
        );
        app.render_dirty().unwrap();
        let texture = app.backend.framebuffer();
        let pixels = texture.read_pixels().expect("read pixels");
        assert_eq!(pixels.get_pixel(1, 3), mirui::types::Color::rgb(0, 0, 255));
        assert_eq!(pixels.get_pixel(1, 5), mirui::types::Color::rgb(0, 255, 0));
    }
}

#[mirui::model]
impl InputGate {
    fn dismiss(&mut self) {
        self.visible = false;
    }
}

#[mirui::event]
fn dismiss_on_tap(gate: model!(InputGate), trace: res!(mut Trace)) {
    gate.dismiss();
    trace.push(Step::Tap);
}

#[test]
fn reactive_hiding_from_one_tap_is_visible_to_the_next_tap_in_the_burst() {
    use mirui::core::model::SharedValue;
    use mirui::input::event::sim::{SimCommand, SimulatedInput, sim_input_system};

    for simulated in [false, true] {
        let mut app = app([]);
        app.with_default_widgets();
        let root = app.spawn_root().id();
        let gate = app.add_model(InputGate { visible: true }).finish();
        let shared = gate.share();
        app.compose(root, |cx| {
            mirui::ui! {
                if ${ shared.visible() } {
                    View (width: 8, height: 8) on Tap(dismiss_on_tap::callback(shared.clone()))
                }
            }
        });
        app.render().unwrap();
        let input = [
            InputEvent::PointerDown {
                kind: mirui::input::event::input::PointerKind::Mouse,
                id: 0,
                x: Fixed::from_int(2),
                y: Fixed::from_int(2),
            },
            InputEvent::PointerUp {
                kind: mirui::input::event::input::PointerKind::Mouse,
                id: 0,
                x: Fixed::from_int(2),
                y: Fixed::from_int(2),
            },
        ];
        if simulated {
            app.add_resource(SimulatedInput::new(
                input
                    .iter()
                    .chain(input.iter())
                    .cloned()
                    .map(|event| SimCommand::at(0, event))
                    .collect(),
            ));
            app.add_system(sim_input_system::system());
        } else {
            app.backend
                .events
                .extend(input.iter().chain(input.iter()).cloned());
        }
        app.tick();
        assert!(!gate.visible());
        assert_eq!(
            app.world
                .resource::<Trace>()
                .unwrap()
                .steps()
                .iter()
                .filter(|step| **step == Step::Tap)
                .count(),
            1,
            "simulated={simulated}"
        );
    }
}

#[test]
fn native_and_scheduled_input_deliver_taps_before_the_following_key() {
    use mirui::input::event::sim::{SimCommand, SimulatedInput, sim_input_system};

    for simulated in [false, true] {
        let input = [
            InputEvent::PointerDown {
                kind: mirui::input::event::input::PointerKind::Mouse,
                id: 0,
                x: Fixed::from_int(2),
                y: Fixed::from_int(2),
            },
            InputEvent::PointerUp {
                kind: mirui::input::event::input::PointerKind::Mouse,
                id: 0,
                x: Fixed::from_int(2),
                y: Fixed::from_int(2),
            },
            InputEvent::Key {
                physical: None,
                code: 42,
                pressed: true,
                modifiers: Some(Default::default()),
                repeat: Some(false),
            },
        ];
        let mut app = app([]);
        app.with_default_widgets();
        let root = app.spawn_root().id();
        app.compose(root, |cx| {
            mirui::ui! {
                View (width: 8, height: 8) on Tap(record_tap::callback())
            }
        });
        app.on_event(Key::EVENT, key::callback());
        app.render().unwrap();
        if simulated {
            app.add_resource(SimulatedInput::new(
                input
                    .into_iter()
                    .map(|event| SimCommand::at(0, event))
                    .collect(),
            ));
            app.add_system(sim_input_system::system());
        } else {
            app.backend.events.extend(input);
        }
        app.tick();
        let trace = app.world.resource::<Trace>().unwrap();
        assert!(
            trace
                .steps()
                .iter()
                .copied()
                .filter(|step| matches!(step, Step::Tap | Step::Key(_)))
                .eq([
                    Step::Tap,
                    Step::Key(Key {
                        physical: None,
                        code: 42,
                        pressed: true,
                        modifiers: Some(Default::default()),
                        repeat: Some(false),
                    }),
                ]),
            "simulated={simulated}: {:?}",
            trace.steps()
        );
    }
}

#[mirui::event]
fn viewport_changed(
    event: event!(ViewportChanged),
    info: res!(DisplayInfo),
    trace: res!(mut Trace),
) {
    assert_eq!((info.width, info.height), event.new.logical_size());
    assert_eq!(info.scale, event.new.scale());
    trace.push(Step::Viewport(*event));
}

#[mirui::event]
fn safe_area_changed(
    event: event!(SafeAreaChanged),
    insets: res!(SafeAreaInsets),
    trace: res!(mut Trace),
) {
    assert_eq!(*insets, event.new);
    trace.push(Step::SafeArea(*event));
}

#[test]
fn surface_geometry_publishes_before_input_without_business_capacity_or_duplicate_notices() {
    let mut app = app([]);
    app.with_event_capacity(mirui::input::event::EventCapacity::new());
    app.on_event(ViewportChanged::EVENT, viewport_changed::callback())
        .on_event(SafeAreaChanged::EVENT, safe_area_changed::callback())
        .on_event(Key::EVENT, key::callback());
    app.tick();
    assert_eq!(
        app.world.resource::<Trace>().unwrap().steps(),
        &[Step::StartHook]
    );
    let old = app.backend.viewport();
    let new = Viewport::new(64, 32, Fixed::from_int(2));
    let safe_area = SafeAreaInsets {
        top: Fixed::from_int(3),
        ..SafeAreaInsets::default()
    };
    app.backend.pending_geometry = Some((new, safe_area));
    app.backend.events.push_back(InputEvent::Key {
        physical: None,
        code: 7,
        pressed: true,
        modifiers: Some(Default::default()),
        repeat: Some(false),
    });
    app.tick();
    let steps = app.world.resource::<Trace>().unwrap().steps();
    assert_eq!(steps[1], Step::Viewport(ViewportChanged { old, new }));
    assert_eq!(
        steps[2],
        Step::SafeArea(SafeAreaChanged {
            old: SafeAreaInsets::default(),
            new: safe_area
        })
    );
    assert!(matches!(steps[3], Step::Key(_)));
    assert_eq!(steps.len(), 4);
    app.tick();
    assert_eq!(app.world.resource::<Trace>().unwrap().steps().len(), 4);
}

#[test]
fn surface_geometry_changes_without_input_are_not_replayed_to_later_listeners() {
    let mut app = app([]);
    let middle = Viewport::new(24, 16, Fixed::ONE);
    app.backend.pending_geometry = Some((middle, SafeAreaInsets::default()));
    app.tick();
    app.on_event(ViewportChanged::EVENT, viewport_changed::callback());
    app.tick();
    assert_eq!(
        app.world.resource::<Trace>().unwrap().steps(),
        &[Step::StartHook]
    );
    // The physical mapping changes even when logical dimensions are unchanged.
    let new = Viewport::new(48, 32, Fixed::from_int(2));
    app.backend.pending_geometry = Some((new, SafeAreaInsets::default()));
    app.tick();
    assert_eq!(
        app.world.resource::<Trace>().unwrap().steps(),
        &[
            Step::StartHook,
            Step::Viewport(ViewportChanged { old: middle, new }),
        ]
    );
}

#[mirui::event]
fn host_focus(event: event!(HostFocusChanged), trace: res!(mut Trace)) {
    trace.push(Step::HostFocus(event.focused));
}

#[mirui::event]
fn host_visibility(event: event!(HostVisibilityChanged), trace: res!(mut Trace)) {
    trace.push(Step::HostVisibility(event.visible));
}

#[test]
fn host_state_is_ordered_deduplicated_and_independent_of_suspension() {
    let mut app = app([]);
    app.with_event_capacity(mirui::input::event::EventCapacity::new());
    app.add_plugin(AutoSuspendPlugin);
    app.on_event(HostFocusChanged::EVENT, host_focus::callback())
        .on_event(HostVisibilityChanged::EVENT, host_visibility::callback())
        .on_event(Key::EVENT, key::callback());
    app.tick();
    assert_eq!(
        app.world.resource::<Trace>().unwrap().steps(),
        &[Step::StartHook]
    );
    app.backend.events.extend([
        InputEvent::HostFocus { focused: true },
        InputEvent::HostFocus { focused: true },
        InputEvent::HostVisibility { visible: false },
        InputEvent::HostVisibility { visible: false },
        InputEvent::HostFocus { focused: false },
        InputEvent::Key {
            physical: None,
            code: 7,
            pressed: true,
            modifiers: Some(Default::default()),
            repeat: Some(false),
        },
    ]);
    app.tick();
    assert!(!app.is_suspended());
    assert_eq!(
        app.world.resource::<Trace>().unwrap().steps(),
        &[
            Step::StartHook,
            Step::HostFocus(true),
            Step::HostVisibility(false),
            Step::HostFocus(false),
            Step::Key(Key {
                physical: None,
                code: 7,
                pressed: true,
                modifiers: Some(Default::default()),
                repeat: Some(false),
            }),
        ]
    );
    app.suspend();
    app.backend.events.extend([
        InputEvent::HostVisibility { visible: true },
        InputEvent::HostFocus { focused: true },
    ]);
    app.tick();
    assert!(app.is_suspended());
    assert!(
        app.world
            .resource::<Trace>()
            .unwrap()
            .steps()
            .ends_with(&[Step::HostVisibility(true), Step::HostFocus(true)])
    );
}

#[test]
fn host_state_is_not_replayed_to_later_listeners() {
    let mut app = app([
        InputEvent::HostFocus { focused: true },
        InputEvent::HostVisibility { visible: true },
    ]);
    app.tick();
    app.on_event(HostFocusChanged::EVENT, host_focus::callback())
        .on_event(HostVisibilityChanged::EVENT, host_visibility::callback());
    app.backend.events.extend([
        InputEvent::HostFocus { focused: true },
        InputEvent::HostVisibility { visible: true },
    ]);
    app.tick();
    assert_eq!(
        app.world.resource::<Trace>().unwrap().steps(),
        &[Step::StartHook]
    );
    app.backend.events.extend([
        InputEvent::HostFocus { focused: false },
        InputEvent::HostVisibility { visible: false },
    ]);
    app.tick();
    assert_eq!(
        app.world.resource::<Trace>().unwrap().steps(),
        &[
            Step::StartHook,
            Step::HostFocus(false),
            Step::HostVisibility(false)
        ]
    );
}

#[test]
fn host_loss_cancels_active_drag_before_the_notice_and_late_release() {
    for input in [
        InputEvent::HostFocus { focused: false },
        InputEvent::HostVisibility { visible: false },
    ] {
        let mut app = app([]);
        app.with_default_widgets();
        app.on_event(HostFocusChanged::EVENT, host_focus::callback())
            .on_event(HostVisibilityChanged::EVENT, host_visibility::callback());
        let root = app.spawn_root().id();
        app.compose(root, |cx| {
            mirui::ui! {
                View (width: 8, height: 8) on DragStart(notice::callback(Step::DragStarted)) on DragCancel(notice::callback(Step::DragCancelled)) on DragEnd(notice::callback(Step::DragEnded))
            }
        });
        app.render().unwrap();
        app.backend.events.extend([
            InputEvent::PointerDown {
                kind: mirui::input::event::input::PointerKind::Mouse,
                id: 0,
                x: 2.into(),
                y: 2.into(),
            },
            InputEvent::PointerMove {
                kind: mirui::input::event::input::PointerKind::Mouse,
                id: 0,
                x: 50.into(),
                y: 2.into(),
            },
            input.clone(),
            InputEvent::PointerUp {
                kind: mirui::input::event::input::PointerKind::Mouse,
                id: 0,
                x: 50.into(),
                y: 2.into(),
            },
        ]);
        app.tick();
        let notice = match input {
            InputEvent::HostFocus { .. } => Step::HostFocus(false),
            _ => Step::HostVisibility(false),
        };
        assert_eq!(
            app.world.resource::<Trace>().unwrap().steps(),
            &[
                Step::StartHook,
                Step::DragStarted,
                Step::DragCancelled,
                notice,
            ]
        );
    }
}

#[mirui::event]
fn press_cancelled(event: event!(PressCancelled), trace: res!(mut Trace)) {
    trace.push(Step::PressCancelled(event.reason));
}

#[mirui::compose]
fn press_target() -> mirui::ecs::Entity {
    mirui::ui! {
        View (width: 8, height: 8) on Tap(notice::callback(Step::Tap)) on Pressed(notice::callback(Step::Pressed)) on Released(notice::callback(Step::Released)) on PressCancelled(press_cancelled::callback())
    }
}

#[test]
fn raw_press_lifecycle_precedes_tap_and_host_notifications() {
    use mirui::input::event::input::PointerKind;
    for end in [
        None,
        Some(InputEvent::HostFocus { focused: false }),
        Some(InputEvent::AppSuspend),
    ] {
        let mut app = app([]);
        app.with_default_widgets();
        app.on_event(HostFocusChanged::EVENT, host_focus::callback());
        let root = app.spawn_root().id();
        app.compose(root, press_target());
        app.render().unwrap();
        app.backend.events.push_back(InputEvent::PointerDown {
            kind: PointerKind::Mouse,
            id: 0,
            x: 2.into(),
            y: 2.into(),
        });
        if let Some(event) = &end {
            app.backend.events.push_back(event.clone());
        }
        app.backend.events.push_back(InputEvent::PointerUp {
            kind: PointerKind::Mouse,
            id: 0,
            x: 2.into(),
            y: 2.into(),
        });
        app.tick();
        let trace = app.world.resource::<Trace>().unwrap().steps();
        match end {
            None => assert_eq!(
                trace,
                &[Step::StartHook, Step::Pressed, Step::Released, Step::Tap]
            ),
            Some(InputEvent::HostFocus { .. }) => assert_eq!(
                trace,
                &[
                    Step::StartHook,
                    Step::Pressed,
                    Step::PressCancelled(PressCancelReason::Platform),
                    Step::HostFocus(false)
                ]
            ),
            _ => {
                assert_eq!(
                    &trace[..3],
                    &[
                        Step::StartHook,
                        Step::Pressed,
                        Step::PressCancelled(PressCancelReason::Suspended)
                    ]
                );
                assert!(!trace.contains(&Step::Released) && !trace.contains(&Step::Tap));
            }
        }
    }
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
fn char_input(input: event!(TextCommitted), trace: res!(mut Trace)) -> bool {
    trace.push(Step::TextCommitted(input.text().len()));
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
        physical: None,
        code: 42,
        pressed: true,
        modifiers: Some(KeyModifiers {
            shift: true,
            control: false,
            alt: true,
            meta: true,
        }),
        repeat: Some(true),
    };
    let text = TextCommitted::new("界");
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
        .on_event(TextCommitted::EVENT, char_input::callback())
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
            Step::TextCommitted("界".len()),
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
        physical: None,
        code: 42,
        pressed: true,
        modifiers: Some(Default::default()),
        repeat: Some(false),
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
        physical: None,
        code: 19,
        pressed: true,
        modifiers: Some(Default::default()),
        repeat: Some(false),
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
        physical: None,
        code: 23,
        pressed: true,
        modifiers: Some(Default::default()),
        repeat: Some(false),
    };
    let mut app = app([input.into()]);
    let local = app.add_model(InstallModel { value: 0 }).finish();
    let mut foreign_app = App::headless(8, 8);
    let foreign = foreign_app.add_model(InstallModel { value: 0 }).finish();
    let ended = {
        let mut owner = App::headless(8, 8);
        owner.add_model(InstallModel { value: 0 }).finish()
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
