# Add, remove, and filter

Type a task title, add it, and filter the list without changing a row’s identity.

## Work through the code {#source}

1. Store tasks in the bounded `ModelList`. The list gives each member a `ModelItemId`. The UI keys its `walk` rows by that ID, and the Remove action sends the same ID back to the Model.
2. The draft lives in a `TextDocument`. Add consumes the draft, with a short generated name for an empty field. Filters bind row visibility to their task’s state, and the count continues to describe the entire list.

<<< @/examples/first-app/src/model.rs#collection{rust}

## Run and check {#run}

Run this in the repository checkout for the current documentation version.

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 3
```

Add a named task, switch Open and Done, then remove that task. Existing rows keep their associated state.

<TutorialDemo :chapter="3" />

Removing by member ID keeps the action correct even if rows have moved or some are currently hidden.
