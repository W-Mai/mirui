---
pageClass: tutorial-page
aside: false
---

# Complete a task

Let each row remember whether it is done, and let the header tally flip on the exact same frame.

<TutorialWorkspace :chapter="2" file="model.rs">

<<< @/examples/first-app/src/model.rs#task{rust}

<template #exercise>

Tap `Connect the display`: its checkmark lights up and the header count steps from `1 of 3` to `2 of 3`. Tap again to undo.

</template>

</TutorialWorkspace>

## Inside this frame {#source}

1. Call `app.add_model(Tasks::default()).finish()` before composing. Each row receives the handle to its own `Task`. Because `done` is marked `#[observe]`, any `${...}` expression that calls `task.done()` subscribes to that field automatically.
2. The row's `on Tap` handler simply calls `task.toggle()`. Both the checkmark visibility and `tasks.completed()` in the header derive from the same model data—there is no second counter to keep in sync by hand.

## Run on your machine {#run}

From the repository root for this manual version:

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 2
```

Always read the generated `task.done()` getter inside `${...}` bindings so the reactive scope records the dependency.
