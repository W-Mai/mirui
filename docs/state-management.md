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

The default `mirui::prelude` supplies the authoring macros and their common types without importing `World`. Framework implementations, backends, and explicit ECS diagnostics can import `mirui::ecs::World`; generated authoring adapters resolve their internal World paths independently of the user's imports.

`mirui::prelude::draw` supplies drawing commands, path values, and paint-context types. Backend traits use the explicit `mirui::render::{Canvas, Renderer}` paths; low-level View descriptors and registries use `mirui::ui::view::{View, ViewRegistry}`.

`App::add_resource(value)` immediately installs a resource in the application's existing World storage. Values move into storage without a `Clone` or `Copy` requirement; adding the same type replaces and drops its previous value. Callbacks borrow the current value through `res!(T)` or `res!(mut T)`. Structural changes during a callback continue to use deferred `commands.insert_resource(value)` and `commands.remove_resource::<T>()` after parameter borrows end. Resource installation does not create model observations or a second resource registry.

Application setup installs shared services with `app.add_resource(value)` and creates paths through `app.paths()`. Path helpers can accept `mirui::text::PathAccess` to insert or edit paths without receiving the entire World.

Within a `#[compose]` Rust body, resource expressions access that composition's existing application storage. Each `with` closure borrows only for its call and returns a value independent of the resource reference. Resources remain shared services or configuration; mutating one does not add Model observations.

A composition can initialize resources without creating widgets. Gallery font registration uses `app.compose(parent, register_fonts())` to borrow the installed `FontManager` through `res!(optional FontManager).with(...)` before constructing text widgets; it does not replace the manager or require a World parameter.

Optional initialization can check `res!(optional T)` and insert a default through `res!().insert(...)` after the read closure returns. The Gallery clock composition uses this pattern to preserve a host-provided `MonoClock` and its timing state.

| Operation | Expression | Result |
| --- | --- | --- |
| Read a required resource | `res!(T).with(read)` | `read` receives `&T` |
| Read an optional resource | `res!(optional T).with(read)` | `read` receives `Option<&T>` |
| Mutate a required resource | `res!(mut T).with(write)` | `write` receives `&mut T` |
| Install or replace a resource | `res!().insert(value)` | Immediate replacement; the previous value drops |
| Remove a resource | `res!().remove::<T>()` | `Option<T>` |

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

`App::compose` returns the composition's result, including entity IDs and owned values. Its construction scope does not escape the call; reactive ownership is restored and dirty storage is prepared before the result is returned.

A prepared composition also works as the child callback of `tree.children(recipe, panel(...))`. The returned `(container, result)` includes the actual child entities, so sibling components can reference those IDs without a later lookup or mutation pass. Snapshot and diagnostic scenes use the same construction entry; their framebuffer capture and renderer inspection remain separate from composition. See [snapshot tools](../gallery/examples/tools/README.md) for local capture commands.

Calling a `#[compose]` function prepares a stack-held composition without executing its body. All arguments remain owned or borrowed by that value until it is executed or dropped. Model validation, resource access, bindings, and widget creation run inside `App::compose`; preparing a composition does not require an active application owner. Composition outputs use concrete Rust types, including generics. Returned references may borrow application data supplied as arguments, but not the construction scope or its resources.

For construction without the UI DSL, pass a closure to `App::compose`. Its scope accepts components, typed widget builders, and tuples of both. `children` returns the container entity and the closure's result. No manual parent wiring is required.

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

The I18n composition borrows an optional FontManager to register its font and installs its translation resource through `res!().insert(...)`. Its locale Signal is captured before installation and remains the language button's state source.

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

Register the instance before building controls. A `model!(Counter)` compose
parameter accepts the value returned by `App::add_model`:

```rust
let root = app.spawn_root().id();
let counter = app.add_model(Counter { count: 0 });
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

Button, Slider, Switch, Checkbox, ProgressBar, TabBar, Text, TextInput, Icon, Image, and BackgroundBlur use this declaration for built-in paint callbacks. Their widget modules export the generated `view` factory directly; component filters, paint priorities, and internal gesture hooks remain part of the descriptor.

StaticGlyphRun declares its component filter, paint priority 80, and layout invalidation through `#[view]`. It submits its borrowed positioned glyph slice without runtime shaping or a layout workspace. FontManager and Theme are borrowed as optional resources, retaining the empty-run and missing-font-manager early returns; a run that reaches painting requires a Theme.

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
View resources are read-only. `res!(T)` requires the resource at each paint invocation; `res!(optional T)` supplies `Option<&T>` and still runs the body when the resource is absent. Both use the same resource role grammar as systems and events, without cloning the resource or allocating parameter storage.

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

Required items, optional items, and `with`/`without` filter types form static View dependencies. Component writes and insertion, removal, or despawn of those types invalidate that descriptor's instances according to its Visual or Layout policy. Tracking is conservative: obtaining a mutable component reference counts as a possible write, and changing an unrelated entity of the same type can repaint another instance. This does not create per-fetch subscriptions or rebuild the composition tree; registering types prepares tracking, and repeated writes and invalidation reuse existing storage.

Pending dependencies are consumed before layout and after layout before the dirty-region plan. A changed `ComputedRect` therefore reaches querying Views in that frame. Changes produced by dirty-plan generation itself, such as `OffscreenGeneration` or scroll residue, remain pending for the next frame rather than replaying the walker. A View whose redraw changes its own queried dependency can continue to invalidate itself; dependency tracking does not suppress that feedback.

Switch queries its component together with optional background and thumb-animation state. Its generated View retains the same attachment, gesture handler, paint priority, and animation systems.

Image queries its component together with its optional resource lease and borrows the texture manager and Theme as optional resources. A cached lease can be used without a manager; replacing a manager affects direct resolutions and newly created leases rather than retargeting an existing lease. Vector paths remain borrowed, and a drawable vector requires Theme only after its geometry is validated.

TextInput queries its optional Placeholder and borrows layout, font, theme, and blink resources. Font-stack resolution receives the font manager directly, validates fallback capacity before handling a missing manager, and retains the existing font selection and layout cache. Attachment, input hooks, cursor animation, and paint priority remain part of its generated descriptor.

Text queries its optional layout handle and TextPath and borrows font, layout, theme, and path resources. Linear and path text retain their borrowed glyph data, draw clipping, text-run identity, and layout invalidation policy. Layout publishes a handle only when its identity changes, so unrelated layout work does not publish a same-value query change. Cache replacement and memory-pressure resets explicitly invalidate handle dependencies because a new cache can reuse the same handle numbers.

Shapes and Butterfly borrow optional Theme and MonoClock resources; Vector
Mandala borrows an optional MonoClock. Their generated paint adapters retain
the default theme and zero-time behavior when those resources are absent.
FillRules, ClipPath, and StrokeStyles borrow an optional Theme. Their paths
remain static, and ClipPath uses the paint context's scoped clip cleanup.
ScopeCanvas observes its model's `visual_revision` and borrows optional waveform
scratch storage. Replacing the component's model rebinds the observation; absent
scratch storage skips painting. Waveform paths reuse buffers prepared at setup.
CurvePaths borrows optional path IDs, a path store, and a theme. Missing path
resources skip painting; retired path IDs are skipped without copying paths.
RasterContour receives its component and optional FontManager/Theme resources.
Missing fonts or unsupported raster representations retain the no-paint behavior.

The generic Style pass uses `#[view]` with a required Theme resource and `paint!()`. It retains paint priority 50 and layout invalidation, skips a background already handled by an earlier widget, and draws the border through the same paint context.

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

Registration retains a shared snapshot of attachment and internal-gesture hooks in registration order. Per-node attachment reuses that snapshot without allocating a hook list. Hooks that replace or extend the registry do not change the active invocation; the next invocation uses the updated registry. Registration can allocate metadata, and whole-tree attachment still uses its traversal stack.

`ui!` seeds the primary component and static text content, prepares binding captures, inserts static component attachments in source order, and invokes matching attachment hooks once before running reactive initial values. Preparing captures does not evaluate reactive expressions or register effects. Layout containers with component attachments participate in the same initialization; plain containers without a matching gesture capability do not gain input feedback. Prepared callbacks stay on the construction stack until installation and reuse the existing effect runtime.

LazyList creates its missing runtime pool on the first system run from its declared direct children, then binds rows in that same run. The pool is a retained snapshot, not a live mirror of Children. An explicit `LazyListPool` selects custom slots and is preserved. Missing or insufficient children leave initialization pending until a later system run; row declarations do not require post-composition pool installation.

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
Paint priority orders only drawing, with registration order
breaking ties. Gesture hooks retain registration order, and contributed systems
use their own `SystemSlot`.

`ui!(compose Name { ... })` declares a Niche with tree and slot attachment but no paint hook. Register its descriptor with `app.with_widget(ui!(compose Name))`. Declared fields retain their Rust types, and parameter snapshots, fallback content, filled slots, and instance isolation remain part of attachment rather than paint traversal.

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
The default Tap key matches every tap. An explicit count matches only that
multiplicity, so `Tap::EVENT.configure((1,))` differs from `Tap::EVENT`.
Counts outside 1 through 255 return `TapCountError`. Other gesture keys have no options.

Event blocks, inline callbacks, ordinary Rust payload functions, and named `#[event]` callback factories use the same installer and prepared executor. A block reads its payload through `event`; a single untyped closure parameter receives the payload inferred from the selector. A function passed directly receives `&E` and returns a result permitted by the event's route. Callback parameters use `event!(E)`, `entity!()`, `res!(...)`, `query!(...)`, and `commands!()` for invocation-time access. Models bind through the enclosing composition or a named callback factory.

```rust,ignore
Button(text: "DROP") on Tap { game.drop_ball(); }

View () on Tap(|input| { game.select_at(input.x, input.y); })

View () on DragMove(|input: event!(DragMove), node: entity!(), items: query!(mut Draggable)| {
    if let Some(item) = items.get(node) {
        item.move_by(input.dx, input.dy);
    }
})

View () on DragMove(moved::callback(game, step))

View () on Tap(handle_tap)
```

An event block treats all parenthesized arguments as selector options. Without a block, the last argument is the callback and any preceding arguments are options: `on Tap(2, callback)`. Node children precede trailing callback bindings: `View () { child_panel() } on Tap(callback)`.

`on Tap(n) { ... }` evaluates its `u32` configuration once when installing the handler, using the same validation as a Tap key. Invalid counts log a warning and skip that binding without failing UI construction or suppressing other valid handlers. A node with no valid gesture bindings receives no authored gesture handler or extra hit target. Dispatch compares the observed tap count with the stored options without reevaluating `n` or repeating installation warnings.

Ordinary and counted Tap handlers run in declaration order, stopping at the first consuming handler. An ordinary Tap is not a fallback for counted handlers. All nodes in a bubbling gesture use the original target's count, frozen before internal or authored callbacks run.

```rust,ignore
View ()
    on Tap { game.record_tap(); false }
    on Tap(2) { game.record_double_tap(); }
```

The first handler runs on every click and allows the second handler to run
on a double tap. Returning `()` or `true` instead stops that sequence.

`Key` and `CharInput` carry their platform fields and support entity and App input source capabilities. `InputSelectors` supplies their component-node keys; focused nodes and ancestors run before App fallback. `Wheel` carries its position and deltas; `Rotary` carries its device id and delta, without fabricated pointer coordinates. Wheel and rotary callback sources are App-only.

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

### Layout size events

`SizeChanged` reports a node's first available logical size and subsequent width or height changes. `event.old` is `None` for a listener's first delivery; `event.new` is a `LayoutSize` with `width` and `height`. Position, scroll offsets, output scaling, and visual transforms are not size changes. Hidden or detached nodes are not measured for this notification.

```rust,ignore
View(width: Dimension::percent(100), height: 360) on SizeChanged {
    model.set_stage_size(event.new.width, event.new.height);
}

View() on SizeChanged(|change: event!(SizeChanged)| {
    model.set_stage_size(change.new.width, change.new.height);
})

#[event]
fn resized(model: model!(PanelModel), change: event!(SizeChanged)) {
    model.set_stage_size(change.new.width, change.new.height);
}

View() on SizeChanged(resized::callback(model.clone()))
```

`SizeChanged`, `LayoutSize`, and `LayoutSelectors` are exported by the ordinary prelude. Typed builders use `on_event(SizeChanged::EVENT, callback)`. Size notifications return `()`, do not bubble, and do not add hit targets or interaction feedback. They are produced by layout, cannot be submitted through `commands.emit`, and cannot be installed on App.

All sizes in a pass are captured after computed rectangles are written, before callbacks run in installation order through the shared event executor. Query and resource parameters retain their existing access rules; commands apply after parameter borrows end. Reactive updates participate in the same frame's layout settling. A listener can receive several distinct sizes in one frame: this is not a final-layout event. Feedback is bounded; non-convergence logs a warning and retains pending layout work while rendering and hit testing use the last completed snapshot. Callback mutations are not rolled back.

Observation storage is prepared from the actual number of installed size listeners and reused during capture and delivery. Size snapshots do not consume business notification payload capacity; notifications emitted by their callbacks still use `EventCapacity`. Callback registration and structural construction retain their normal allocations.

Curve Text uses a size event to update `CurveModel.stage_size`; its three `Text.path` properties observe that value with `$` bindings. Resizing updates their path ranges even while animation is paused. Use `@` dependencies for cross-node layout properties and size events for geometry-driven Model state; neither requires an author callback that receives World.

### App callbacks and event capacity

`App::on_event(E::EVENT, callback)` installs a named callback for App lifecycle notifications or input fallback. The callback factory infers the installation context; authors do not name an internal context type or write a World callback.

```rust
use mirui::prelude::*;

#[model]
struct Shortcuts {
    enabled: bool,
}

#[model]
impl Shortcuts {
    fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }
}

#[event]
fn handle_key(input: event!(Key), shortcuts: model!(Shortcuts)) -> bool {
    if input.pressed && input.code == 32 {
        shortcuts.toggle();
        true
    } else {
        false
    }
}

let mut app = App::headless(320, 240);
let shortcuts = app.add_model(Shortcuts { enabled: false });
app.on_event(Key::EVENT, handle_key::callback(shortcuts));
```

Focused keyboard handlers and default text editing run before App key and character fallback. Wheel and rotary events first try the scroll route. An accepted input is not dispatched again through a second handler path. App fallback also works without a widget root. Plugin consumption can stop user input, but does not stop lifecycle requests from reaching other plugins or bypass cancellation.

Atlas Restoration, Echo Walker, Pocket Post, Twin Beacons, and Folding Ark bind keyboard controls with `#[event]`, `event!(CharInput)`, and `model!` parameters. Their shortcuts use App fallback rather than a lifecycle plugin or an explicit Model handle type. Registered instances are shared with the rest of each game; character input consumed by a focused editor does not trigger the shortcut.

Twin Beacons and Folding Ark install their hint services with `App::add_resource` before binding the keyboard callbacks, which borrow them through `res!` on each invocation. At setup, each game's keyboard and hint controls share the same search workspace; subsequent keyboard calls read the current installed service.

`Started` follows the startup hooks once, including applications driven directly by `tick`. Suspend and resume notify only after an actual state change and their plugin hooks; suspension and quit cancel active pointer, scroll, and gesture state. Memory warnings and host interactions notify after their hooks. Quit hooks and notifications run once even if `tick` is called again. These notifications join the same FIFO as previously submitted widget notifications.

App provides a bounded event preset automatically. Ordinary widget notifications and short deferred callbacks need no capacity declaration. The preset reserves 8 business notification records and 64 payload bytes per buffer, 4 deferred command records per callback, and at most 16 notification deliveries in one outer flush. Notification storage supports the alignment of built-in widget payloads; the command payload budget is calculated from one theme change and three transform changes.

The automatic preset prepares storage only when installation needs it. Without an explicit override, applications using only direct input and Model callbacks without deferred commands do not allocate business notification storage; callbacks without `commands!()` do not allocate command queues. A deferred callback prepares notification storage because commands can emit notifications. An explicit capacity declaration prepares its requested notification storage immediately. Notification buffers are shared by the App, while each deferred callback owns a separate reusable command queue. The command quota applies to one callback invocation, not the sum of commands from every callback in a frame.

`EventCapacity::default()` selects that preset. `EventCapacity::new()` starts an explicit empty declaration; it does not inherit the preset. Applications needing larger payloads, longer notification chains, or batches of commands can replace the preset through `App::with_event_capacity` before installing the first callback. `EventCapacity` is available from `mirui::app` and the ordinary prelude:

```rust,ignore
app.with_event_capacity(
    EventCapacity::new()
        .notifications::<PadHit>(MAX_PENDING_HITS)
        .commands(CommandCapacity::new().emit::<PadHit>(MAX_HITS_PER_CALLBACK))
        .flush_limit(MAX_DELIVERIES),
);
```

Explicit notification counts describe combined pending payload storage, not per-type runtime quotas. A nonzero notification declaration needs an explicit flush limit. Omitted notification or command storage in an explicit declaration does not inherit the corresponding preset budget. An explicitly declared zero quota supplies no storage for that operation.

The first successfully installed callback freezes the capacity configuration. Reapplying the same configuration is a no-op; an invalid or different configuration logs a warning and preserves the installed configuration. A rejected binding logs a warning and skips that binding. Dispatch never grows the prepared buffers. Runtime submission and flush failures retain their structured error contract internally and are logged by App; completed data changes are not rolled back, and failed work is not automatically retried. An interrupted notification flush discards its remaining notifications and callback writes.

App lifecycle listeners reserve one additional unit-notification slot, independent of business storage. Business notifications cannot borrow that slot. Listener and route scratch storage are prepared from actual installed listeners, matching gesture hooks, and keyboard handlers at structural boundaries. Stable dispatch does not grow those buffers; route snapshots and callback borrows finish before further preparation.

### Notification payload storage

The notification service and its retained handle are private. Runtime preparation reserves an existing service in place instead of replacing it; generic resource commands cannot remove or replace that service. Preparing capacity during an active flush returns `Busy`, while normal emission continues into the separate write buffer.

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

`CommandCapacity` is available from `mirui::ecs` and the ordinary prelude. Its typed operation counts describe deferred storage without exposing arena bytes or alignment. `insert`, `remove`, `despawn`, `insert_resource`, and `remove_resource` describe structural operations; `set`, `set_theme`, and `emit` include their actual deferred apply payloads. Capacity construction is allocation-free and supports const declarations.

```rust
use mirui::prelude::*;

#[component]
struct Marker;

const COMMANDS: CommandCapacity = CommandCapacity::new()
    .insert::<Marker>(1)
    .despawn(2);
```

Each count reserves a combination of operations, not a per-type runtime quota. The declared combination fits in any recording order, including mixed alignments and empty operations. Recording a different, larger combination can still exceed the total record, byte, or alignment limit. Arithmetic overflow is returned by bounded preparation rather than wrapping or reducing the declaration. Prepared queues retain only the resolved limits, not the capacity-construction statistics.

Named event factories derive installation inputs, typed invocation parameters, access metadata, and body arguments from one ordered parameter plan. Authored types remain in their declaration scope; relative paths are relocated only when emitting the generated factory module. Model and ordinary value arguments retain their declaration order.

Internal typed callbacks share the system parameter fetch and access validator. Preparation rejects component/resource aliases and foreign, mixed, or retired declared Model bindings with structured installation errors. App logs these errors and skips the affected binding. Successful preparation binds the callback to one App and reserves the current query change-record budget. The prepared callback and its access plan cannot be replaced independently.

Event factories accept the same shared binding values as composition. Optional bindings recursively contribute their registered Model identity; `None` contributes none. Signal, Computed, and audio handles retain their own access rules instead of acquiring a Model registration. Capture requirements merge without retaining Model cells or allocating owner storage. This is an installation identity check, not a new lifetime lease or poison check.

Ordinary Rust captures retain their value semantics. The framework does not inspect or reject manually constructed containers to discover Models inside them. Model access still enforces the registered App and lifetime; an invocation-time failure does not roll back values already changed by the callback.

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

Deferred operations can fail during submission as well as recording. `CommandError::Commit` distinguishes submission failures from command-record limits. Submission stops at the first error, drops unexecuted payloads, and preserves completed operations. No callback borrow remains active during submission.

`SystemScheduler::run_all` returns `Result<(), SystemRunError>`. A failure identifies the system and its `CommandError`, stops the remaining systems in that batch, and preserves completed updates. Failed calls contribute timing when a clock is installed; unexecuted calls do not. App logs scheduling failures and completes rendering and the frame. Owner, access-plan, and required-resource validation remain programming-error checks.

Timer callbacks consume their invocation after the body completes. Command submission errors still advance periodic schedules, decrement repeat counts, and remove completed one-shot or deadline timers. The error stops the remaining timers and returns through `SystemScheduler::run_all`; already completed updates are not rolled back or automatically retried. Timer command storage remains reusable after failure. Ordinary `#[system]` and `#[timer]` functions retain their authored signatures; fallible execution belongs to the internal wrappers.

This execution layer does not install or route listeners. Its ABI is confined
to hidden macro support, not the ordinary authoring prelude.

### Listener selection and notification delivery

The internal listener pool installs prepared callbacks for entity and App
targets. Widget notification installation checks the source component and
entity generation. Captured Model owners are validated by preparation; the
pool rejects a different App before installation or dispatch.

Typed widget contexts constrain a notification source to the widget's component type at compile time. Generic node contexts validate the source component after attachments have been installed. Both contexts use the same installer, prepared callback, and listener pool; an event qualifier does not prove that a component exists. App-only sources cannot acquire a node context.

Listener entries and snapshot steps have independent explicit count limits.
Matching produces an installation-order snapshot without borrowing the World
or retaining a borrow of the entry list. Removing a listener does not change
the current selection; a new listener participates in later selections. Each
callback checks entity generation before running. Notification callbacks fan
out; consumable input stops on a prevention result.

Installation and fresh route selection reclaim listeners for retired entity
generations before checking the installation quota or choosing callbacks.
Reclamation retains entry storage and live installation order. Captures are
dropped outside the entry borrow; an existing snapshot keeps its shared callback
until that route ends. App listeners remain installed until removal or pool
disposal. Removing a source component alone is not entity retirement.

Notification draining uses this pool and the same prepared executor. Payloads
are borrowed without cloning, and reserved invocation scratch is cleared on
success, error, and unwinding. Reentrant use of that scratch returns an error.
Selection and draining within reserved capacities allocate no listener storage.

Gesture selection freezes the complete Parent chain before running any body.
Each node contributes matching internal View hooks in registration order, then
its authored listeners in installation order. Reparenting, removing a filter
component, or changing listeners does not alter that selection. Each step still
checks entity generation before execution. Hidden or disabled nodes and ancestors
suppress the route before any steps are selected. Eligibility is frozen with the
route; later state changes do not rewrite its selected steps. Business
notifications do not use input eligibility and still reach live disabled or
hidden widgets. Parent cycles and insufficient step capacity fail before any
body runs.

Keyboard selection freezes the focused node, ancestor chain, and default editing
hooks before running any body. Each node contributes authored listeners first,
then its default editor; App fallback comes last. A default editor's acceptance
stops the route even when its content or caret does not change. Hidden, disabled,
missing, or generation-stale focus skips entity input but still permits App
fallback. Key and character payloads share this route; wheel and rotary payloads
cannot use it. The keyboard route reuses the same step storage, generation
checks, owner scope, and error cleanup as gesture and notification delivery.

The internal scroll handler reports wheel and rotary acceptance separately
from offset changes. A wheel spring submitted on a matching axis is accepted
even at a clamped boundary. Rotary input requires a live retained target with
a scroll offset; retired targets and targets without that component are not
marked dirty. The listener pool's scroll route runs that handler first and
selects App listeners only when scrolling is not accepted. It retains wheel
coordinates and rotary hardware fields, uses the same prepared callbacks and
reserved step storage, and rejects foreign owners or busy scratch before
scrolling. App fallback uses this route after scroll acceptance is checked.

Internal hooks and listeners share the reserved step storage and cleanup guard.
A failed reservation does not raise either logical quota. The pool's ABI is confined to hidden macro support. App callbacks, widget builders, and all three `on` forms install into the same prepared listener pool.

### Deferred widget notification submission

The internal command facade records `emit(entity, payload)` without reading the World or borrowing notification storage. The payload must declare an entity notification route; input and App lifecycle payloads cannot be emitted through this capability. Recording a request does not mean that it has been delivered.

After callback borrows end, submission checks the entity generation, actual source component, and notification storage owner. The owning payload moves from command storage into the pending notification arena without cloning. Missing sources, missing storage, and storage limits return structured errors through the prepared event executor. Source removal earlier in the command batch is visible to a later submission. Already completed data changes are not rolled back.

Pending storage is retained by an owner-bound shared runtime resource. Draining retains a shared reference before acquiring the World, so callbacks can submit further notifications without removing that resource. The listener pool drains the queue in FIFO order with stable read payloads and separate write storage. App prepares business storage from the automatic preset or an explicit `EventCapacity` and flushes at input, system, render-hook, and lifecycle boundaries.

### Recognized gesture batches

App input, `SimulatedInput`, and `SimTimeline` dispatch recognized gestures with the current clock time. Tap focus is updated before internal and authored handlers; hidden targets leave focus unchanged. Simulated recognizer input and tap-count tracking use the same tick timestamp, so successive taps retain their multiplicity.

App input and both simulation routes share a retained gesture batch handoff.
The installed `GestureSystem` prepares a write buffer and a separate read
buffer. Dispatch swaps their storage and drains the selected read batch without
borrowing the World resource during callbacks or collecting a temporary Vec.
Events queued by a callback stay in the write buffer, outside that batch.

Recursive draining of the same read buffer returns `Busy`. Dispatch errors and unwinding clear unread events while retaining capacity and newly queued write events. Removing or replacing `GestureSystem` does not invalidate the selected batch or consume the replacement's events. Standalone `GestureEvents` retains only its ordinary output buffer.

Each complete gesture route releases its listener snapshot before flushing widget notifications. The next gesture in the retained batch runs afterward; an arbitrary platform poll burst therefore does not accumulate all its widget notices into one business buffer. Inner notification delivery keeps FIFO order and its explicit feedback limit.

Builtin widgets enqueue notifications only when that entity has a listener for the exact payload type. Unsubscribed controls do not need business notification storage. Programmatic property updates do not emit user notifications. Explicit `commands.emit` still validates storage even without a listener; missing or exhausted capacity never triggers hidden growth, and completed state changes are not rolled back.

This handoff allocates no storage during dispatch. Recognizer writes retain
their existing growth policy. The handoff does not impose a bounded input-burst
quota or change callback selection.

### Named event callback declarations

`#[event]` generates a module with `callback(...)`. The factory takes registered
Model instances and ordinary `Copy + 'static` values in declaration order.
Resources, queries, the payload, the current entity, and deferred commands are
invocation parameters, not factory arguments.

Types and const arguments using `self::` or `super::` retain their authored
module scope in the generated factory. Imported and absolute paths are
unchanged.

For example:

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

`moved::callback(game, step)` describes the callback without exposing World or generated handle types. `event!(E)` borrows a concrete payload; omitting it leaves the event type to the binding site. `entity!()` excludes App-only bindings. Notification callbacks return `()`; consumable routes also accept `bool` or `BubbleControl`. Named factories and inline `on` callbacks share System resource/query lowering and the prepared executor.

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

The binding runs after layout and may request one additional layout pass when it changes a layout property. The pass count is bounded, and unchanged derived values do not invalidate the widget. Continuous geometry that cannot be expressed as a widget property can use `UiScope::bind_layout` with explicit `LayoutDependency` values and caller-owned state.

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
- **Structural World access**: the internal ambient window is paired with its actual World identity. An owner-only composition scope cannot inherit another World's window; leaving that scope restores owner context without recreating an ended structural window. Generated reactive tree updates use explicit matching windows and retain the existing mutable-borrow checks.
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
