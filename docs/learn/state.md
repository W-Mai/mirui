---
pageClass: tutorial-page
aside: false
---

# Complete a task

Make a task remember whether it is complete, and make the counter agree with it.

<TutorialWorkspace :chapter="2" file="model.rs">

<<< @/examples/first-app/src/model.rs#task{rust}

<template #exercise>

Click the first row. Its checkmark appears and the total moves from one completed task to two. Click again to undo it.

</template>

</TutorialWorkspace>

## Work through the code {#source}

1. Register `Tasks` with `app.add_model(Tasks::default()).finish()` before composing the UI. Each row receives the handle to a task, so both the row and the derived count can observe `done`.
2. The row binds its label to the task and calls `task.toggle()` from `on Tap`. Click the same row twice to see the change and its reversal. The completion count is derived from the list, so there is no second count to update.

## Run locally {#run}

Run this in the repository checkout for the current documentation version.

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 2
```

Use the generated `done()` getter inside the reactive expression so it subscribes to the field it reads.
