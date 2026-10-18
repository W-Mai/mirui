//! Text-input sessions and application-provided input methods.

use alloc::{borrow::Cow, collections::VecDeque, rc::Rc};
use core::{
    cell::{Cell, RefCell},
    ops::Range,
};

use super::widgets::{
    TextEditing,
    text_editing::edit_target::{EditTarget, EditorIdentity},
};
use crate::ecs::commands::DeferredCommands;
use crate::ecs::world::WorldId;
use crate::ecs::{CommandCapacity, Entity, World};
use crate::input::event::callback::{AppContext, EventCallback, PreparedEvent};
use crate::input::event::contract::{
    AppNotification, AppSource, Event, InputNotification, InputSource,
};
use crate::input::event::focus::FocusState;
use crate::input::event::input::{Key, TextCommitted};
use crate::input::event::listeners::InputBranch;
use crate::input::event::{BubbleControl, runtime};
use crate::types::Rect;

mod registration;
pub use registration::{InputMethod, InputMethodCallbacks};

/// Selected producer of text for the focused editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextInputMode {
    Native,
    Software,
}

/// Identity of one editor and input source, issued by the App.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextInputSession {
    owner: WorldId,
    generation: u64,
}

impl TextInputSession {
    /// Capture a native preview with a selection expressed in UTF-8 byte offsets.
    pub fn preedit(
        self,
        text: impl Into<Cow<'static, str>>,
        selection: Option<Range<usize>>,
    ) -> crate::surface::InputEvent {
        TextInputEvent(Request::Preedit(
            self,
            CompositionUpdated {
                text: text.into(),
                selection,
            },
        ))
        .into()
    }

    /// Capture a native text commit without copying an owned string.
    pub fn commit(self, text: impl Into<Cow<'static, str>>) -> crate::surface::InputEvent {
        TextInputEvent(Request::Commit(self, TextCommitted::new(text))).into()
    }

    /// Capture cancellation of this session's composition.
    pub fn cancel(self) -> crate::surface::InputEvent {
        TextInputEvent(Request::Cancel(self)).into()
    }
}

/// Native text captured for a specific App-issued session.
#[derive(Clone, Debug)]
pub struct TextInputEvent(Request);

impl From<TextInputEvent> for crate::surface::InputEvent {
    fn from(event: TextInputEvent) -> Self {
        Self::TextInput(event)
    }
}

/// Current editor session and its surface-space caret, when layout is available.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextInputState {
    pub session: TextInputSession,
    pub caret: Option<Rect>,
}

/// Input-method context. A missing state clears the provider's active editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextInputChanged {
    pub state: Option<TextInputState>,
}
impl Event for TextInputChanged {
    type Route = AppNotification;
    type Source = AppSource;
    type Options = ();
}

/// A composition has begun on this editor.
#[derive(Clone, Copy, Debug)]
pub struct CompositionStarted;

/// A replacement preview. The document has not been edited.
#[derive(Clone, Debug)]
pub struct CompositionUpdated {
    text: Cow<'static, str>,
    selection: Option<Range<usize>>,
}
impl CompositionUpdated {
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn selection(&self) -> Option<Range<usize>> {
        self.selection.clone()
    }
}

/// Why a composition ended. `Committed` does not imply acceptance by the editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositionEndReason {
    Committed,
    Cancelled,
    Interrupted,
}
/// The composition's display state has been cleared.
#[derive(Clone, Copy, Debug)]
pub struct CompositionEnded {
    pub reason: CompositionEndReason,
}
macro_rules! notices {
    ($($event:ty),+) => {$(impl Event for $event {
        type Route = InputNotification;
        type Source = InputSource;
        type Options = ();
    })+};
}
notices!(CompositionStarted, CompositionUpdated, CompositionEnded);

#[derive(Clone, Debug)]
enum Request {
    Preedit(TextInputSession, CompositionUpdated),
    Commit(TextInputSession, TextCommitted),
    Cancel(TextInputSession),
    Key(TextInputSession, Key),
    Mode(TextInputMode),
}

/// Deferred requests to the current text-input session.
pub struct TextInputCommands<'borrow, 'queue>(&'borrow mut DeferredCommands<'queue>);
impl<'queue> DeferredCommands<'queue> {
    pub fn text_input(&mut self) -> TextInputCommands<'_, 'queue> {
        TextInputCommands(self)
    }
}
impl CommandCapacity {
    /// Reserve deferred text-input requests without reserving text payload buffers.
    pub const fn text_input(self, count: usize) -> Self {
        self.resource_value::<Request>(count)
    }
}
impl TextInputCommands<'_, '_> {
    pub fn preedit(
        &mut self,
        session: TextInputSession,
        text: impl Into<Cow<'static, str>>,
        selection: Option<Range<usize>>,
    ) {
        self.send(Request::Preedit(
            session,
            CompositionUpdated {
                text: text.into(),
                selection,
            },
        ));
    }
    pub fn commit(&mut self, session: TextInputSession, text: impl Into<Cow<'static, str>>) {
        self.send(Request::Commit(session, TextCommitted::new(text)));
    }
    pub fn cancel(&mut self, session: TextInputSession) {
        self.send(Request::Cancel(session));
    }
    pub fn key(&mut self, session: TextInputSession, key: Key) {
        self.send(Request::Key(session, key));
    }
    pub fn set_mode(&mut self, mode: TextInputMode) {
        self.send(Request::Mode(mode));
    }
    fn send(&mut self, request: Request) {
        self.0.apply_resource(request, enqueue);
    }
}

#[derive(Clone)]
struct Active {
    state: TextInputState,
    entity: Entity,
    identity: EditorIdentity,
    composition: Option<EditTarget>,
    invalid: bool,
}
struct State {
    mode: TextInputMode,
    generation: u64,
    active: Option<Active>,
    changed: Option<PreparedEvent<TextInputChanged, AppContext>>,
    key: Option<PreparedEvent<Key, AppContext>>,
    published: Option<Option<TextInputState>>,
    pending: VecDeque<Request>,
    capacity: usize,
    unavailable: u8,
    committing: Option<(Entity, EditTarget)>,
}
struct Service {
    state: RefCell<State>,
    delivering: Cell<bool>,
}
impl Service {
    fn new(unavailable: u8) -> Self {
        Self {
            state: RefCell::new(State {
                mode: TextInputMode::Native,
                generation: 0,
                active: None,
                changed: None,
                key: None,
                published: None,
                pending: VecDeque::new(),
                capacity: 0,
                unavailable,
                committing: None,
            }),
            delivering: Cell::new(false),
        }
    }
}
fn service(world: &World) -> Option<Rc<Service>> {
    world.resource::<Rc<Service>>().cloned()
}

fn enqueue(world: &mut World, request: Request) {
    let Some(service) = service(world) else {
        if !matches!(request, Request::Mode(TextInputMode::Native)) {
            crate::warn!(target: "mirui::input", "text-input request has no installed provider");
        }
        return;
    };
    let mut state = service.state.borrow_mut();
    if state.pending.len() == state.capacity {
        crate::warn!(target: "mirui::input", "text-input request capacity exhausted");
    } else {
        state.pending.push_back(request);
    }
}

fn focused(world: &World) -> Option<Entity> {
    let entity = world.resource::<FocusState>()?.focused?;
    InputBranch::scan(world, entity)
        .ok()?
        .allowed()
        .then_some(entity)
}

/// Record invalidation without entering another callback while a dispatch snapshot is borrowed.
pub(crate) fn observe(world: &World) {
    let Some(service) = service(world) else {
        return;
    };
    let mut state = service.state.borrow_mut();
    if let Some(active) = &mut state.active {
        active.invalid |= focused(world) != Some(active.entity)
            || active.identity.resolve(world).is_none()
            || active
                .composition
                .as_ref()
                .is_some_and(|target| target.after_edit(world, false).is_none());
    }
}

pub(crate) fn host_input(world: &World, event: &crate::input::event::input::InputEvent) {
    use crate::input::event::input::InputEvent;
    let Some(service) = service(world) else {
        return;
    };
    let (mask, unavailable) = match event {
        InputEvent::HostFocus { focused } => (1, !focused),
        InputEvent::HostVisibility { visible } => (2, !visible),
        InputEvent::AppSuspend => (4, true),
        InputEvent::AppResume => (4, false),
        _ => return,
    };
    let mut state = service.state.borrow_mut();
    if unavailable {
        state.unavailable |= mask;
    } else {
        state.unavailable &= !mask;
    }
    if unavailable && let Some(active) = &mut state.active {
        active.invalid = true;
    }
}

fn notice<E: Event<Route = InputNotification>>(world: &mut World, entity: Entity, event: &E) {
    if let Err(error) = runtime::input_notice(world, entity, event) {
        crate::warn!(target: "mirui::input", "composition notification skipped: {:?}", error);
    }
}

fn clear(world: &mut World, active: &Active) {
    if active.identity.resolve(world).is_some()
        && let Some(editor) = world.get_mut::<TextEditing>(active.entity)
    {
        editor.clear_preedit();
    }
}

fn end(world: &mut World, active: &mut Active, reason: CompositionEndReason) {
    if active.composition.take().is_some() {
        clear(world, active);
        if active.identity.resolve(world).is_some() {
            notice(world, active.entity, &CompositionEnded { reason });
        }
    }
}

pub(crate) struct InterruptedEdit(Option<Active>);

pub(crate) fn interrupt_edit(world: &mut World, entity: Entity) -> InterruptedEdit {
    let retired = service(world).and_then(|service| {
        let mut state = service.state.borrow_mut();
        if state
            .active
            .as_ref()
            .is_some_and(|active| active.entity == entity)
        {
            state.active.take()
        } else {
            None
        }
    });
    if let Some(editor) = world.get_mut::<TextEditing>(entity) {
        editor.clear_preedit();
    }
    InterruptedEdit(retired)
}

impl InterruptedEdit {
    pub(crate) fn finish(self, world: &mut World) {
        if let Some(active) = self.0
            && active.composition.is_some()
            && active.identity.resolve(world).is_some()
        {
            notice(
                world,
                active.entity,
                &CompositionEnded {
                    reason: CompositionEndReason::Interrupted,
                },
            );
        }
    }
}

fn reconcile(world: &mut World, service: &Service) {
    observe(world);
    let retired = {
        let mut state = service.state.borrow_mut();
        if state.active.as_ref().is_some_and(|active| active.invalid) {
            state.active.take()
        } else {
            None
        }
    };
    if let Some(mut retired) = retired {
        end(world, &mut retired, CompositionEndReason::Interrupted);
    }
    let mut state = service.state.borrow_mut();
    if state.active.is_none()
        && state.unavailable == 0
        && let Some(entity) = focused(world)
        && let Some(identity) = EditorIdentity::capture(world, entity)
        && let Some(generation) = state.generation.checked_add(1)
    {
        state.generation = generation;
        state.active = Some(Active {
            state: TextInputState {
                session: TextInputSession {
                    owner: world.id(),
                    generation,
                },
                caret: None,
            },
            entity,
            identity,
            composition: None,
            invalid: false,
        });
    }
    if let Some(active) = &mut state.active {
        active.state.caret =
            super::widgets::text_editing::geometry::caret_rect(world, active.entity);
    }
    let next = (state.mode == TextInputMode::Software)
        .then(|| state.active.as_ref().map(|active| active.state))
        .flatten();
    let changed = (state.published != Some(next))
        .then(|| state.changed.clone())
        .flatten();
    state.published = Some(next);
    drop(state);
    if let Some(changed) = changed
        && let Err(error) = changed.invoke(world, &TextInputChanged { state: next }, ())
    {
        crate::warn!(target: "mirui::input", "input-method notification skipped: {:?}", error);
    }
}

fn deliver(world: &mut World, service: &Service, request: &Request) {
    observe(world);
    if let Request::Mode(mode) = request {
        let mut state = service.state.borrow_mut();
        if state.mode == *mode {
            return;
        }
        if *mode == TextInputMode::Software && state.changed.is_none() {
            crate::warn!(target: "mirui::input", "no software input method installed");
            return;
        }
        state.mode = *mode;
        let retired = state.active.take();
        drop(state);
        if let Some(mut active) = retired {
            end(world, &mut active, CompositionEndReason::Interrupted);
        }
        return;
    }
    let session = match &request {
        Request::Preedit(session, _)
        | Request::Commit(session, _)
        | Request::Cancel(session)
        | Request::Key(session, _) => *session,
        Request::Mode(_) => unreachable!(),
    };
    let state = service.state.borrow();
    if state
        .active
        .as_ref()
        .is_none_or(|active| active.invalid || active.state.session != session)
    {
        return;
    }
    let mut active = state.active.as_ref().unwrap().clone();
    drop(state);
    match request {
        Request::Preedit(_, update) => {
            if update.text.is_empty() && active.composition.is_none() {
                return;
            }
            let valid = update.selection.as_ref().is_none_or(|range| {
                range.start <= range.end
                    && update.text.is_char_boundary(range.start)
                    && update.text.is_char_boundary(range.end)
            });
            if valid {
                if active.composition.is_none() {
                    active.composition = EditTarget::capture(world, active.entity);
                    service
                        .state
                        .borrow_mut()
                        .active
                        .as_mut()
                        .unwrap()
                        .composition = active.composition.clone();
                    notice(world, active.entity, &CompositionStarted);
                }
                observe(world);
                if service
                    .state
                    .borrow()
                    .active
                    .as_ref()
                    .is_some_and(|active| !active.invalid)
                    && active
                        .composition
                        .as_ref()
                        .is_some_and(|target| target.after_edit(world, false).is_some())
                {
                    notice(world, active.entity, update);
                }
            } else {
                crate::warn!(target: "mirui::input", "invalid preedit UTF-8 selection");
            }
        }
        Request::Commit(_, text) => {
            clear(world, &active);
            let composition = active.composition.take();
            let composing = composition.is_some();
            {
                let mut state = service.state.borrow_mut();
                state.active.as_mut().unwrap().composition = None;
                state.committing = composition.map(|target| (active.entity, target));
            }
            if let Err(error) = runtime::keyboard_from(world, active.entity, text) {
                crate::warn!(target: "mirui::input", "text commit skipped: {:?}", error);
            }
            service.state.borrow_mut().committing = None;
            if let Err(error) = runtime::flush(world, super::lifecycle::prepare) {
                crate::warn!(target: "mirui::input", "commit notifications skipped: {:?}", error);
            }
            if composing && active.identity.resolve(world).is_some() {
                notice(
                    world,
                    active.entity,
                    &CompositionEnded {
                        reason: CompositionEndReason::Committed,
                    },
                );
            }
        }
        Request::Cancel(_) => end(world, &mut active, CompositionEndReason::Cancelled),
        Request::Key(_, key) => {
            let delivery =
                super::delivery::KeyboardDelivery::capture(world, Some(active.entity), *key);
            if let Err(error) = delivery.deliver(world) {
                crate::warn!(target: "mirui::input", "editing key skipped: {:?}", error);
            }
        }
        Request::Mode(_) => unreachable!(),
    }
    if let Some(current) = service.state.borrow_mut().active.as_mut()
        && current.state.session == session
    {
        current.composition = active.composition;
    }
}

pub(crate) fn flush(world: &mut World) {
    let Some(service) = service(world) else {
        return;
    };
    if service.delivering.replace(true) {
        return;
    }
    struct Guard<'a>(&'a Cell<bool>);
    impl Drop for Guard<'_> {
        fn drop(&mut self) {
            self.0.set(false);
        }
    }
    let _guard = Guard(&service.delivering);
    let limit = runtime::delivery_limits(world).flush_limit.unwrap_or(0);
    for _ in 0..limit {
        super::lifecycle::prepare(world);
        reconcile(world, &service);
        let request = service.state.borrow_mut().pending.pop_front();
        let Some(request) = request else {
            if service
                .state
                .borrow()
                .active
                .as_ref()
                .is_some_and(|active| active.invalid)
            {
                continue;
            }
            return;
        };
        if matches!(request, Request::Mode(_))
            || service.state.borrow().mode == TextInputMode::Software
        {
            deliver(world, &service, &request);
        }
        if let Err(error) = runtime::flush(world, super::lifecycle::prepare) {
            crate::warn!(target: "mirui::input", "editing notifications skipped: {:?}", error);
        }
    }
    reconcile(world, &service);
    if !service.state.borrow().pending.is_empty() {
        crate::warn!(target: "mirui::input", "text-input delivery limit reached");
        service.state.borrow_mut().pending.clear();
    }
}

pub(crate) fn intercept_key(world: &mut World, key: &Key) -> bool {
    let Some(service) = service(world) else {
        return false;
    };
    let state = service.state.borrow();
    let native_composition = state.mode == TextInputMode::Native
        && state
            .active
            .as_ref()
            .is_some_and(|active| active.composition.is_some());
    if native_composition && key.selection_shortcut().is_none() {
        return true;
    }
    let callback = (state.mode == TextInputMode::Software && state.active.is_some())
        .then(|| state.key.clone())
        .flatten();
    drop(state);
    callback.is_some_and(|callback| match callback.invoke(world, key, ()) {
        Ok(Some(BubbleControl::Prevent)) => true,
        Ok(_) => false,
        Err(error) => {
            crate::warn!(target: "mirui::input", "input-method key skipped: {:?}", error);
            false
        }
    })
}

pub(crate) fn software_selected(world: &World) -> bool {
    service(world).is_some_and(|service| service.state.borrow().mode == TextInputMode::Software)
}

pub(crate) fn native_state(world: &mut World, unavailable: u8) -> Option<TextInputState> {
    super::lifecycle::prepare(world);
    if service(world).is_none() {
        let entity = focused(world)?;
        EditorIdentity::capture(world, entity)?;
        world.insert_resource(Rc::new(Service::new(unavailable)));
    }
    flush(world);
    let service = service(world)?;
    let state = service.state.borrow();
    (state.mode == TextInputMode::Native)
        .then(|| state.active.as_ref().map(|active| active.state))
        .flatten()
}

pub(crate) fn deliver_native(world: &mut World, event: &TextInputEvent) {
    let Some(service) = service(world) else {
        return;
    };
    if service.state.borrow().mode != TextInputMode::Native {
        return;
    }
    deliver(world, &service, &event.0);
}

pub(crate) fn accepts_commit(world: &World, entity: Entity) -> bool {
    let Some(service) = service(world) else {
        return true;
    };
    let state = service.state.borrow();
    state.committing.as_ref().is_none_or(|(origin, target)| {
        *origin == entity
            && state.active.as_ref().is_some_and(|active| !active.invalid)
            && target.after_edit(world, false).is_some()
    })
}

#[cfg(test)]
mod tests;
