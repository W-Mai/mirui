---
pageClass: tutorial-page
aside: false
---

# Add, remove, and filter

Dynamic lists have two classic pitfalls: unbounded heap growth on embedded targets, and index-based keys that delete or scramble the wrong row when items are filtered.

## 1. Minimal demo: `ModelList`, `TextInput`, and `walk` {#concept}

`ModelList::bounded(N)` provides a fixed-capacity reactive collection and assigns each item a stable `ModelItemId`.

In this minimal demo, `TextInput` binds a `TextDocument` draft, tapping `+ Add` (or pressing Return) pushes a new item, and `walk ${ notes.items().entries() } with entry by entry.0` keys each row by its `ModelItemId`:

<MiruiSnippet recipe="mini-list" label="CONCEPT" file="recipes.rs" height="230px">

<template #exercise>

Type a short note and press Return (or tap `+ Add`), then remove any row in the middle with `×`.

</template>

</MiruiSnippet>

---

## 2. In the task list: adding, removing, and filtering tasks {#app}

Now we wire `TextInput`, `Remove` buttons, and the `All / Open / Done` filter bar into the task list.

Inspect the focused changes across `model.rs` and `interface.rs` (marked with **`+`**): filtering never removes items from `ModelList`—it simply binds each row's `visible` property to `tasks.filter().includes(task.done())`, while the header tally continues to report the full list:

<ArchFlow kind="uml-ch3-collections" />

<TutorialWorkspace :chapter="3" file="model.rs">

<template #exercise>

Type a title in the bottom field (or tap `+ Add`), flip between `Open` and `Done`, then remove a task—every remaining row keeps its identity and state.

</template>

</TutorialWorkspace>

### What changed in this step {#source}

1. **Addressing by `ModelItemId` instead of index**: When completed rows are hidden under `Open`, the first visible row on screen may be the second item in storage. Passing `ModelItemId` to `Edit::Remove(key)` ensures deletion always hits the exact item you tapped.
2. **Decoupled text draft**: Cursor movement and text selection live inside `TextDocument`. `Tasks` is only mutated when `edit_tasks` fires on Return or `+ Add`.

## Run on your machine {#run}

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 3
```
