---
pageClass: tutorial-page
aside: false
---

# Save and restore the list

Mark persistent fields on the model so added tasks and checkmarks survive when the window closes.

<TutorialWorkspace :chapter="6" file="lib.rs">

<<< @/examples/first-app/src/lib.rs#persistence{rust}

<template #exercise>

Add or toggle a task, wait half a second for autosave, then switch chapters and come back—your changes remain intact.

</template>

</TutorialWorkspace>

## Inside this frame {#source}

1. `Task` and `Tasks` annotate durable fields with `#[persist]`. Installing `PersistencePlugin::new(storage).autosave(500)` **before** `app.add_model(..).finish()` restores the snapshot during model registration, before the first frame is composed.
2. The desktop runner writes to `task-list.mirui`; the browser runner writes to a versioned `localStorage` key. Transient view state—the active filter tab and any in-flight animation—stays out of storage, and the progress bar initializes directly to the restored count.

## Run on your machine {#run}

From the repository root for this manual version:

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 6
```

The desktop example resolves `task-list.mirui` relative to the current working directory, so relaunch from the same directory to verify recovery.
