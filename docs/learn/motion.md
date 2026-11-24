# Give the interface motion

Animate completion progress and make the selection outline follow the active filter.

## Work through the code {#source}

1. Derive a normalized `Fixed` target from the task count. The progress view stores its displayed value and the most recent target. `Transition::retarget` starts from the current motion value after a quick reversal.
2. The framework’s `#[animate]` callback advances the progress appearance. The filter outline attaches `FollowBounds` to an ordinary `View`; the selected button supplies the target and `INTERACTIVE` supplies the spring.

<<< @/examples/first-app/src/motion.rs#retarget{rust}

## Run and check {#run}

Run this in the repository checkout for the current documentation version.

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 5
```

Toggle a task twice quickly, then select Open and Done. The progress bar reverses smoothly and the outline follows the selected button.

<TutorialDemo :chapter="5" />

The Model changes immediately. Only the displayed progress and the outline move through intermediate values.
