# Reactive State

mirui has a reactive state layer — signals, computed values, and effects —
that drives widget attributes and structure declaratively from the `ui!`
macro. Change a signal and the widgets that read it update; no manual
diffing, no observer wiring.

This guide is the long-form companion to the `mirui::core::reactive` API docs and
the State demos in the gallery.

See [Persistence](persistence.md) for storage and automatic-save configuration.

## Contents

1. [Registered model instances](#registered-model-instances)
2. [Choosing state and rendering roles](#choosing-state-and-rendering-roles)
3. [Primitives](#primitives)
4. [Reactive attributes](#reactive-attributes)
5. [Layout-responsive attributes](#layout-responsive-attributes)
6. [Reactive control flow](#reactive-control-flow)
7. [Lists: `walk`, index vs keyed](#lists)
8. [The flush model](#the-flush-model)
9. [Limits](#limits)

## Registered model instances

The default `mirui::prelude` supplies the authoring macros and their common types without importing `World`. Framework implementations, backends, and explicit ECS diagnostics can import `mirui::ecs::World`; generated authoring adapters resolve their internal World paths independently of the user's imports.

`mirui::prelude::draw` supplies drawing commands, path values, and paint-context types. Backend traits use the explicit `mirui::render::{Canvas, Renderer}` paths; low-level View descriptors and registries use `mirui::ui::view::{View, ViewRegistry}`.

`App::add_resource(value)` immediately installs a resource in the application's existing World storage. Values move into storage without a `Clone` or `Copy` requirement; adding the same type replaces and drops its previous value. Callbacks borrow the current value through `res!(T)` or `res!(mut T)`. Structural changes during a callback continue to use deferred `commands.insert_resource(value)` and `commands.remove_resource::<T>()` after parameter borrows end. Resource installation does not create model observations or a second resource registry.

Application setup installs shared services with `app.add_resource(value)` and creates paths through `app.paths()`. Path helpers can accept `mirui::text::PathAccess` to insert or edit paths without receiving the entire World.

Within a `#[compose]` Rust body, resource expressions access that composition's existing application storage. Each `with` closure borrows only for its call and returns a value independent of the resource reference. Resources remain shared services or configuration; mutating one does not add Model observations.

A composition can initialize resources without creating widgets. Gallery font registration uses `app.compose(parent, register_fonts())` to borrow the installed `FontManager` through `res!(FontManager).with(...)` before constructing text widgets; it does not replace the manager or require a World parameter.

Optional initialization can check `res!(optional T)` and insert a default through `res!().insert(...)` after the read closure returns. The Gallery clock composition uses this pattern to preserve a host-provided `MonoClock` and its timing state.

A composition can use the entity returned by `ui!` to initialize node-targeted resources before returning that entity. PinchRotate creates its gesture target, installs a `SimTimeline` anchored to it through `res!().insert(...)`, and returns the target for ordinary child composition. Simulation system and clock registration stay in the outer setup.

For targets already named by other compositions, `res!(IdMap).with(...)` reads the application's existing widget registry and can return copied entity IDs. Kinetic Console prepares its input sequence with `app.compose(root, kinetic_console::build_sim_timeline())`; the result is `None` if a required target is absent. The caller installs the returned `SimTimeline` with `app.add_resource` when autoplay is enabled.

Widgets uses the same named-target lookup, so its automation does not select unrelated controls elsewhere in the application. `widgets::install(app, root, autoplay)` installs its composition, Model-bound theme timer, and optional input sequence on desktop and `no_std` targets. The caller supplies the clock, default widgets and systems, and backend-specific plugins. `widgets::build_widgets()` constructs only the widget tree, with layout determined by the parent size.

| Operation | Expression | Result |
| --- | --- | --- |
| Read a required resource | `res!(T).with(read)` | `read` receives `ResourceRef<'_, T>` |
| Read an optional resource | `res!(optional T).with(read)` | `read` receives `Option<ResourceRef<'_, T>>` |
| Mutate a required resource | `res!(mut T).with(write)` | `write` receives `ResourceMut<'_, T>` |
| Install or replace a resource | `res!().insert(value)` | Immediate replacement; the previous value drops |
| Remove a resource | `res!().remove::<T>()` | `Option<T>` |

Resource borrows carry their application's identity and dereference to the stored value. Field and method access stay direct. Pass `&value` or `&mut value` to ordinary helpers taking references; use `optional.as_deref()` for helpers taking `Option<&T>`. Callback macros supply the mutable binding for `res!(mut T)`. A composition closure that modifies a resource writes `.with(|mut value| { ... })`; read-only use needs no mutable binding. These borrows add no heap allocation and cannot escape a composition's `with` call.

Only `res!(optional T)` permits an absent resource. Use required roles for dependencies installed by application or composition setup; use optional roles when absence is part of the callback's behavior, such as checking whether a clock is installed or reading a cache before its first update. Dereferencing a resource never creates a default value.

```rust,ignore
struct PanelConfig {
    gap: i32,
}

#[compose]
fn configured_panel() {
    let gap = res!(PanelConfig).with(|config| config.gap);
    ui! {
        Column (gap: gap) {}
    };
}

app.add_resource(PanelConfig { gap: 8 });
app.compose(root, configured_panel());
```

`App::compose` returns the composition's result, including entity IDs and owned values. `Computed` and `Effect` values created in that call belong to the application. Its construction scope does not escape the call; reactive ownership is restored and dirty storage is prepared before the result is returned. Structural World windows are internal to generated bindings and tree updates, not an authoring entry point.

Keep visual styling on the nodes a composition creates. Orbit Console paints its background through its shell's backdrop component without changing the caller's parent background.

A prepared composition also works as the child callback of `tree.children(recipe, panel(...))`. The returned `(container, result)` includes the actual child entities, so sibling components can reference those IDs without a later lookup or mutation pass. Snapshot and diagnostic scenes use the same construction entry; their framebuffer capture and renderer inspection remain separate from composition. See [snapshot tools](../gallery/examples/tools/README.md) for local capture commands.

Calling a `#[compose]` function prepares a stack-held composition without executing its body. All arguments remain owned or borrowed by that value until it is executed or dropped. Model validation, resource access, bindings, and widget creation run inside `App::compose`; preparing a composition does not require an active application owner. Composition outputs use concrete Rust types, including generics. Returned references may borrow application data supplied as arguments, but not the construction scope or its resources.

For construction without the UI DSL, pass a closure to `App::compose`. Its scope accepts components, typed widget builders, and tuples of both. `children` returns the container entity and the closure's result. Scope creation and raw widget builders are internal; generated UI and Niche construction use the same implementation through hidden macro support. The supplied scope does not expose World access or require manual parent wiring.

```rust
let (panel, slider) = app.compose(root, |tree| {
    tree.children(panel_style(), |column| {
        column.spawn(Text::build("Level").style(label_style()));
        let source = controls.clone();
        let slider = column.spawn(
            Slider::build(0, 100)
                .bind_property::<prop::SliderValue>(move || source.level())
                .fill_color(ColorToken::Primary)
                .style(slider_style())
                .on_event(Slider::ValueChanged, level_changed::callback(controls.clone())),
        );
        column.spawn((
            Button::build().on_event(Tap::EVENT, reset::callback(controls.clone())),
            Text::label("Reset"),
        ));
        slider
    })
});
```

Typed builders retain their widget type across `bind_property`, `on_event`, and style configuration. Properties use the existing reactive effects and property setters; programmatic updates do not emit widget input notifications. `tree.on_event(entity, key, callback)` installs a callback on an already constructed node and validates its attached source. Typed builder callbacks retain compile-time widget-source constraints. See `builder_form` for the complete Model, event callbacks, and layout.

Static tuple components are inserted before any attachment or binding executes, with the last component of a given type taking precedence. Pending properties and events use stack-held tuples rather than an allocated container. Entity storage and effect and callback registration retain their existing construction-time allocations; installed bindings add no allocation to repeated Model updates or event execution.

The root builder initializes its attachment hooks when `id()` completes, after style and safe-area overrides. `App::set_root` selects an already constructed tree; it does not repeat attachment hooks or rebuild descendant widgets. Register widget Views before constructing their nodes.

Compute resource-backed arguments before passing them to `ui!`, `format!`, or another macro; the composition attribute does not rewrite another macro's tokens. Required resources report missing values as programming errors, while optional reads receive `None`. Composition inserts and removals are immediate; structural changes during System or Event callbacks continue through deferred `commands!`. Resource access adds no heap storage; registering a new resource type still uses the existing resource store's allocation.

CoverFlow and Particles install their bounds from within their compositions through `res!().insert(...)`. Shapes, Butterfly, and Vector Mandala read an optional MonoClock before building their trees, retaining zero start time when the clock is absent.

The I18n composition borrows the application's FontManager to register its font and installs its translation resource through `res!().insert(...)`. Its locale Signal is captured before installation and remains the language button's state source.

Application creation supplies the named-widget ID registry. Compositions declare `id: ...` without initializing that registry, and existing named entries remain available to later compositions.

`id("source")` and `id(source)` can reference a later named widget in the same static sibling sequence, including references inside an earlier sibling's children or child-composition arguments. The macro reserves that widget's identity before executing the sequence; it initializes components, attaches the widget, and appends children in declaration order. A forward reference supplies an `Entity`, not an already initialized component set. IDs declared inside a conditional branch, loop body, child composition, or Niche are not reserved outside that scope. Names remain unique across the application.

```rust,ignore
ui! {
    View () {
        View () [DropShadow::new(id("source"))]
        View (id: "source", width: 80, height: 40)
    }
}
```

Declarative nodes connect to their parent and register their final `NamedId` after static components are installed and before matching View attachment hooks run. Reactive property initialization and event installation follow attachment. Each node has one named identity; declaring another name for the same node replaces that marker, not an alias. Layout containers gain input feedback only when a matching widget View provides it.

Place `#[model]` on a struct and one inherent impl to generate its instance handle and forwarded methods. Each `app.add_model(value).finish()` call registers an independent instance; cloning a handle refers to the same instance without cloning its data.

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
let counter = app.add_model(Counter { count: 0 }).finish();
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

Declare an effect extractor in the marked Model impl. Return `Option<E>` for at most one event or `[Option<E>; N]` for a bounded batch:

```rust
#[effects]
fn take_note(&mut self) -> Option<Note> {
    self.pending_note.take()
}

player.on_effect(move |note: &Note| play_note(note.pitch))?;
```

Application code calls business methods; the framework calls the extractor at the end of the outer Model transaction. Extractors are not exposed on the registered instance. Each `Some(event)` is delivered separately to its registered consumer; `None` is skipped. A single optional event uses the same delivery path as a batch of capacity one.

`model.on_effect(handler)` installs one consumer per effect type and Model instance. A duplicate registration returns `AlreadyRegistered`; it does not append or replace a handler. Registration belongs to the Model rather than the composition that installs it. Events are drained even without a consumer. Delivery occurs after state publication and release of the Model data borrow; consumers can read committed state but cannot start another Model write transaction.

The callback borrows each event only for its invocation. Events do not require `Copy` or `Clone`; retaining owned event data for later use requires an explicit application copy. Extractors must move their pending events out of the Model, for example with `Option::take` or `core::mem::take`, to avoid delivering the same business event again.

Lists and maps can consume their direct members' effects with `items.on_effect(|effect: &ModelItemEffect<E>| { ... })`. The envelope carries the stable member ID and the event; individual delivery precedes collection delivery. See [member effects](model-collections.md#member-effects) for registration and lifetime rules.

### Owned child models

Declare `child: model!(owned Child)` inside a `#[model]` struct to store child data in its parent. Construct the field from raw data with `.into()` and register only the parent. The field getter returns the child's registered instance for existing Component, Compose, System, Event, and View roles.

Use `model!(owned ModelList<Item>)` for ordered child models, `model!(owned ModelMap<Key, Item>)` for keyed child models, or `model!(owned ModelSet<Key>)` for observable membership. See [Model collections](model-collections.md) for initialization, identity, capacity, and iteration rules.

```rust
#[model]
struct Scoreboard {
    left: model!(owned Counter),
    right: model!(owned Counter),
}

#[model]
impl Scoreboard {
    fn new() -> Self {
        Self {
            left: Counter { count: 0 }.into(),
            right: Counter { count: 0 }.into(),
        }
    }

    #[observe]
    fn total(&self) -> u32 {
        self.left.count() + self.right.count()
    }

    fn increment_both(&mut self) {
        self.left.increment();
        self.right.increment();
    }

    fn reset_left(&mut self) {
        self.left = Counter { count: 0 }.into();
    }
}

let scoreboard = app.add_model(Scoreboard::new()).finish();
let left = scoreboard.left();
left.increment();
assert_eq!(scoreboard.total(), 1);
scoreboard.increment_both();
assert_eq!(scoreboard.total(), 3);
scoreboard.reset_left();
assert_eq!(left.count(), 0);
```

One outermost mutating method call commits the whole registered tree, whether called through the parent or a child. Observed fields and derived values publish only when their final values differ. Each child accumulates its own named watch masks, independently of its parent's change type; each matching watch advances at most once per commit. A parent-derived observation includes direct child updates. Internal parent methods call the owned fields directly; an external handle cannot re-enter the same tree during a mutable borrow.

Child identity belongs to the field path. Replacing or swapping child data keeps existing handles, subscriptions, callbacks, and effect consumers attached to that field. A successful replacement invalidates that child's named watches and descendant watches, while observed values still compare before and after. Dropping the parent registration ends all child registrations; handles do not keep the data alive.

Effects are extracted in parent-first, declaration-order depth-first traversal and delivered after all observations have been published and the tree borrow released. Pending effects move with swapped data; replacement drops effects still held by the overwritten value. Already-extracted effects remain in the current batch. This order is not the chronological order of business method calls across children; use one bounded business queue when that order matters.

An outer `Err` keeps data changes but discards the whole tree's pending notifications and effects from that call. An inner child call has no separate transaction or rollback: if the parent handles its error and succeeds, final observed values and effects still present in the data are committed. The failed child return does not match that child's change masks; successful deeper calls retain their pending masks.

Declared owned fields form a finite tree; owned Model collections add dynamic members with explicit capacity preparation. Static-tree construction with `.into()` and warm reads, writes, resets, and swaps add no framework heap allocation. Registration prepares root storage, child projections, observations, and retained transaction snapshots; effect consumers retain their registration-time allocation. Each write compares the active tree, so transaction work scales with its observations and effects. Business constructors and method bodies keep their own allocation costs.

The [State Todo model](../src/gallery/demos/state_todo/model.rs) owns a `ModelList` of tasks. Keyed rows retain their item bindings during reordering, completed tasks can be removed, and the parent derives the remaining count from current members. See [Model collections](model-collections.md) for data sources, identity, and capacity rules.

With the `persistence` feature enabled, the demo saves its task collection. See [Persistence](persistence.md#collections) for archive and restoration behavior.

### Model roles

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

Composition validates every declared registered Model parameter against the supplied UI scope before executing the function body. A foreign or ended registration is rejected before any authored changes; optional roles recursively validate present values and accept `None`. Signal, Computed, and audio handles retain their existing access and lifetime rules. This check covers declared role parameters, not arbitrary closure captures.

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

Use `com!(entity, T)` during composition to inspect an
attached component. The read returns `Option<&T>`, remains valid only for its
short lexical borrow, and does not subscribe to changes. A typed `#[view]`
model role with `watch(...)` is the rendering path when model changes must
invalidate a View.

Register the instance before building controls. A `model!(Counter)` compose parameter accepts the handle returned by `app.add_model(value).finish()`:

```rust
let root = app.spawn_root().id();
let counter = app.add_model(Counter { count: 0 }).finish();
app.compose(root, counter_controls(counter.clone()));
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

Related entities can be read through separate queries. The Subpixel demo iterates `query!(Entity, mut BarState, Parent)` and obtains each bar's container bounds with `rects.get(parent.0)`, where `rects` is declared as `query!(ComputedRect)`. The parent relation supplies the target; no global arena resource or ID lookup is needed.

Entity-local animation state can be attached with `View(...) [component]` or the corresponding widget declaration. CompactCurveText attaches `CompactCurveMotion` to its Text and queries that state alongside `TextPath`; each instance advances independently and reads its current path from the same entity.

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

With the default widget Views registered, `query!(mut Style)` records the entity's component write access and the render boundary applies the Style View's layout invalidation policy. Fetching a mutable row is conservative: it records a change even without a field assignment, and a color-only assignment also requests layout. Reactive properties and typed property commands compare old and new values and distinguish unchanged, visual, and layout updates; arbitrary low-level mutable references do not provide the same entity-level tracking.

Event blocks read their payload through `event`. A literal callback or a named `#[event]` function declares entity, query, resource, and command roles in its parameters. Queries use the same access validation and point-fetch rules as systems. Deferred commands apply in source order after parameter borrows end.

```rust
use mirui::input::event::BubbleControl;
use mirui::prelude::*;

#[component]
struct Selected;

#[event]
fn toggle_selection(
    input: event!(Tap),
    selections: query!(Widget, optional Selected),
    commands: commands!(),
) -> BubbleControl {
    let Some((_, selected)) = selections.get(input.target) else {
        return BubbleControl::Allow;
    };
    if selected.is_some() {
        commands.remove::<Selected>(input.target);
    } else {
        commands.insert(input.target, Selected);
    }
    BubbleControl::Allow
}

#[compose]
fn selectable() -> Entity {
    ui! {
        View () on Tap(toggle_selection::callback())
    }
}
```

For consumable input, returning `()` or `true` stops propagation; `false` or `BubbleControl::Allow` continues it. Widget notifications require `()` and notify every selected listener. A gesture payload's `target` is the original recognized target; `entity!()` is the current handler node. A panic discards that invocation's pending commands, retaining the command buffer for reuse.

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

Cursor and rotary feedback Views use their overlay marker as the descriptor filter and borrow `res!(optional InputFeedback)` on each paint call. Replacing or removing that resource affects subsequent calls; inserting, replacing, or removing the marker uses the descriptor's visual invalidation policy.

Button, Slider, Switch, Checkbox, ProgressBar, TabBar, Text, TextInput, Icon, Image, and BackgroundBlur use this declaration for built-in paint callbacks. Their widget modules export the generated `view` factory directly; component filters, paint priorities, and typed input callbacks remain part of the descriptor.

StaticGlyphRun declares its component filter, paint priority 80, and layout invalidation through `#[view]`. It submits its borrowed positioned glyph slice without runtime shaping or a layout workspace. Theme is required; FontManager remains optional, with empty runs and unavailable fonts producing no draw commands.

Render Showcase and Blur Filter declare their retained scene workspace with
read-only `res!` and replay borrowed scene operations through `paint!()`.
Their setup prepares RGBA storage before painting; the View borrows that
storage rather than allocating another buffer in its callback.
BackgroundBlur receives its component and paint context directly. Target
sampling prepares readback before accessing pixels, retains backend errors,
and submits the sampled region without applying its transform twice.
`model!(component.field)` borrows that component's model only during paint;
`watch(selector)` adds an exact subscription to an `#[observe]` field, an
observed getter, or a named revision. An optional model field uses
`model!(optional component.field, watch(selector))`. Add a named
`component!()` parameter only when the paint body also needs `&T`. `res!(T)`,
`entity!()`, and `paint!()` expose resources, entity identity, and one
frame-scoped paint context. `PaintCtx` provides the current geometry, style,
widget state, transform, clip, drawing, and scene replay operations without
exposing the renderer as a separate callback parameter.
View resources are read-only. `res!(T)` requires the resource at each paint invocation; `res!(optional T)` supplies `Option<ResourceRef<'_, T>>` and still runs the body when the resource is absent. Both use the same resource role grammar as systems and events, without cloning the resource or allocating parameter storage.

View queries use the same `query!` grammar and row types as systems and events, but accept only shared component access. A required component item or `with` filter drives the query; `Entity` and optional items alone do not supply a driver. The query can read other entities, iterate its complete dataset, or call `get(entity)`; the View's `component = T` filter selects its own instances rather than restricting query results.

```rust
#[view(component = Gauge)]
fn paint_gauge(
    gauge: component!(),
    samples: query!(Sample, optional Calibration; with Active; without Hidden;),
    paint: paint!(),
) {
    if let Some((sample, calibration)) = samples.get(gauge.source) {
        paint_sample(paint, sample, calibration);
    }
}
```

Required items, optional items, and `with`/`without` filter types form static View dependencies. When paint queries only fetch the current entity, changes to those types invalidate that entity according to the View's Visual or Layout policy. Reading another entity, even a missing row, or starting iteration permanently broadens that descriptor's dependency scope to all its instances. Before the first successful paint, invalidation is also conservative. Obtaining a mutable component reference counts as a possible write. Replacing a shared source, such as the font manager or text layout cache, invalidates all dependent instances regardless of their query scope. Tracking does not create per-fetch subscriptions or rebuild the composition tree; registration prepares tracking, and warmed updates reuse existing storage.

Pending dependencies are consumed before layout and after layout before the dirty-region plan. A changed `ComputedRect` therefore reaches querying Views in that frame. Changes produced by dirty-plan generation itself, such as `OffscreenGeneration` or scroll residue, remain pending for the next frame rather than replaying the walker. A View whose redraw changes its own queried dependency can continue to invalidate itself; dependency tracking does not suppress that feedback.

Switch queries its component together with optional background and thumb-animation state. Its generated View declares attachment and Tap callbacks, paint priority, and animation systems.

Image queries its component together with its optional resource lease and borrows the required Theme and optional texture manager. A cached lease can be used without a manager; replacing a manager affects direct resolutions and newly created leases rather than retargeting an existing lease. Vector paths remain borrowed.

Text and TextInput paint from prepared layout storage in `Outputs<TextGeometry>`. TextInput's attachment installs the `TextEditing` capability; its View paints that capability's current content, selection, and cursor. See [text geometry](typography.md#query-caret-and-selection-geometry) for output dependencies and [text editing](text-editing.md) for editor composition and input handling.

Gallery Views require the Theme, FontManager, and PathStore supplied by the App when they use those resources. ScopeCanvas requires its setup's waveform workspace, and CurvePaths requires its registered path IDs; retired path IDs are skipped without copying paths. RasterContour skips missing glyphs and unsupported raster representations. Shapes, Butterfly, and Vector Mandala retain an optional MonoClock so static snapshots can render without a running clock.

The generic Style pass uses `#[view]` with a required Theme resource and `paint!()`. It retains paint priority 50 and layout invalidation, skips a background already handled by an earlier widget, and draws the border through the same paint context.

`with_clip_path` owns the matching clip push/pop pair, including cleanup after
a body draw fails. Passing raw `PushClip` or `PopClip` commands to `draw` is
rejected. Paint operations retain the first render error for the frame walker
instead of requiring callbacks to forward backend results themselves.
The generated adapter holds no model borrow after paint and rejects model writes inside the callback. Multiple model fields, including two fields that refer to the same instance, can be read together. `invalidation = Layout` promotes watched changes from visual to layout invalidation; visual invalidation is the default. `attach = initialize::callback()`, `input = (tap::callback(), key::callback())`, and static `systems` declare widget capabilities. Attachment and input functions use `#[event]` with the existing parameter roles. An optional `name = "..."` keeps a stable registration name when the paint function name differs from the widget name. Subscriptions reserve visual notification queue capacity when attached; model updates and notifications reuse that capacity. Component replacement and removal update bindings before the next paint.

`#[view]` constructs the descriptor returned by `function_name::view()`; `app.with_widget(...)` registers it. Raw descriptor assembly and observation collectors are confined to hidden macro support. The returned `View` supports `with_systems` and `with_invalidation` configuration.

`with_systems([step::system(first), step::system(second)])` replaces the whole system list with owned systems bound to explicit Model instances. Borrowed system templates are also accepted. Register each View kind once; multiple instances can contribute systems in the same list or through `app.add_system`. Each system is checked against the target App before installation. Repeated registration adds another descriptor and another set of systems; it does not replace an installed kind.

Systems belong to the App scheduler, not widget entities. Hiding or removing a widget does not uninstall its systems, and existing `expect` conditions retain their component-presence semantics. Arrays and source iteration add no heap allocation; bound callbacks and scheduler storage retain their registration-time allocation costs. Installed descriptors do not retain a second system list. `app.with_widgets` accepts Views with the same source type; chain `with_widget` for different array lengths or mixed source types.

A View describes a widget kind, not just its paint function. Paint, attach, input, observation, and systems are independent capabilities.

Registration retains a shared attachment/input plan in registration order. Per-node attachment reuses that plan without allocating a hook list. Components inserted during initialization are initialized before their View inputs become available. Replacing or removing a component retires its previous attachment and pending input hooks. Registration and structural construction can allocate metadata; ordinary component writes do not rerun attachment.

`ui!` seeds the primary component and static text content, prepares binding captures, inserts static component attachments in source order, and invokes matching attachment hooks once before running reactive initial values. Preparing captures does not evaluate reactive expressions or register effects. Layout containers with component attachments participate in the same initialization; plain containers without a matching gesture capability do not gain input feedback. Prepared callbacks stay on the construction stack until installation and reuse the existing effect runtime.

`LazyList` supplies each retained cell with a `ListItem` Model. Reading `item.index()` in a reactive property binds it to the cell's current data index; inactive cells suspend their bindings until reused. Persistent selection and editing state belong in a business Model keyed by data identity, not the recycled Entity. See [scrolling and virtualized cells](scrolling.md#virtualized-cells) for cell construction, viewport behavior, and input handling.

An empty `#[view]` function declares a widget without a paint hook:

```rust,ignore
#[view(component = TabContent, name = "TabPages", systems = [tab_pages_system::system()])]
fn tab_pages_view() {}

app.with_widget(tab_pages_view::view());
```

TabPages and LazyList use this declaration and remain Views. The macro generates
neither a body helper nor a render callback for an empty body; nonempty bodies
remain paint callbacks even when they do not draw. Parameter roles are still
validated, and declared model watches still create subscriptions. Systems are
installed only when the View is registered, preserving their execution order
and component requirements. Paint traversal skips these descriptors.
Paint priority orders only drawing, with registration order breaking ties. View input hooks retain registration order, and contributed systems use their own `SystemSlot`.

`ui!(compose Name { ... })` declares a Niche with tree and slot attachment but no paint hook. Register its descriptor with `app.with_widget(ui!(compose Name))`. Declared fields retain their Rust types, and parameter snapshots, fallback content, filled slots, and instance isolation remain part of attachment rather than paint traversal.

Event declarations, callback binding, delivery order, and capacity configuration are covered in [Events and callbacks](events.md). Widget-specific notifications are listed in [Widget events](widget-events.md).

## Choosing state and rendering roles

| Role | Use | Update path |
| --- | --- | --- |
| `#[model]` | Registered business state, including multiple instances of one type | Generated handle methods compare `#[observe]` values, publish named revisions, and deliver typed effects |
| `Signal<T>` | A standalone reactive value or a value published by another owner | `set` / `update` notify reactive readers |
| `#[component]` | Data attached to an entity | World insertion and replacement drive component lifecycle; `model!(T)` fields retain registered model identity |
| `#[compose]` | Build a widget subtree | `model!(T)` parameters share instances among generated bindings, callbacks, and direct child calls |
| `com!(entity, T)` | Read a component during composition | Returns a short-lived `Option<&T>` without subscribing; event callbacks use `query!` parameters |
| `View` / `#[view]` | Register a widget kind; nonempty bodies supply painting and empty bodies omit it | Independent hooks provide painting, attachment, gestures, observation, and systems; typed roles declare model projections and resources |
| `#[system]` | Scheduled updates | `model!(T)` captures a registered instance; `res!`, `query!`, and `commands!` declare all other runtime access |
| `on Event { ... }` | Handle gesture and widget business events | `event` borrows the typed payload; `commands!()` defers structural changes; callback parameters provide resource and query access |
| `#[timer]` | Run a callback on a time schedule | `model!`, `entity!`, and `commands!` declare callback access |
| `#[animate]` | Apply a scalar motion to component state | `motion!`, `component!`, `model!`, `entity!`, and `commands!` declare frame access |
| `AudioHandle` | Shared output controls when the `audio` feature is enabled | Host UI and demo controls read one output state and submit to one bounded bus |
| `World` | Low-level entity, component, and resource access | Available for integration code; ordinary model methods and UI callbacks do not need it |

`#[model]` does not replace components: a model has application-level identity,
while components attach behavior and model bindings to individual entities.
`#[view]` does not replace composition: it declares a widget's behavior and optional paint callback,
while `#[compose]` builds its surrounding widget tree. `Signal<T>` remains useful
for a small independent value; a model does not need to wrap every signal.

`#[system]`, `#[event]`, `#[timer]`, `#[view]`, and `#[animate]` share callback
attribute routing. Conditional compilation applies to every generated item.
Function attributes such as `inline` apply to the callback body; documentation
applies to its registration module or generated motion component. `cfg_attr`
entries are split by that purpose, and malformed or nested entries are rejected.

System, event, timer, and View factories preserve authored `self::` and `super::`
paths when generating their registration modules. This includes typed roles,
access plans, View generic bounds and hooks, and timer schedule expressions.
The body remains in its declaration scope. Generic Views forward type and const
arguments explicitly, including types used through associated resource types.

`Slider` input and `prop::SliderValue` use the same clamped value update. A
programmatic property change invalidates the control without emitting a user
gesture event.
`Switch` tap and `prop::SwitchOn` changes also use one animation-seeding path. Assigning the current value leaves the running spring untouched; only taps emit `switch::Toggled`.
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

The macro generates the property application callback internally. Application code declares `@` dependencies and values; the generated callback constructor is exposed only through hidden macro support.

The binding runs after layout and may request another layout pass when it changes a layout property. The pass count is bounded, and unchanged derived values do not invalidate the widget. Use [`SizeChanged`](events.md#layout-size-events) callbacks to update Model state from a node's size.

## Reactive control flow

A `$` on a control-flow head makes its selection reactive. An `if` caches
all branches when its parent is built and toggles visibility when signals
change. A `match` with patterns that do not bind values also caches its arms;
`walk` builds rows as its input changes.

For a retained scrolling list, leave `ScrollConfig` content dimensions at zero to derive scroll bounds from the laid-out children. Adding and removing rows then updates the range without an effect that copies row counts into the scroll configuration. Explicit content dimensions are for virtualized content whose full extent is not present in the tree. Set `clip_children: true` on the scrolling container.

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
- **View rendering**: paint callbacks may read models but cannot update them while painting. Use an input callback or system to change model state before rendering.
- **Computed ownership**: a computed created inside an App belongs to that
  App and cannot be read from another App or after that App ends. An ownerless computed may share
  ownerless signals but cannot read a registered model. Computed evaluation
  cannot write models.
- **Effect ownership**: an effect created outside an App remains worldless until it first reads a registered model, then belongs to that model's App. It cannot read models from two Apps. While unbound, it has no structural World access, including during the run that first binds it; later runs use the bound App's internal execution window. Ordinary author code cannot obtain that World through `core::reactive`; generated tree updates use the hidden macro support boundary.
- **Structural World access**: the internal ambient window is paired with its actual World identity. An owner-only composition scope cannot inherit another World's window; leaving that scope restores owner context without recreating an ended structural window. Generated reactive tree updates use explicit matching windows and retain the existing mutable-borrow checks.
- **Effect delivery**: `#[effects]` extractors return `Option<E>` or `[Option<E>; N]`. Instance consumers registered with `model.on_effect` borrow each present event and cannot start another model write during delivery.
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
