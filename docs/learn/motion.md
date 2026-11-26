---
pageClass: tutorial-page
aside: false
---

# Give the interface motion

Tween the completion meter and let a critically damped spring glide the selection frame across the filter bar.

<TutorialWorkspace :chapter="5" file="motion.rs">

<<< @/examples/first-app/src/motion.rs#progress-motion{rust}

<template #exercise>

Double-tap a row while the progress bar is still moving—watch it reverse smoothly from mid-flight. Then switch between `Open` and `Done`.

</template>

</TutorialWorkspace>

## Inside this frame {#source}

1. `aim_progress` computes the normalized `Fixed` ratio from the completed and total counts. Whenever that target changes, it launches a new tween starting from the **currently displayed** `progress.0`—so rapid clicks retarget in place without snapping.
2. The `#[animate]` callback writes each interpolated value into `Progress` and removes the animation component once it settles. On the filter bar, a `View` with `FollowBounds` chases the active button's geometry using the `INTERACTIVE` spring.

## Run on your machine {#run}

From the repository root for this manual version:

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 5
```

The underlying `Model` flips on the first frame; only the visual meter width and the selection frame travel through intermediate values.
