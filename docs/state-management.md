# Reactive State

mirui has a reactive state layer — signals, computed values, and effects —
that drives widget attributes and structure declaratively from the `ui!`
macro. Change a signal and the widgets that read it update; no manual
diffing, no observer wiring.

This guide is the long-form companion to the `mirui::core::reactive` API docs and
the State demos in the gallery.

## Contents

1. [Registered model instances](#registered-model-instances)
2. [Event type contracts](#event-type-contracts)
3. [Choosing state and rendering roles](#choosing-state-and-rendering-roles)
4. [Primitives](#primitives)
5. [Reactive attributes](#reactive-attributes)
6. [Layout-responsive attributes](#layout-responsive-attributes)
7. [Reactive control flow](#reactive-control-flow)
8. [Lists: `walk`, index vs keyed](#lists)
9. [The flush model](#the-flush-model)
10. [Limits](#limits)

## Registered model instances

Place `#[model]` on a struct and one inherent impl to generate its instance
handle and forwarded methods. Each `App::add_model` call registers an independent
instance; cloning a handle refers to the same instance without cloning its data.

```rust
use mirui::prelude::*;

#[model]
struct Counter {
    #[observe]
    count: u32,
}

#[model]
impl Counter {
    #[observe]
    fn is_even(&self) -> bool { self.count % 2 == 0 }

    fn increment(&mut self) { self.count += 1; }
    fn decrement(&mut self) { self.count = self.count.saturating_sub(1); }
}

let mut app = App::headless(32, 32);
let counter = app.add_model(Counter { count: 0 });
counter.increment();
```

Read-only model methods can return owned values; a generated handle cannot
return a reference into its temporarily borrowed model. Mutating methods in a
marked impl return the model's change type (`()` unless `change` is declared),
or `Result<Change, E>` for a command that can be rejected. A successful command
publishes observations, named revisions, and effects normally. An error is
returned unchanged without publishing observations or revisions, and any
pending effects are discarded. Validate before modifying the model: rejected
commands are not a rollback mechanism. Use the canonical `Result` path rather
than a type alias so the macro can identify the transaction boundary. Helpers
returning another type remain in an ordinary impl and are not forwarded to the
handle. An impl in another module uses the model's qualified path, such as
`#[model] impl super::data::Counter`.
Observed fields, observed getters, and effect extractors can use `#[cfg]`;
their generated accessors and source storage follow the same condition.

`#[observe]` creates a handle getter for a small `Copy + Eq` field. Reading
`counter.count()` inside a reactive UI binding subscribes that binding to the
field. Model methods compare observed values before and after each update;
unchanged values do not notify subscribers. Large buffers and other
non-`Copy` fields remain ordinary model data.
An `#[observe]` method with `&self`, no arguments, and a `Copy + Eq` return
value works the same way: `counter.is_even()` notifies only when its computed
result changes.

For changes that cannot be observed as a small value, a model can declare a
change type and named masks with
`#[model(change = ChangeSet, watch(visual = ChangeSet::VISUAL))]`.
A mutating model method returns `ChangeSet`; a result containing `VISUAL`
increments `model.visual_revision()` and notifies its readers. A result with
no matching mask still publishes changed `#[observe]` values. The change type
must provide `contains(mask)`; each named mask is checked independently.

Fixed-size effect arrays can be drained after a model method completes:

```rust
#[effects]
fn take_notes(&mut self) -> [Option<Note>; 4] {
    core::mem::take(&mut self.notes)
}

app.on_effect(&player, move |note: Note| play_note(note))?;
```

The extractor lives in the marked model impl and is not exposed as a handle
method. One consumer may be registered per effect type and model instance.
Events are drained even when no consumer is installed; delivery runs after
the model borrow is released. Consumers can read committed model state, but
cannot start another model write transaction during delivery.

Model roles keep generated handle types out of application code. A
`model!(T)` field in a `#[component]` stores one registered instance, while a
`model!(T)` parameter in a `#[compose]` function receives one:

```rust
#[component]
#[derive(Clone)]
struct CounterTile {
    counter: model!(Counter),
}

#[compose]
fn counter_value(counter: model!(Counter), label: &'static str) -> Entity {
    ui! {
        Text(text: ${ format!("{label}: {}", counter.count()) })
    }
}

#[compose]
fn counter_controls(counter: model!(Counter)) {
    ui! {
        Row {
            Button(text: "−") on Tap { counter.decrement(); }
            Button(text: "+") on Tap { counter.increment(); }
            counter_value(counter, "count")
        }
    };
}
```

`CounterTile` can be cloned even though `Counter` is not `Clone`. Ordinary
fields and parameters retain their declared Rust types. Generated `ui!`
callbacks share a model role when they read it. Direct child compose calls,
including calls inside niches, also share a model role when its name is passed
directly. Multiple callbacks or children can therefore use one instance
without spelling a generated handle type or manually cloning it. Nested
argument expressions retain Rust move semantics; use `counter.clone()` inside
them when the value is needed again.
Implicit named references in standard `format!` and `format_args!` calls are
shared inside generated callbacks too. This applies to the bare standard macro
names and absolute `::std::` / `::alloc::` / `::core::` paths; aliases are not
analyzed. Use an explicit argument such as `format!("{}", counter.count())` in
other formatting macros. If a local macro uses one of the bare standard names,
call it through its own module path. An escaped `{counter}` or a named argument
such as `counter = 7` does not by itself read the model role; an argument
expression that uses the model still does.
This also applies when `ui!` is called as `mirui::ui!`, `::mirui::ui!`, or a
local `crate::ui!` re-export. Other arguments keep their normal move semantics.

Use `com!(entity, T)` during composition or in an event body to inspect an
attached component. The read returns `Option<&T>`, remains valid only for its
short lexical borrow, and does not subscribe to changes. A typed `#[view]`
model role with `watch(...)` is the rendering path when model changes must
invalidate a View.

Register the instance before building controls. A `model!(Counter)` compose
parameter accepts the value returned by `App::add_model`:

```rust
let root = app.spawn_root().id();
let counter = app.add_model(Counter { count: 0 });
app.compose(root, |cx| counter_controls(cx, counter.clone()));
```

The model owns business state; `#[component]` model fields attach specific
instances to entities. A component may contain several model fields, including
optional fields, and several components may share one model. Neither pattern
creates a second copy of the model data. Marble Play attaches its board and
controls to one model; the Counter demo uses the same API without a canvas,
audio output, or change mask.

Systems declare model, resource, query, and deferred-command roles explicitly.
The authored function does not receive `World`:

```rust
#[system(order = Animation)]
fn advance(
    counter: model!(Counter),
    delta: res!(mirui::ecs::DeltaTimeMs),
) {
    for _ in 0..delta.0 {
        counter.increment();
    }
}

app.add_system(advance::system(counter.clone()));
```

Each registration keeps its own model instance. Multiple `model!(T)` parameters
are passed to the generated `system(...)` constructor in source order.
`res!(T)` and `res!(mut T)` request required resources; `res!(optional T)`
requests an optional shared resource. `query!(...)` declares component access
and filters, while `commands!()` records structural changes that are applied
after the callback releases its borrows. Missing required resources and
cross-App model bindings fail explicitly. Parameter preparation and invocation
reuse scheduler-owned storage after warm-up.

Queries also support `rows.get(entity)`. A point fetch applies the same driver,
required-component, generation, and filter checks as iteration; a missing row
returns `None`. Read-only fetches can share references. A query containing
`mut T` lends its row exclusively until the returned references are released,
then permits another fetch or consuming iteration. Generated parameters supply
the local mutable binding without requiring `mut rows` in the authored signature.

```rust
#[system]
fn update_selected(
    selected: res!(SelectedEntity),
    rows: query!(mut Position, optional Velocity),
) {
    if let Some((position, velocity)) = rows.get(selected.0) {
        if let Some(velocity) = velocity {
            position.x += velocity.x;
        }
    }
}
```

Mutable point fetches use the same component-change notification path as
iteration. Repeated writes to one entity/component pair are coalesced; their
records fit the query's reserved unique-row budget.

Event bodies receive event fields directly and use capability macros for ECS
access. `com!(entity, T)` performs a short read. `commands!()` returns the
handler's reusable deferred-command facade; structural changes are applied in
source order after all short reads and the event body have ended. Component
selectors can contain another `com!` read; both use the current event context:

```rust
use mirui::input::event::BubbleControl;

#[component]
struct Selected;

#[compose]
fn selectable() -> Entity {
    ui! {
        View () on Tap {
            if com!(target, Selected).is_some() {
                commands!().remove::<Selected>(target);
            } else {
                commands!().insert(target, Selected);
            }
            BubbleControl::Allow
        }
    }
}
```

Returning `()` or `true` consumes the event. Returning `false` or
`BubbleControl::Allow` continues bubbling. Gesture bodies expose the fields of
their event variant, such as `target`, `x`, `y`, `dx`, and `dy`; widget
business events expose their named payload fields. A panic discards that
invocation's pending commands. The command buffer retains its capacity for the
next event.

`#[timer]` declares one schedule and typed callback roles. Model instances are
provided when the generated timer is registered; entity and deferred-command
roles are supplied for each firing:

```rust
#[timer(every = 1_000)]
fn tick(
    counter: model!(Counter),
    timer: entity!(),
    commands: commands!(),
) {
    counter.increment();
    commands.insert(timer, Selected);
}

app.add_timer(tick::timer(counter));
```

The schedule forms are `after = ms`, `every = ms`,
`repeat = (count, ms)`, and `until = (deadline, ms)`.

`#[animate]` declares how a scalar `Motion` is applied. It generates the named
motion component and a system constructor; the callback can receive model,
component, entity, motion, and deferred-command roles:

```rust
use mirui::anim::{Motion, Tween};

#[component]
struct Opacity(Fixed);

#[animate(component = AnimateOpacity)]
fn apply_opacity(value: motion!(), opacity: component!(mut Opacity)) {
    opacity.0 = value;
}

app.add_system(AnimateOpacity::system());

#[compose]
fn animated_opacity() -> Entity {
    ui! {
        View () [
            Opacity(Fixed::ZERO),
            AnimateOpacity(Motion::from(Tween::ease_to(
                Fixed::ZERO,
                Fixed::ONE,
                240,
            ))),
        ]
    }
}
```

`World::watch_component_type` records insertions, replacements, removals, and
despawns for selected component types. Repeated changes to one entity and type
are coalesced until `drain_component_changes` reads the final state. The queue
can be reserved ahead of a batch. Direct field writes through `World::get_mut`
do not create a structural change record; replace the complete component when
its model binding changes.

`#[view]` generates a filtered View from its component attribute and builds
model subscriptions from typed parameter roles:

```rust
#[view(component = CounterTile, priority = 60, invalidation = Visual)]
fn paint_counter(
    counter: model!(component.counter, watch(count)),
    entity: entity!(),
    paint: paint!(),
) {
    let _ = (counter.count, entity, paint.rect());
}

app.with_widget(paint_counter::view());
```

`component = T` selects the entities painted by the View.
Button, Slider, Checkbox, ProgressBar, TabBar, and Icon use this declaration
for built-in paint callbacks. Their widget modules export the generated `view`
factory directly; component filters, paint priorities, and internal gesture
hooks remain part of the descriptor.
Render Showcase and Blur Filter declare their retained scene workspace with
read-only `res!` and replay borrowed scene operations through `paint!()`.
Their setup prepares RGBA storage before painting; the View borrows that
storage rather than allocating another buffer in its callback.
`model!(component.field)` borrows that component's model only during paint;
`watch(selector)` adds an exact subscription to an `#[observe]` field, an
observed getter, or a named revision. An optional model field uses
`model!(optional component.field, watch(selector))`. Add a named
`component!()` parameter only when the paint body also needs `&T`. `res!(T)`,
`entity!()`, and `paint!()` expose resources, entity identity, and one
frame-scoped paint context. `PaintCtx` provides the current geometry, style,
widget state, transform, clip, drawing, and scene replay operations without
exposing the renderer as a separate callback parameter.
`with_clip_path` owns the matching clip push/pop pair, including cleanup after
a body draw fails. Passing raw `PushClip` or `PopClip` commands to `draw` is
rejected. Paint operations retain the first render error for the frame walker
instead of requiring callbacks to forward backend results themselves.
The generated adapter holds no model borrow after paint and rejects model
writes inside the callback. Multiple model fields, including two fields that
refer to the same instance, can be read together. `invalidation = Layout`
promotes watched changes from visual to layout invalidation; visual
invalidation is the default. `attach`, `gesture`, and static `systems` can be
forwarded to the View builder. An optional `name = "..."` keeps a stable
registration name when the paint function name differs from the widget name.
Subscriptions reserve visual notification queue capacity when attached; model
updates and notifications reuse that capacity. Component replacement and
removal update bindings before the next paint.

A View describes a widget kind, not just its paint function. Paint, attach,
gesture, observation, and systems are independent capabilities.
`View::systems_only(name, systems)` registers behavior without a paint hook;
TabPages and LazyList use this path and remain Views. Paint traversal skips
these descriptors. Paint priority orders only drawing, with registration order
breaking ties. Gesture hooks retain registration order, and contributed systems
use their own `SystemSlot`.

## Event type contracts

`input::event::contract::Event` declares a payload's route, source, and options.
`EventKey<E>` stores only the options and preserves the payload type; it does not
own a payload, a model, or a callback. A key with `Options = ()` occupies no memory.

Widget notification types use `EntityNotification` and `WidgetSource<W>`:

```rust
use mirui::input::event::contract::{EntityNotification, Event, WidgetSource};

#[mirui::component]
struct Pad;

struct Hit {
    strength: u8,
}

impl Event for Hit {
    type Route = EntityNotification;
    type Source = WidgetSource<Pad>;
    type Options = ();
}
```

The source capability accepts only `Pad`, not another widget or an App listener.
`EntityBubble` describes target-to-ancestor gestures; `InputFallback` describes
input reaching App listeners after focus or scroll handling; `AppNotification`
describes non-consumable App notifications. Source/route pairs are checked when
the payload implements `Event`.

`EventResult<R>` encodes the route's result policy. Consumable routes accept `()`,
`bool`, or `BubbleControl`; notifications accept only `()`. `EventOptions`
validates typed arguments and returns explicit errors before storing options in
an event key. Declaring a payload does not require an event-definition macro or
changes to the UI macro's event catalog.

Built-in notification payloads live in their widget modules:
`slider::ValueChanged`, `slider::DragStarted`, `slider::DragEnded`,
`switch::Toggled`, `checkbox::Toggled`, `progress_bar::ValueChanged`,
`tabbar::SelectionChanged`, and `text_input::Changed`.
The corresponding constants, such as `Slider::ValueChanged`, are typed event
keys. Qualifying a selector does not permit installing a different widget's
notification: `Switch::Toggled` retains `WidgetSource<Switch>`.

The ordinary prelude exports `Tap`, `LongPress`, `DragStart`, `DragMove`,
`DragEnd`, `DragCancel`, `Pinch`, and `Rotate`, plus `Event` and `EventKey`.
Their fields match the recognizer's gesture data, including its original target.
`GestureSelectors` provides the corresponding typed constants for any component.
Tap keys hold a validated count; the default is 1 and explicit values outside
1 through 255 return `TapCountError`. Other gesture keys have no options.

`Key` and `CharInput` carry their platform fields and support entity and App
input source capabilities. `Wheel` carries its position and deltas; `Rotary`
carries its device id and delta, without fabricated pointer coordinates.
Wheel and rotary callback sources are App-only.

SDL and WGPU preserve press and release for supported editing keys. Character
input is produced by text input or key presses, not key releases. Escape retains
its quit-request behavior; releasing it does not generate a second request.

`Started`, `Suspended`, `Resumed`, `MemoryWarning`, `HostInteraction`, and
`Quit` are App notification payloads with unit options and results. They do not
implement platform-input projection: raw suspend/resume/quit requests must not
be treated as notifications of completed lifecycle transitions.

Active input simulation suppresses competing pointer, wheel, rotary, keyboard,
and character input before widget and plugin input handlers. Suspend, resume,
and quit requests remain available to the application lifecycle path.

### Notification payload storage

Internal notification storage owns payloads in two explicitly reserved buffers.
A callback reads one stable payload window while emitted notifications append
to the other buffer. Remaining records in the current batch run before callback
writes, preserving FIFO order without recursive delivery.

Record count, payload bytes, and alignment are logical limits; unused allocator
capacity does not raise them. Writes within those limits do not allocate.
Rejected writes return the original payload, and interrupted or feedback-limited
flushes destroy pending values and restore reusable storage. Notification
references are read-only and cannot escape their invocation.

Drain callbacks return a result. Storage errors and listener errors remain
distinct; listener errors are returned without cloning or replacing the error
value. Failure discards unread and newly emitted notifications and leaves the
buffers reusable. Model and resource updates already applied by a listener are
not rolled back.

This storage facility does not install listeners, resolve widget sources, or
validate an application's owner and entity generations. Those checks belong to
event installation and routing, not the payload arena.

### Prepared callback execution

Internal typed callbacks share the system parameter fetch and access validator.
Preparation checks component/resource aliases and captured Model owners, binds
the callback to one App, and reserves the current query change-record budget.
The prepared callback and its access plan cannot be replaced independently.

Invocation checks the App identity and entity generation before fetching
parameters. Entity contexts receive an `Entity`; App contexts receive no entity.
Payloads are read-only borrows, resources and query rows cannot escape the body,
and callback results follow the payload's route contract. Snapshot copies share
the installed callback allocation. Execution within reserved capacities does not
allocate callback storage.

Deferred callbacks receive a command facade backed by installation-time storage
limits. The body records commands; every live parameter borrow ends before the
executor applies them in source order. A recording error discards that batch
and is returned from invocation. It does not roll back Model or resource values
already updated by the body. Callback and apply unwinding release pending
payloads and leave storage reusable. A busy command buffer returns an error
rather than recursively borrowing it. Plain callbacks have no command buffer.

This execution layer does not install or route listeners. Its ABI is confined
to hidden macro support, not the ordinary authoring prelude.

### Listener selection and notification delivery

The internal listener pool installs prepared callbacks for entity and App
targets. Widget notification installation checks the source component and
entity generation. Captured Model owners are validated by preparation; the
pool rejects a different App before installation or dispatch.

Listener entries and snapshot steps have independent explicit count limits.
Matching produces an installation-order snapshot without borrowing the World
or retaining a borrow of the entry list. Removing a listener does not change
the current selection; a new listener participates in later selections. Each
callback checks entity generation before running. Notification callbacks fan
out; consumable input stops on a prevention result.

Notification draining uses this pool and the same prepared executor. Payloads
are borrowed without cloning, and reserved invocation scratch is cleared on
success, error, and unwinding. Reentrant use of that scratch returns an error.
Selection and draining within reserved capacities allocate no listener storage.
Gesture selection freezes the complete Parent chain before running any body.
Each node contributes matching internal View hooks in registration order, then
its authored listeners in installation order. Reparenting, removing a filter
component, or changing listeners does not alter that selection. Each step still
checks entity generation before execution. Hidden ancestors suppress the route;
Parent cycles and insufficient step capacity fail before any body runs.

Keyboard selection freezes the focused node, ancestor chain, and default editing
hooks before running any body. Each node contributes authored listeners first,
then its default editor; App fallback comes last. A default editor's acceptance
stops the route even when its content or caret does not change. Hidden, disabled,
missing, or generation-stale focus skips entity input but still permits App
fallback. Key and character payloads share this route; wheel and rotary payloads
cannot use it. The keyboard route reuses the same step storage, generation
checks, owner scope, and error cleanup as gesture and notification delivery.

Internal hooks and listeners share the reserved step storage and cleanup guard.
A failed reservation does not raise either logical quota. The pool's ABI is
confined to hidden macro support; the ordinary `on` and App installation APIs
are not implemented by this layer.

### Named event callback declarations

`#[event]` generates a module with `callback(...)`. The factory takes registered
Model instances and ordinary `Copy + 'static` values in declaration order.
Resources, queries, the payload, the current entity, and deferred commands are
invocation parameters, not factory arguments:

```rust,ignore
#[event]
fn moved(
    game: model!(Game),
    step: u16,
    input: event!(DragMove),
    node: entity!(),
    positions: query!(mut Position),
) -> bool {
    let Some(position) = positions.get(node) else {
        return false;
    };
    game.move_to(position, input.x, input.y, step);
    true
}
```

`moved::callback(game, step)` describes the callback without exposing World or
generated handle types. `event!(E)` borrows a concrete payload; omitting it
leaves the event type to the binding site. `entity!()` excludes App-only
bindings. Notification callbacks return `()`; consumable routes also accept
`bool` or `BubbleControl`. The declaration shares System resource/query
lowering and the prepared executor. Listener installation and the `on`
frontends are separate from this factory facility.

## Choosing state and rendering roles

| Role | Use | Update path |
| --- | --- | --- |
| `#[model]` | Registered business state, including multiple instances of one type | Generated handle methods compare `#[observe]` values, publish named revisions, and deliver typed effects |
| `Signal<T>` | A standalone reactive value or a value published by another owner | `set` / `update` notify reactive readers |
| `#[component]` | Data attached to an entity | World insertion and replacement drive component lifecycle; `model!(T)` fields retain registered model identity |
| `#[compose]` | Build a widget subtree | `model!(T)` parameters share instances among generated bindings, callbacks, and direct child calls |
| `com!(entity, T)` | Read a component in composition or an event | Returns a short-lived `Option<&T>` without subscribing |
| `View` / `#[view]` | Register a widget kind; the attribute generates its paint callback | Independent hooks provide painting, attachment, gestures, observation, and systems; typed paint roles declare model projections and resources |
| `#[system]` | Scheduled updates | `model!(T)` captures a registered instance; `res!`, `query!`, and `commands!` declare all other runtime access |
| `on Event { ... }` | Handle gesture and widget business events | Event fields are local bindings; `com!` reads and `commands!()` defers structural changes |
| `#[timer]` | Run a callback on a time schedule | `model!`, `entity!`, and `commands!` declare callback access |
| `#[animate]` | Apply a scalar motion to component state | `motion!`, `component!`, `model!`, `entity!`, and `commands!` declare frame access |
| `AudioHandle` | Shared output controls when the `audio` feature is enabled | Host UI and demo controls read one output state and submit to one bounded bus |
| `World` | Low-level entity, component, and resource access | Available for integration code; ordinary model methods and UI callbacks do not need it |

`#[model]` does not replace components: a model has application-level identity,
while components attach behavior and model bindings to individual entities.
`#[view]` does not replace composition: it defines a component's paint callback,
while `#[compose]` builds its surrounding widget tree. `Signal<T>` remains useful
for a small independent value; a model does not need to wrap every signal.

`Slider` input and `prop::SliderValue` use the same clamped value update. A
programmatic property change invalidates the control without emitting a user
gesture event.
`Switch` tap and `prop::SwitchOn` changes also use one animation-seeding path.
Assigning the current value leaves the running spring untouched; only taps
emit `SwitchEvent::Toggled`.
`Checkbox.checked` can bind a reactive value through `ui!`; it shares its
checked-state update with taps, while programmatic changes emit no toggle event.

## Primitives

Three types live in `mirui::core::reactive`:

```rust
use mirui::core::reactive::{Signal, Computed, Effect};

let count = Signal::new(0i32);          // holds a value, tracks readers
count.set(1);                           // replace
count.update(|n| *n += 1);              // mutate in place
let n = count.get();                    // read (subscribes the caller)
count.with(|n| println!("{n}"));        // borrow without cloning

let doubled = {
    let count = count.clone();
    Computed::new(move || count.get() * 2)   // lazy, recomputes when count changes
};

Effect::new(move || {
    // re-runs whenever a signal read inside it changes
});
```

- `Signal<T>` can own state or publish a small projection of an authoritative
  model. `get()` / `with()` subscribe the current reader (an effect, a computed,
  or a reactive binding). `set()` / `update()` mark subscribers dirty.
- `Computed<T>` derives a value lazily: it only recomputes when one of its
  inputs changed, and only when read.
- `Effect` runs a closure now and again whenever the signals it read change.

Signals are `Clone` (cheap, reference-counted), so clone one into each
closure that needs it.

## Reactive attributes

Inside `ui!`, prefix an attribute value with `$` to bind it reactively. A
bare path reads the signal; a `${ … }` block runs an expression:

```rust
let label = Signal::new(0i32);

#[compose]
fn counter_label(label: Signal<i32>) {
    ui! {
        Text (text: ${ alloc::format!("Count: {}", label.get()) }, height: 40)
    };
}
```

When `label` changes, only that attribute updates — the widget is not
rebuilt. Reactive binding is supported on `text`, `path`, `visible`,
`bg_color`, `text_color`, `border_color`, `border_width`, `render_key`,
`font_size`, `direction`, `width`, `height`, `left`, `top`,
`Button.normal_color`, `Text.paragraph`, `ProgressBar.value`, `Slider.value`,
`Switch.on`, and `Checkbox.checked`.

Reactive `Slider.value` clamps to the control range and does not emit a
`ValueChanged` event when the model updates it. Reactive `Switch.on` starts
the switch animation when the model changes its state without emitting
`Toggled`. User input still emits those events. Marble Play binds its labels,
controls, and custom board View directly to one registered model; its physics
arrays are not copied into a second UI-state struct.

`attr: $signal` is shorthand for `attr: ${ signal.get() }`.

A reactive text path keeps the text entity stable while transferring its subscription to the selected path:

```rust
let selected = Signal::new(first_path);

ui! {
    Text("Live route", path: $selected)
}
```

To derive the geometry of one mutable path from signals, use `UiScope::bind_path`. The owner-bound effect edits the existing `PathId`, retains its allocation, and invalidates drawing and text consumers when its revision changes. See [`typography.md`](typography.md#derive-path-geometry-from-signals).

## Layout-responsive attributes

`$` reacts to application state. `@` reacts to computed layout geometry and lists every dependency in its head:

```rust
ui! {
    Column (container: true) {
        Text(
            "Status",
            font_size: @width {
                if width < Fixed::from_int(520) { 18_u16 } else { 30_u16 }
            },
            padding: @(width, height) {
                Padding::all(width.min(height) / 24)
            }
        )

        View(
            id: "stage",
            width: Dimension::percent(100),
            height: 180
        )

        Text(
            "Stage label",
            width: @id(stage).width { stage.width },
            top: @id("stage").height as stage_height { stage_height / 8 }
        )
    }
}
```

Bare `width` and `height` resolve to the nearest ancestor marked `container: true`. Named dependencies use the existing widget ID registry. String IDs require `as alias`; aliases are also available for identifier IDs and container geometry. One widget can declare up to 4 distinct dependencies, stored without a runtime allocation. Multiple responsive attributes on the same widget share one binding and one geometry snapshot.

The binding runs after layout and may request one additional layout pass when it changes a layout property. The pass count is bounded, and unchanged derived values do not invalidate the widget. Continuous geometry that cannot be expressed as a widget property can use `UiScope::bind_layout` with explicit `LayoutDependency` values and caller-owned state.

## Reactive control flow

A `$` on a control-flow head makes its selection reactive. An `if` caches
all branches when its parent is built and toggles visibility when signals
change. A `match` with patterns that do not bind values also caches its arms;
`walk` builds rows as its input changes.

```rust
let show = Signal::new(true);
let other = Signal::new(false);
let state = Signal::new(Load::Loading);   // an application enum

#[compose]
fn status_panel(show: Signal<bool>, other: Signal<bool>, state: Signal<Load>) {
    ui! {
        Column (grow: 1.0) {
            if ${ show.get() } {
                Text ("visible")
            } elif ${ other.get() } {
                Text ("alt")
            } else {
                Text ("hidden")
            }
            match ${ state.get() } {
                Load::Loading => {
                    Text ("loading")
                }
                Load::Ready(s) => {
                    Text (s)
                }
            }
        }
    };
}
```

- `if $cond` / `elif` / `else` construct every branch once, then switch
  visibility without discarding state. Each branch may have several roots.
- `match $expr` caches every arm when no pattern binds a value. If any arm
  binds a value, as in `Load::Ready(s)`, the whole match keeps structural
  rebuild behavior so its body sees the current `s`. Both forms support
  multiple top-level roots.
- Use qualified unit variants such as `Load::Loading` for cached matching;
  an unqualified identifier can also be a value binding and takes the
  structural path. `None`, literals, wildcards, and ignored payloads such as
  `Some(_)` do not bind values.
- `elif` is a single keyword (not `else if`).
- A head **without** `$` is static — evaluated once at build, never re-run.

## Lists

`walk` iterates a collection. With `$` it re-evaluates when the iterable's
signals change:

```rust
let items = Signal::new(alloc::vec![/* … */]);

Column (grow: 1.0) {
    walk ${ items.get() } with item {
        Text (item.name, bg_color: item.color, height: 28)
    }
}
```

Two reconciliation strategies:

- **Index-based** (default, no `by`): rows align by position. Growing the
  list builds and appends new tail rows; shrinking despawns tail rows;
  surviving rows keep their entity. Correct for append / drop-tail lists.
- **Keyed** (`by <key>`): rows align by identity. When the list reorders or
  an item is inserted/removed in the middle, a row keeps its entity (and any
  per-widget state) and just moves, rather than being rebuilt in place.

```rust
walk ${ items.get() } with item by item.id {
    Text (item.name, height: 28)
}
```

Use keyed when the list reorders or mutates in the middle; index-based is
lighter for plain append/drop-tail.

## The flush model

Setting a signal does not update widgets immediately. It marks subscribers
dirty and enqueues them. Before layout, after systems and plugin updates,
`flush_signal_dirty` drains the queue: dirty effects re-run, dirty widgets
get re-rendered. This also happens for an explicit `App::render` call; there
is no background thread.
Multiple source changes before one flush queue the same effect or widget once.

A reactive binding's first run applies its initial value at construction
(inside the `ui!` build), so the first frame already shows the correct
state.

Registered model methods, observed notifications, fixed-array effect delivery,
typed `#[view]` subscriptions, and registered
model/signal-to-computed-to-effect propagation reuse their storage during
updates. Each View binding owns an independent dependency-graph consumer;
removing one View does not detach another View on the same entity. Initial
attachment may reserve storage, while rebinding already-used model sources
reuses it.
Model handles become invalid when their registration is removed or the App ends,
even if another owner retains the underlying storage.
This is not a zero-allocation guarantee for text formatting, new widgets,
layout, glyph loading, or a complete render frame. Reserve those resources
separately when targeting a bounded-memory device.

## Limits

- **Observed values**: `#[observe]` fields and derived getters require small
  `Copy + Eq` results. Keep large mutable data in the model and publish an
  explicit named revision when a View must repaint for it.
- **View dependencies**: `model!(component.field)` alone does not subscribe a
  View. Add observed fields, getters, or named revisions with `watch(...)`;
  replacing the component replaces its subscriptions.
- **View rendering**: both generated and manually registered View renderers
  may read models but cannot update them while painting. Use an input callback
  or system to change model state before rendering.
- **Computed ownership**: a computed created inside an App belongs to that
  App and cannot be read from another App or after that App ends. An ownerless computed may share
  ownerless signals but cannot read a registered model. Computed evaluation
  cannot write models.
- **Effect ownership**: an effect created outside an App remains worldless
  until it first reads a registered model, then belongs to that model's App.
  It cannot read models from two Apps. While unbound, `with_world()` returns
  `None` even when a World flush triggers the effect; the binding run remains
  worldless and later runs use the bound App.
- **Effect delivery**: `#[effects]` extractors return fixed arrays of optional
  events. Consumers cannot start another model write during delivery.
- **Reactive `if` branches**: every branch body is constructed once when
  its parent is composed, even when initially hidden. Multiple top-level
  widgets keep their component state and source order across selections.
  Hidden branches do not participate in layout, drawing, or pointer hits.
  Construction code runs eagerly, and IDs must be unique across branches.
  A branch root's own `visible` binding remains independent of its branch
  selection. Reactive bindings in a hidden branch may still update its cached
  state.
  Put nested reactive `if`, `match`, or `walk`, and slot operations inside a
  widget so their children remain under the cached branch root.
- **Reactive `match`**: when no arm binds a value, all arms are constructed
  once when their parent is composed; switching only changes visibility and
  retains widget state and source order. Construction code in every arm runs
  eagerly. If any arm binds a value, the whole match rebuilds its selected
  arm when the source changes, may allocate, and manages all top-level roots.
  Put nested reactive control flow and slot operations inside a widget in
  either form. A user's scrutinee expression may allocate independently of
  the branch mechanism.
- **Reactive `walk`**: rows are built on demand. Its initial dynamic roots
  may appear after static siblings, regardless of source order.
- **Index-based `walk` does not update surviving rows' content**: with no
  `by` key, a middle insert/remove shifts which data each surviving row
  shows only through that row's own reactive attributes; the row structure
  itself is not re-matched. Use keyed `walk` when identity matters.
