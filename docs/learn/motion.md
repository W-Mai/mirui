# Give the interface motion

Animate completion progress and make the selection outline follow the active filter.

## Work through the code {#source}

1. Derive a normalized `Fixed` target from the task count. When it changes, start a tween from the displayed progress. A second click during the tween starts from the same displayed position.
2. The `#[animate]` callback applies the current value. The animation component leaves the entity when it finishes. The filter outline attaches `FollowBounds` to a `View` and moves to the selected button using `INTERACTIVE`.

<<< @/examples/first-app/src/motion.rs#progress-motion{rust}

## Run and check {#run}

Run this in the repository checkout for the current documentation version.

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 5
```

Toggle a task twice quickly, then select Open and Done. The progress bar reverses smoothly and the outline follows the selected button.

<TutorialDemo :chapter="5" />

The Model changes immediately. Only the displayed progress and the outline move through intermediate values.
