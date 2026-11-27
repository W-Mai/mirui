---
pageClass: tutorial-page
aside: false
---

# Reuse components and styles

As an interface grows, repeating dimensions, radii, and colors on every widget quickly clutters the tree. `mirui` keeps trees concise with **`#[compose]` functions** and **`StyleSheetPlugin` classes**.

## 1. Minimal demo: `prop!(T)` and reactive `class` {#concept}

Here is a two-pill example in isolation:
- `StyleSheetPlugin` registers a base `"pill"` rule alongside state-specific `"pill.on"` and `"pill.off"` rules;
- `#[compose] fn status_badge(...)` accepts `prop!(String)` and `prop!(bool)`, allowing callers to pass either static literals or reactive `${...}` expressions:

<MiruiSnippet recipe="mini-style" label="CONCEPT" file="recipes.rs" height="200px">

<template #exercise>

Tap either pill—both badges flip between `.pill.on` and `.pill.off` through their reactive `class` bindings.

</template>

</MiruiSnippet>

---

## 2. In the task list: reusable filter tabs and completed row styles {#app}

Back in the task list, we extract `filter_button` in `interface.rs` and register the `task`, `task.complete`, and `filter` classes in `lib.rs`. When a task is checked off, `task_row` attaches `"task.complete"` and dims its label automatically:

<ArchFlow kind="uml-ch4-styles" />

<TutorialWorkspace :chapter="4" file="interface.rs">

<template #exercise>

Toggle a few rows and compare the muted ink of completed items against open ones. Click the `lib.rs` tab in the source header to see the `StyleSheetPlugin` registration.

</template>

</TutorialWorkspace>

### What changed in this step {#source}

1. **Zero-overhead `prop!(T)` forwarding**: Declaring `title: prop!(String)` on `filter_button` forwards the getter directly to the inner `Button(text: ${ title })` without creating an intermediate subscription.
2. **Inline attributes override classes**: An attribute written directly on a widget takes precedence over a stylesheet class. Keep state-driven colors in `StyleSheetPlugin` so class switches can control them cleanly.

## Run on your machine {#run}

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 4
```
