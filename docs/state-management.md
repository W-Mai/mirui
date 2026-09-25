# Reactive State

mirui has a reactive state layer — signals, computed values, and effects —
that drives widget attributes and structure declaratively from the `ui!`
macro. Change a signal and the widgets that read it update; no manual
diffing, no observer wiring.

This guide is the long-form companion to the `mirui::core::reactive` API docs and
the State demos in the gallery.

## Contents

1. [Registered model instances](#registered-model-instances)
2. [Primitives](#primitives)
3. [Reactive attributes](#reactive-attributes)
4. [Layout-responsive attributes](#layout-responsive-attributes)
5. [Reactive control flow](#reactive-control-flow)
6. [Lists: `walk`, index vs keyed](#lists)
7. [The flush model](#the-flush-model)
8. [Limits](#limits)

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

Bound declarations retain the model type in source while storing its shared
handle:

```rust
#[component(bind(counter))]
#[derive(Clone)]
struct CounterTile { counter: Counter }

#[compose(bind(counter))]
fn counter_controls(counter: Counter) {
    ui! {
        Row {
            Button(text: "−") on Tap { counter.decrement(); }
            Button(text: "+") on Tap { counter.increment(); }
            Text(text: ${ counter.count().to_string() })
        }
    };
}
```

`CounterTile` can be cloned even though `Counter` is not `Clone`. Only names
listed in `bind(...)` are converted; other fields and parameters keep their
declared types. The generated `ui!` callbacks share each bound handle, so
multiple callbacks can use one model without manually cloning it.

Systems can bind a registered model instance and read small `Copy` resources
without exposing `World` to the system function:

```rust
#[system(order = ANIMATION, bind(counter))]
fn advance(counter: &Counter, delta: mirui::ecs::DeltaTimeMs) {
    for _ in 0..delta.0 {
        counter.increment();
    }
}

app.add_system(advance::system(counter.clone()));
```

Each registration keeps its own instance. Other model parameters may be listed
in `bind(...)`; they are passed to the generated `system(...)` constructor in
the same order. An unbound `T` parameter is read as a required `Copy` resource;
`Option<T>` reads an optional one. Missing required resources and cross-App
model bindings fail explicitly. The callback is allocated at registration,
with no allocation per invocation. Existing `fn(&mut World)` systems and
`const fn` system descriptors remain available.

`World::watch_component_type` records insertions, replacements, removals, and
despawns for selected component types. Repeated changes to one entity and type
are coalesced until `drain_component_changes` reads the final state. The queue
can be reserved ahead of a batch. Direct field writes through `World::get_mut`
do not create a structural change record; replace the complete component when
its model binding changes.

An observed `View` can use `with_filter::<Component>()` and
`with_observation(...)` to hold model-source subscriptions for each matching
entity. After a complete component replacement, the previous subscriptions
are released and the new instance is attached before painting; removal and
despawn release them as well. Registering a View after its components exist
also attaches those existing entities. The View retains its usual render
priority, attach callback, internal gesture, and static systems.

`#[view]` generates the filtered View and its observation callback from a
typed paint function:

```rust
#[view(
    component = CounterTile,
    read(counter),
    watch(counter.count()),
    priority = 60,
)]
fn paint_counter(counter: &Counter, ctx: &mut ViewCtx<'_>) {
    // Read model data and set paint context state.
}

app.with_widget(paint_counter::view());
```

`read` borrows the current component's model only during paint; it does not
create a subscription by itself. `watch` accepts marked observed getters and
named revision getters. The generated adapter holds no model borrow after
paint and rejects model writes inside that callback. Multiple model fields,
including two handles to the same instance, can be read together. `attach`,
`gesture`, and static `systems` can be forwarded to the existing View builder.
Subscriptions reserve visual notification queue capacity when attached; model
updates and notifications reuse that capacity. Component replacement and
removal update bindings before the next paint.

`Slider` input and `prop::SliderValue` use the same clamped value update. A
programmatic property change invalidates the control without emitting a user
gesture event.

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

ui! {
    :(
        parent: root
        world: &mut world
    :)

    Text (text: ${ alloc::format!("Count: {}", label.get()) }, height: 40)
}
```

When `label` changes, only that attribute updates — the widget is not
rebuilt. Reactive binding is supported on `text`, `path`, `visible`,
`bg_color`, `text_color`, `render_key`, `font_size`, `direction`, `width`,
`height`, `Button.normal_color`, `Text.paragraph`, `ProgressBar.value`,
`Slider.value`, and `Switch.on`.

Reactive `Slider.value` clamps to the control range and does not emit a
`ValueChanged` event when the model updates it. Reactive `Switch.on` starts
the switch animation when the model changes its state without emitting
`Toggled`. User input still emits those events. A large simulation model can
stay in a World resource while a small scalar signal drives its labels,
visibility, colors, and controls; Marble Play uses this split without copying
physics arrays into reactive state.

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

A `$` on a control-flow head makes the branch reactive: when the head's
signals change, the subtree is rebuilt.

```rust
let show = Signal::new(true);
let other = Signal::new(false);
let state = Signal::new(Load::Loading);   // your own enum

ui! {
    :(
        parent: root
        world: &mut world
    :)

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
}
```

- `if $cond` / `elif` / `else` swap one single-root branch in place.
- `match $expr` selects one arm and rebuilds it on change.
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

A reactive binding's first run applies its initial value at construction
(inside the `ui!` build), so the first frame already shows the correct
state.

## Limits

- **Single-root reactive branches**: each `if` / `match` / `walk` reactive
  branch produces one top-level widget, matching SolidJS / Leptos. Wrap
  multiple widgets in a container.
- **Reactive blocks mount after static siblings**: a reactive `if` / `match`
  inside a container whose other children are static appears after them on
  first build, regardless of source order. The branch keeps its position
  across swaps thereafter.
- **Index-based `walk` does not update surviving rows' content**: with no
  `by` key, a middle insert/remove shifts which data each surviving row
  shows only through that row's own reactive attributes; the row structure
  itself is not re-matched. Use keyed `walk` when identity matters.
