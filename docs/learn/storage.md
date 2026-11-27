---
pageClass: tutorial-page
aside: false
---

# Save and restore the list

A task list shouldn't lose your work when the window closes. Persistence in `mirui` is declarative: mark durable fields with `#[persist]`, install `PersistencePlugin`, and the model restores its snapshot as soon as it is registered.

## 1. Minimal demo: `#[persist]` and simulated relaunch {#concept}

Here is a minimal persistent counter: `SavedCount` opts into `#[model(persistence(key = "..."))]` and marks `value` with `#[persist]`.

Tap `+ 1 (Autosaves)` a few times, wait 0.2 seconds, and click **`Relaunch ↻`** at the bottom-right of the workspace—it destroys the running `App` instance and mounts a brand-new one from storage:

<MiruiSnippet recipe="mini-storage" label="CONCEPT" file="recipes.rs" height="200px">

<template #exercise>

Increment the counter, pause briefly for autosave, then click **`Relaunch ↻`** in the workspace footer to verify the new `App` instance restores the saved count.

</template>

</MiruiSnippet>

---

## 2. In the task list: autosave and startup restoration {#app}

Now we annotate `Task` and `Tasks` in `model.rs` and install `PersistencePlugin` **before** `app.add_model(...)` in `lib.rs`:

<ArchFlow kind="uml-ch6-storage" />

<TutorialWorkspace :chapter="6" file="model.rs">

<template #exercise>

Add or toggle a task, wait half a second for autosave, then click **`Relaunch ↻`** in the footer (or refresh the browser) to see your list restored.

</template>

</TutorialWorkspace>

### What changed in this step {#source}

1. **Install the plugin before registering models**: `app.add_model(Tasks::default()).finish()` queries `PersistencePlugin` immediately during registration, so the very first composed frame already reflects the restored data.
2. **Persist domain data, not transient UI state**: Notice `filter` on `Tasks` is marked `#[observe]` but not `#[persist]`. Active filter tabs and in-flight animations reset cleanly on launch while tasks and completion states endure.

## Run on your machine {#run}

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 6
```
