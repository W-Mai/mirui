# Reactive state and styles

`mirui` drives widget attributes and tree structure declaratively from `#[model]` instances, `Signal<T>`, `Computed<T>`, and `StyleSheetPlugin`. Mutate a model field or signal and only the widgets that read it update on the next frame—no manual diffing or observer wiring.

For `#[compose]` functions, `res!(T)` resources, `prop!(T)` forwarding, ECS systems, and custom `#[view]` descriptors, see [Composition, roles, and custom views](composition.md). For storage and autosave, see [Persistence](persistence.md).

<MiruiSnippet recipe="reactive" label="Model" file="recipes.rs" height="240px">

<<< @/examples/first-app/src/recipes.rs#recipe-reactive{rust}

<template #exercise>

Tap `+ 10` or `- 10` to mutate `Gauge::level`—the label text, percentage bar width, and threshold color update on the same frame.

</template>

</MiruiSnippet>

<div class="arch-diagram" role="img" aria-label="Reactive transaction and single-frame flush pipeline">
  <svg viewBox="0 0 880 172" xmlns="http://www.w3.org/2000/svg">
    <defs>
      <marker id="state-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
        <path d="M0 0L8 4L0 8Z" fill="var(--site-blue-ink)" />
      </marker>
    </defs>
    <rect x="16" y="22" width="192" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="32" y="48" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">01 · MUTATION</text>
    <text x="32" y="72" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">Model / Signal Write</text>
    <text x="32" y="96" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">gauge.step(10)</text>
    <text x="32" y="116" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">signal.set(next)</text>
    <text x="32" y="136" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">Compares #[observe] snapshots</text>
    <path d="M208 86H246" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#state-arrow)" />
    <rect x="254" y="22" width="192" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="270" y="48" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">02 · DIRTY QUEUE</text>
    <text x="270" y="72" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">Coalesced Subscribers</text>
    <text x="270" y="96" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">${ gauge.level() }</text>
    <text x="270" y="116" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">Computed&lt;T&gt; · Effect</text>
    <text x="270" y="136" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">Enqueued once per frame</text>
    <path d="M446 86H484" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#state-arrow)" />
    <rect x="492" y="22" width="182" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="508" y="48" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="11" font-weight="600">03 · FLUSH</text>
    <text x="508" y="72" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">flush_signal_dirty</text>
    <text x="508" y="96" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">Property::apply</text>
    <text x="508" y="116" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">StyleSheet classes</text>
    <text x="508" y="136" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">Updates only changed fields</text>
    <path d="M674 86H712" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#state-arrow)" />
    <rect x="720" y="22" width="144" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="736" y="48" fill="var(--logo-see)" font-family="var(--font-mono)" font-size="11" font-weight="600">04 · DAMAGE</text>
    <text x="736" y="72" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="14" font-weight="620">Layout / Visual</text>
    <text x="736" y="96" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">PrevRect ∪ CurrRect</text>
    <text x="736" y="116" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="11.5">render_dirty()</text>
    <text x="736" y="136" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="11">Repaints dirty region</text>
  </svg>
</div>

## Contents

1. [Registered model instances (`#[model]`)](#registered-model-instances)
   - [Owned child models](#owned-child-models)
   - [Model roles (`model!(T)`)](#model-roles)
2. [Reactive primitives (`Signal`, `Computed`, `Effect`)](#primitives)
3. [Reusable styles (`style!` & `StyleSheetPlugin`)](#reusable-styles)
   - [Stylesheet classes](#stylesheet-classes)
4. [Reactive attributes (`$`)](#reactive-attributes)
5. [Layout-responsive attributes (`@`)](#layout-responsive-attributes)
6. [Reactive control flow (`if $`, `match $`)](#reactive-control-flow)
7. [Lists: `walk`, index vs keyed](#lists)
8. [The flush model](#the-flush-model)
9. [Limits and contracts](#limits)

---

## Registered model instances {#registered-model-instances}

Place `#[model]` on a struct and one inherent `impl` to generate its instance handle and forwarded methods. Each `app.add_model(value).finish()` call registers an independent instance; cloning a handle refers to the same instance without cloning its data.

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

- **Field getters (`#[observe]`)**: `#[observe]` creates a handle getter for a `Clone + Eq` field (including `String` and `Vec<T>`). Reading `counter.count()` inside `${...}` subscribes the widget to that field. Unchanged values and changes restored within the same transaction do not notify subscribers. Field getters return owned clones rather than references into the Model.
- **Derived getters (`#[observe] fn`)**: An `#[observe]` method taking `&self` with no arguments and a `Clone + Eq` return value publishes only when its computed result changes.
- **Fallible transactions (`Result<Change, E>`)**: Mutating methods return the model's change type (`()` by default) or `Result<Change, E>`. Returning `Err` skips publishing observations or revisions and discards pending effects. Validate before modifying the model—rejected commands are not a rollback mechanism.
- **Named change masks**: For changes that cannot be observed as a small value, declare `#[model(change = ChangeSet, watch(visual = ChangeSet::VISUAL))]`. A mutating method returning a `ChangeSet` containing `VISUAL` increments `model.visual_revision()` and notifies its readers.
- **Business effect extractors (`#[effects]`)**: Declare an effect extractor returning `Option<E>` or `[Option<E>; N]` in the marked `impl`:

```rust,ignore
#[effects]
fn take_note(&mut self) -> Option<Note> {
    self.pending_note.take()
}

player.on_effect(move |note: &Note| play_note(note.pitch))?;
```

`model.on_effect(handler)` installs one consumer per effect type and Model instance. Delivery occurs after state publication and release of the Model data borrow; consumers can read committed state but cannot start another Model write transaction.

### Owned child models {#owned-child-models}

Declare `child: model!(owned Child)` inside a `#[model]` struct to store child data in its parent. Construct the field from raw data with `.into()` and register only the parent. Use `model!(owned ModelList<Item>)`, `model!(owned ModelMap<Key, Item>)`, or `model!(owned ModelSet<Key>)` for collections (see [Model collections](model-collections.md)).

```rust
use mirui::prelude::*;

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
}
```

One outermost mutating method call commits the whole registered tree, whether called through the parent or a child. Observed fields and derived values publish only when their final values differ.

### Model roles (`model!(T)`) {#model-roles}

Model roles keep generated handle types out of application signatures. A `model!(T)` field in a `#[component]` stores one registered instance, while a `model!(T)` parameter in a `#[compose]` function receives one:

```rust
use mirui::prelude::*;
use mirui::ui::widgets::{Button, Text};

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

Generated `ui!` callbacks and direct child compose calls automatically share `counter` without requiring manual `.clone()` calls at each use site.

---

## Primitives (`Signal`, `Computed`, `Effect`) {#primitives}

Three standalone reactive primitives live in `mirui::core::reactive`:

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

let _effect = Effect::new(move || {
    // re-runs whenever a signal read inside it changes
});
```

- `Signal<T>` owns a standalone reactive value. `get()` / `with()` subscribe the current reader; `set()` / `update()` mark subscribers dirty.
- `Computed<T>` derives a value lazily: it only recomputes when one of its inputs changed, and only when read.
- `Effect` runs a closure immediately and re-runs whenever the signals it read change. Inside a composition, `effect_with_widget(entity, closure)` binds the effect's lifetime to a widget entity.

For a full comparison of `#[model]`, `Signal<T>`, `#[component]`, `#[compose]`, and `#[view]`, see [Choosing state and rendering roles](composition.md#choosing-state-and-rendering-roles).

---

## Reusable styles {#reusable-styles}

`style!` produces a typed preset of declared fields. Apply it with `style:` and override individual fields directly on the widget.

```rust,ignore
const STEP: StylePreset = style!(grow: 1.0, height: 44, border_radius: 22);

ui! {
    Row (column_gap: 4) {
        Button (style: STEP, text: "-")
        Button (style: STEP, height: 48, text: "+")
    }
};
```

### Stylesheet classes {#stylesheet-classes}

Register shared class rules when installing `StyleSheetPlugin`. A widget can declare one class, an ordered list, or a reactive expression that selects classes. The last class setting a field wins; the widget's own preset and direct inline fields have higher priority.

<MiruiSnippet recipe="mini-style" label="StyleSheetPlugin" file="recipes.rs" height="200px">

<template #exercise>

Tap either pill—both buttons share the `"pill"` class and dynamically switch between `"pill.on"` and `"pill.off"`.

</template>

</MiruiSnippet>

Use `.bind_class(name, getter)` on `StyleSheetPlugin` or `StyleSheet` to calculate a shared rule from reactive state once per update regardless of how many widgets use it:

```rust,ignore
#[compose]
fn register_compact_steps() {
    res!(mut StyleSheet).with(|mut styles| {
        styles.set("step", style!(grow: 1.0, height: 36, border_radius: 18));
    });
}
```

---

## Reactive attributes {#reactive-attributes}

Inside `ui!`, prefix an attribute value with `$` to bind it reactively. `attr: $signal` is shorthand for `attr: ${ signal.get() }`:

```rust,ignore
let label = Signal::new(0i32);

#[compose]
fn counter_label(label: Signal<i32>) {
    ui! {
        Text (text: ${ alloc::format!("Count: {}", label.get()) }, height: 40)
    };
}
```

When `label` changes, only that attribute updates—the widget is not rebuilt. Reactive binding is supported on:
- **Layout & geometry**: `width`, `height`, `left`, `top`, `direction`, `visible`, `scroll_offset`
- **Visual & style**: `style`, `class`, `bg_color`, `text_color`, `border_color`, `border_width`, `render_key`, `Button.normal_color`
- **Text & paths**: `text`, `font_size`, `Text.paragraph`, `path`
- **Controls**: `ProgressBar.value`, `Slider.value`, `Switch.on`, `Checkbox.checked`

Programmatic updates to `Slider.value`, `Switch.on`, or `Checkbox.checked` through reactive bindings update the control and its animation without emitting user gesture events (`ValueChanged` / `Toggled`).

---

## Layout-responsive attributes {#layout-responsive-attributes}

`$` reacts to application state. `@` reacts to computed layout geometry and lists every dependency in its head:

<MiruiSnippet recipe="state-at" label="@width & @id(stage)" file="recipes.rs" height="200px">

<template #exercise>

Drag the bottom-right resize grip so the container width crosses `320 px`—the top label switches font size via `@width`, while the bottom bar tracks `@id(stage).width / 2`.

</template>

</MiruiSnippet>

```rust,ignore
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

        View(id: "stage", width: Dimension::percent(100), height: 180)

        Text(
            "Stage label",
            width: @id(stage).width { stage.width },
            top: @id("stage").height as stage_height { stage_height / 8 }
        )
    }
}
```

Bare `width` and `height` resolve to the nearest ancestor marked `container: true`. Named dependencies use the widget ID registry. One widget can declare up to 4 distinct geometry dependencies with zero runtime allocation.

---

## Reactive control flow {#reactive-control-flow}

A `$` on a control-flow head makes its selection reactive:

```rust,ignore
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
                Load::Loading => { Text ("loading") }
                Load::Ready(s) => { Text (s) }
            }
        }
    };
}
```

- `if $cond` / `elif` / `else` construct every branch once, then switch visibility without discarding widget state.
- `match $expr` caches every arm when no pattern binds a value (e.g., `Load::Loading`). If any arm binds a payload (as in `Load::Ready(s)`), the match rebuilds its active arm when the source changes so the body receives the new payload.
- A head **without** `$` is static—evaluated once at build time and never re-run.

---

## Lists {#lists}

`walk` iterates a collection. With `$` it re-evaluates when the collection changes:

- **Index-based** (`walk ${ items.get() } with item`): rows align by position. Growing appends tail rows; shrinking despawns tail rows. Best for append/drop-tail logs.
- **Keyed** (`walk ${ items.entries() } with row by row.0`): rows align by stable identity (`ModelItemId` or key). When an item is inserted, removed, or reordered in the middle, surviving rows keep their entities and state.

See [Model collections](model-collections.md) for bounded `ModelList`, `ModelMap`, and `ModelSet` APIs.

---

## The flush model {#the-flush-model}

Setting a signal or mutating a `#[model]` does not mutate widgets mid-callback. It marks subscribers dirty and enqueues them. On the frame schedule—after systems and plugin updates, and right before layout—`flush_signal_dirty` drains the queue once: dirty effects re-run and dirty widget properties apply. Multiple mutations to the same source before a flush coalesce into a single update.

---

## Limits {#limits}

- **Observed values**: `#[observe]` fields and derived getters require `Clone + Eq`. Owned field observations retain one comparison snapshot per registered instance.
- **View dependencies**: `model!(component.field)` alone does not subscribe a custom `#[view]`; add `watch(...)` to subscribe to specific fields, getters, or named revisions. Paint callbacks are read-only.
- **Computed & Effect ownership**: `Computed` and `Effect` instances created inside an `App` belong to that `App` and cannot read models from two different `App` instances.
- **Cached branches (`if $` / `match $`)**: Unselected cached branches pause their display-property bindings until selected again, and their widget IDs must be unique across branches. Put nested reactive `if`, `match`, or `walk` blocks inside a container widget so their dynamic children stay under the branch root.
