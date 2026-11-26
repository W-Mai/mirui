---
pageClass: tutorial-page
aside: false
---

# Add, remove, and filter

Type a new task, switch filter tabs, or delete a row in the middle—every remaining row keeps its identity and state.

<TutorialWorkspace :chapter="3" file="model.rs">

<<< @/examples/first-app/src/model.rs#collection{rust}

<template #exercise>

Type a title in the bottom field (or tap `+ Add`), flip between `Open` and `Done`, then remove the row you added.

</template>

</TutorialWorkspace>

## Inside this frame {#source}

1. Tasks live in a fixed-capacity `ModelList`, avoiding unbounded heap growth. Each entry receives a stable `ModelItemId`. The `walk ... by row.0` loop keys rows by that ID, and `Remove` hands the exact same ID back to `tasks.remove(key)`.
2. The in-progress text lives in a `TextDocument`. Tapping `+ Add` trims and consumes the draft—or falls back to `Task N` when empty. Filter tabs only toggle row visibility, while the header tally continues to report the full list.

## Run on your machine {#run}

From the repository root for this manual version:

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 3
```

Because deletion addresses a `ModelItemId` rather than a positional index, hiding or reordering rows never deletes the wrong item.
