---
pageClass: tutorial-page
aside: false
---

# Reuse components and styles

Fold the repeated filter tabs into a `#[compose]` function and let completed rows dim through a shared stylesheet.

<TutorialWorkspace :chapter="4" file="interface.rs">

<<< @/examples/first-app/src/interface.rs#filter-button{rust}

<template #exercise>

Toggle a few rows and compare the muted ink of completed items against the crisp ink of open ones.

</template>

</TutorialWorkspace>

## Inside this frame {#source}

1. `#[compose]` turns the filter button into a reusable unit: `model!(Tasks)` handles the tap action, while `prop!(String)` accepts either a static string or a reactive `${...}` expression and binds it once at the `Button`.
2. `StyleSheetPlugin` registers `task`, `task.complete`, and `filter` rules in one place. Each row toggles `"task.complete"` from `task.done()`, updating its ink color while inheriting font metrics from the parent column.

<<< @/examples/first-app/src/lib.rs#task-styles{rust}

## Run on your machine {#run}

From the repository root for this manual version:

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 4
```

An attribute written directly on a widget overrides a stylesheet class. Keep state-driven colors in the stylesheet when you want classes to switch them.
