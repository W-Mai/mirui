# Composition, roles, and custom views {#composition-roles-and-custom-views}

How `#[compose]` functions, resource borrows (`res!`), forwarded props (`prop!`), ECS systems (`#[system]`), and custom view descriptors (`#[view]` / `ui!(compose Name)`) fit together.

For reactive models, signals, and stylesheets, see [Reactive state and styles](state-management.md). For event payload routing and gesture recognition, see [Events and callbacks](events.md).

<MiruiSnippet recipe="layout" label="Composition & Flexbox" file="recipes.rs" height="230px">

<template #exercise>

Tap `Toggle width` or drag the bottom-right grip to resize the viewport—the primary card switches between `96 px` and `156 px` while its `grow: 1.0` sibling claims the rest of the row.

</template>

</MiruiSnippet>

<div class="arch-diagram" role="img" aria-label="Role flow from App::compose to ECS systems and custom Views">
  <svg viewBox="0 0 940 186" xmlns="http://www.w3.org/2000/svg">
    <defs>
      <marker id="comp-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
        <path d="M0 0L8 4L0 8Z" fill="var(--site-blue-ink)" />
      </marker>
    </defs>
    <rect x="16" y="24" width="208" height="138" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="30" y="52" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="10.5" font-weight="600">01 · #[compose]</text>
    <text x="30" y="76" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="13.2" font-weight="620">UI Tree &amp; Props</text>
    <text x="30" y="102" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">model!(T) · prop!(T)</text>
    <text x="30" y="122" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">res!(T) · id("name")</text>
    <text x="30" y="144" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="10.5">Stack-held in App::compose</text>
    <path d="M224 93H244" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#comp-arrow)" />
    <rect x="250" y="24" width="208" height="138" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="264" y="52" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="10.5" font-weight="600">02 · #[event] / #[timer]</text>
    <text x="264" y="76" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="13.2" font-weight="620">Input &amp; Callbacks</text>
    <text x="264" y="102" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">event!(Tap) · entity!()</text>
    <text x="264" y="122" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">query!(...) · commands!()</text>
    <text x="264" y="144" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="10.5">Mutates Models &amp; commands</text>
    <path d="M458 93H478" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#comp-arrow)" />
    <rect x="484" y="24" width="208" height="138" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="498" y="52" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="10.5" font-weight="600">03 · #[system] / #[animate]</text>
    <text x="498" y="76" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="13.2" font-weight="620">Frame Schedule</text>
    <text x="498" y="102" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">query!(mut C) · motion!()</text>
    <text x="498" y="122" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">flush_signal_dirty</text>
    <text x="498" y="144" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="10.5">Advances springs &amp; layout</text>
    <path d="M692 93H712" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#comp-arrow)" />
    <rect x="718" y="24" width="206" height="138" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="732" y="52" fill="var(--logo-see)" font-family="var(--font-mono)" font-size="10.5" font-weight="600">04 · #[view]</text>
    <text x="732" y="76" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="13.2" font-weight="620">Paint &amp; Damage</text>
    <text x="732" y="102" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">component!()</text>
    <text x="732" y="122" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">paint!() · watch(...)</text>
    <text x="732" y="144" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="10.5">Submits DrawCommands</text>
  </svg>
</div>

---

## Choosing state and rendering roles {#choosing-state-and-rendering-roles}

| Role | Use | Update path |
| --- | --- | --- |
| `#[model]` | Registered business state, including multiple instances of one type | Generated handle methods compare `#[observe]` values, publish named revisions, and deliver typed effects |
| `Signal<T>` | A standalone reactive value or a value published by another owner | `set` / `update` notify reactive readers |
| `#[component]` | Data attached to an entity | World insertion and replacement drive component lifecycle; `model!(T)` fields retain registered model identity |
| `#[compose]` | Build a widget subtree | `model!(T)` parameters share instances; `prop!(T)` inputs forward static values or getters to consuming bindings |
| `com!(entity, T)` | Read a component during composition | Returns a short-lived `Option<&T>` without subscribing; event callbacks use `query!` parameters |
| `View` / `#[view]` | Register a widget kind; nonempty bodies supply painting and empty bodies omit it | Independent hooks provide painting, attachment, gestures, observation, and systems; typed roles declare model projections and resources |
| `#[system]` | Scheduled updates | `model!(T)` captures a registered instance; `res!`, `query!`, and `commands!` declare all other runtime access |
| `on Event { ... }` | Handle gesture and widget business events | `event` borrows the typed payload; `commands!()` defers structural changes; callback parameters provide resource and query access |
| `#[timer]` | Run a callback on a time schedule | `model!`, `entity!`, and `commands!` declare callback access |
| `#[animate]` | Apply a scalar motion to component state | `motion!`, `component!`, `model!`, `entity!`, and `commands!` declare frame access |
| `AudioHandle` | Shared output controls when the `audio` feature is enabled | Host UI and demo controls read one output state and submit to one bounded bus |
| `World` | Low-level entity, component, and resource access | Available for integration code; ordinary model methods and UI callbacks do not need it |

`#[model]` does not replace components: a model has application-level identity, while components attach behavior and model bindings to individual entities. `#[view]` does not replace composition: it declares a widget's behavior and optional paint callback, while `#[compose]` builds its surrounding widget tree. `Signal<T>` remains useful for a small independent value; a model does not need to wrap every signal.

`#[system]`, `#[event]`, `#[timer]`, `#[view]`, and `#[animate]` share callback attribute routing. Conditional compilation applies to every generated item. Function attributes such as `inline` apply to the callback body; documentation applies to its registration module or generated motion component. `cfg_attr` entries are split by that purpose, and malformed or nested entries are rejected.

System, event, timer, and View factories preserve authored `self::` and `super::` paths when generating their registration modules. This includes typed roles, access plans, View generic bounds and hooks, and timer schedule expressions. The body remains in its declaration scope. Generic Views forward type and const arguments explicitly, including types used through associated resource types.

---

## Compositions and `prop!(T)` inputs {#compositions-and-prop-inputs}

Calling a `#[compose]` function prepares a stack-held composition without executing its body. All arguments remain owned or borrowed by that value until it is executed or dropped. Model validation, resource access, bindings, and widget creation run inside `App::compose`; preparing a composition does not require an active application owner. Composition outputs use concrete Rust types, including generics. Returned references may borrow application data supplied as arguments, but not the construction scope or its resources.

`App::compose` returns the composition's result, including entity IDs and owned values. `Computed` and `Effect` values created in that call belong to the application. Its construction scope does not escape the call; reactive ownership is restored and dirty storage is prepared before the result is returned. Structural World windows are internal to generated bindings and tree updates, not an authoring entry point.

Keep visual styling on the nodes a composition creates. Orbit Console sets its background on its own shell node without changing the caller's parent background.

### Zero-allocation prop forwarding (`Prop<T>` & `prop!(T)`) {#zero-allocation-prop-forwarding-prop-t-prop-t}

`Prop<T>` carries a static value or a shared read-only getter. `Prop::from(value)` stores a value; `Prop::new(move || source.get())` retains a getter without evaluating it. `get()` reads in the caller's reactive scope, so a widget binding subscribes to the original sources. Cloning a dynamic prop shares its getter without allocating or registering another consumer. Each read evaluates the getter; use `Computed` when a result needs caching. Static props add no framework allocation, while each dynamic prop allocates one shared closure during preparation.

Declare composition inputs with `prop!(T)`. Pass an ordinary value for static content or `${expression}` for a getter. Intermediate compositions forward the input directly; the final widget's `${text}` reads the prop in its existing binding. This registers no intermediate effects and does not reconstruct the subtree when a source changes. Static values and `${expression}` results convert to the declared input type through `Into`; String inputs accept `&str` and `String`.

```rust
use mirui::prelude::*;
use mirui::ui::widgets::Text;

#[compose]
fn caption(text: prop!(String)) -> Entity {
    ui! { Text(text: ${text}) }
}

#[compose]
fn header(text: prop!(String)) -> Entity {
    ui! { caption(text) }
}

#[compose]
fn panel(title: model!(Signal<String>)) {
    ui! {
        Column {
            header("Static subtitle")
            header(${title.get()})
        }
    };
}
```

---

## Application resources (`res!`) {#application-resources}

`App::add_resource(value)` immediately installs a resource in the application's existing World storage. Values move into storage without a `Clone` or `Copy` requirement; adding the same type replaces and drops its previous value. Callbacks borrow the current value through `res!(T)` or `res!(mut T)`. Structural changes during a callback continue to use deferred `commands.insert_resource(value)` and `commands.remove_resource::<T>()` after parameter borrows end. Resource installation does not create model observations or a second resource registry.

Within a `#[compose]` Rust body, resource expressions access that composition's existing application storage. Each `with` closure borrows only for its call and returns a value independent of the resource reference.

| Operation | Expression | Result |
| --- | --- | --- |
| Read a required resource | `res!(T).with(read)` | `read` receives `ResourceRef<'_, T>` |
| Read an optional resource | `res!(optional T).with(read)` | `read` receives `Option<ResourceRef<'_, T>>` |
| Mutate a required resource | `res!(mut T).with(write)` | `write` receives `ResourceMut<'_, T>` |
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

Only `res!(optional T)` permits an absent resource. Use required roles for dependencies installed by application or composition setup; use optional roles when absence is part of the callback's behavior, such as checking whether a clock is installed or reading a cache before its first update. Dereferencing a resource never creates a default value.

Compute resource-backed arguments before passing them to `ui!`, `format!`, or another macro; the composition attribute does not rewrite another macro's tokens.

---

## Named widget identities (`id`) {#named-widget-identities}

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

---

## Programmatic builders (`App::compose` closures) {#programmatic-builders}

For construction without the UI DSL, pass a closure to `App::compose`. Its scope accepts components, typed widget builders, and tuples of both. `children` returns the container entity and the closure's result.

```rust,ignore
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

Typed builders retain their widget type across `bind_property`, `on_event`, and style configuration. Properties use the existing reactive effects and property setters; programmatic updates do not emit widget input notifications. `tree.on_event(entity, key, callback)` installs a callback on an already constructed node and validates its attached source.

---

## Systems, queries, and timers (`#[system]`, `#[timer]`, `#[animate]`) {#systems-queries-and-timers}

Systems declare model, resource, query, and deferred-command roles explicitly. The authored function does not receive `World`:

```rust,ignore
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

Queries also support point fetches via `rows.get(entity)`:

```rust,ignore
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

### Timers (`#[timer]`) {#timers-timer}

`#[timer]` declares one schedule (`after = ms`, `every = ms`, `repeat = (count, ms)`, or `until = (deadline, ms)`) and typed callback roles:

```rust,ignore
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

### Scalar motion applicators (`#[animate]`) {#scalar-motion-applicators-animate}

`#[animate]` declares how a scalar `Motion` is applied. It generates the named motion component and a system constructor:

```rust,ignore
use mirui::anim::{Motion, Tween};

#[component]
struct Opacity(Fixed);

#[animate(component = AnimateOpacity)]
fn apply_opacity(value: motion!(), opacity: component!(mut Opacity)) {
    opacity.0 = value;
}

app.add_system(AnimateOpacity::system());
```

---

## Custom Views (`#[view]`) {#custom-views}

`#[view]` generates a filtered View from its component attribute and builds model subscriptions from typed parameter roles:

```rust,ignore
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

- `model!(component.field, watch(selector))` borrows that component's model during paint and subscribes to an `#[observe]` field, getter, or named revision.
- `invalidation = Layout` promotes watched changes from visual to layout invalidation; `Visual` is the default.
- `attach = initialize::callback()`, `input = (tap::callback(), key::callback())`, and `systems = [...]` declare widget lifecycle and input capabilities.
- A custom widget can also expose a reactive attribute on a component field with `#[property(P)]` where `P: Property`.
- An empty `#[view]` body (such as `TabPages` or `LazyList`) registers attachment, observation, and systems without a paint callback.

---

## Reusable Niches (`ui!(compose Name)`) {#reusable-niches}

`ui!(compose Name { ... })` declares a Niche with tree and slot attachment (`@@slot_name` / `@slot_name { ... }`) but no paint hook. Register its descriptor with `app.with_widget(ui!(compose Name))`:

<MiruiSnippet recipe="compose-niche" label="ui!(compose Name)" file="recipes.rs" height="190px">

<template #exercise>

Click `Toggle` inside the injected `@body` slot—`StatusCard` updates its reactive `title: prop!(String)` while hosting the caller's slot children at `@@body`.

</template>

</MiruiSnippet>
