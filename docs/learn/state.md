---
pageClass: tutorial-page
aside: false
---

# Complete a task

In chapter 1 the rows were static labels. Now we introduce reactive state (`#[model]`) so tapping a row flips its checkmark and updates the header tally on the same frame.

## 1. Minimal demo: `#[model]` and `${...}` bindings {#concept}

Annotate a struct with `#[model]` and mark observable fields with `#[observe]`. The macro generates a getter method for each observed field (`state.checked()`, `state.taps()`).

Calling those getters inside `${...}` subscribes the widget attribute to the field; mutating the model inside `on Tap` updates every subscriber automatically:

<MiruiSnippet recipe="mini-state" label="CONCEPT" file="recipes.rs" height="200px">

<template #exercise>

Tap the button on the right: the button label, background color, and tap counter all react to `state.flip()` without manual widget mutation.

</template>

</MiruiSnippet>

---

## 2. In the task list: syncing rows and the header count {#app}

Now bring the same pattern into the task list. In `model.rs` we mark `done` with `#[observe]` and add a derived `completed()` query on `Tasks`; in `interface.rs` each row wraps its content in `Button ... on Tap { task.toggle(); }`.

With the **eye button** on, the source pane gathers the focused regions across **`model.rs` · `interface.rs` · `lib.rs`** and highlights the lines added in this step (`+`). Click any file tab or turn the eye off to read the full files:

<TutorialWorkspace :chapter="2" file="model.rs">

<template #exercise>

Tap `Connect the display`: its checkmark appears and the header count steps from `1 of 3` to `2 of 3`. Tap again to undo.

</template>

</TutorialWorkspace>

### What changed in this step {#source}

1. **Single source of truth**: The header's `format_args!("{} of {} complete", tasks.completed(), tasks.total())` counts completed items directly from the list. Because there is no second counter to maintain, the row checkmarks and header tally can never drift apart.
2. **Always call the generated getter**: Inside `${...}`, call `task.done()` rather than reading `task.done` directly so the reactive scope records the subscription.

## Run on your machine {#run}

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 2
```
