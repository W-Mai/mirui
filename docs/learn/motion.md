---
pageClass: tutorial-page
aside: false
---

# Give the interface motion

Responsive motion must handle interruption gracefully: if a user double-taps a task while the progress bar is half-way across, the bar should reverse smoothly from its current position rather than snapping back to zero.

## 1. Minimal demo: `FollowBounds` spring selection ring {#concept}

First, let's isolate the filter bar's selection ring. Below, three buttons have different widths, and a trailing `View` attaches `FollowBounds::new(id("spring_first")).with_transition(...)`. When a button is tapped, `pick_tab` sets `follower.target = target` and the critically damped spring interpolates both position and width:

<MiruiSnippet recipe="mini-motion" label="CONCEPT" file="recipes.rs" height="220px">

<template #exercise>

Tap rapidly across `MCU`, `Desktop`, and `Web`—watch the blue outline reverse mid-flight while preserving velocity and resizing to the new target.

</template>

</MiruiSnippet>

---

## 2. In the task list: retargetable progress tween + filter spring {#app}

Now we mount that `FollowBounds` ring inside `filters` (`interface.rs`) and add a custom `Progress` view driven by `aim_progress` and `#[animate]` (`motion.rs`) in the header:

<TutorialWorkspace :chapter="5" file="motion.rs">

<template #exercise>

Double-tap a row while the progress bar is still moving—watch it reverse smoothly from mid-flight. Then switch between `Open` and `Done`.

</template>

</TutorialWorkspace>

### What changed in this step {#source}

1. **Retargeting from the displayed value**: In `motion.rs`, whenever `goal` changes, `aim_progress` starts a new tween from the **currently displayed** `progress.0` (`settings.0.motion(progress.0, goal)`).
2. **Zero cost at rest**: `#[animate(component = AnimateProgress)]` updates `Progress` each frame and removes `AnimateProgress` from the entity as soon as the tween settles.

## Run on your machine {#run}

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 5
```
