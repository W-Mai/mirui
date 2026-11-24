# Save and restore the list

Keep added tasks and completion states between app launches.

## Work through the code {#source}

1. The item and the owned list declare persisted fields. Install `PersistencePlugin` before `app.add_model(..).finish()` so registration can restore data before the first composition.
2. In the desktop runner the storage is `task-list.mirui`. The Web runner uses a versioned local storage prefix. The filter selection and in-progress animation are presentation states. The progress starts from the count already restored.

<<< @/examples/first-app/src/lib.rs#persistence{rust}

## Run and check {#run}

Run this in the repository checkout for the current documentation version.

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 6
```

Change at least one task, allow half a second for autosave, then close and reopen the app.

<TutorialDemo :chapter="6" />

Use the same working directory when restarting the desktop example, since the example storage path is relative.
