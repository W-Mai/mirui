# Reactive State

mirui has a reactive state layer — signals, computed values, and effects —
that drives widget attributes and structure declaratively from the `ui!`
macro. Change a signal and the widgets that read it update; no manual
diffing, no observer wiring.

This guide is the long-form companion to the `mirui::core::reactive` API docs and
the State demos in the gallery.

## Contents

1. [Primitives](#primitives)
2. [Reactive attributes](#reactive-attributes)
3. [Reactive control flow](#reactive-control-flow)
4. [Lists: `walk`, index vs keyed](#lists)
5. [The flush model](#the-flush-model)
6. [Limits](#limits)

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

- `Signal<T>` is the source of truth. `get()` / `with()` subscribe the
  current reader (an effect, a computed, or a reactive binding). `set()` /
  `update()` mark subscribers dirty.
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

    View (text: ${ alloc::format!("Count: {}", label.get()) }, height: 40)
}
```

When `label` changes, only that attribute updates — the widget is not
rebuilt. Reactive binding is supported on `text`, `bg_color`,
`text_color`, `width`, and `height`.

`attr: $signal` is shorthand for `attr: ${ signal.get() }`.

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
            View (text: "visible")
        } elif ${ other.get() } {
            View (text: "alt")
        } else {
            View (text: "hidden")
        }
        match ${ state.get() } {
            Load::Loading => {
                View (text: "loading")
            }
            Load::Ready(s) => {
                View (text: s)
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
        View (text: item.name, bg_color: item.color, height: 28) {}
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
    View (text: item.name, height: 28) {}
}
```

Use keyed when the list reorders or mutates in the middle; index-based is
lighter for plain append/drop-tail.

## The flush model

Setting a signal does not update widgets immediately. It marks subscribers
dirty and enqueues them. Once per frame, after systems and before render,
`flush_signal_dirty` drains the queue: dirty effects re-run, dirty widgets
get re-rendered. Reactivity is tick-driven, consistent with the rest of the
framework — there is no background thread.

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
