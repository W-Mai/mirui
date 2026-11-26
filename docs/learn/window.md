---
pageClass: tutorial-page
aside: false
---

# Open a window

Start with a self-contained example to see how `mirui` arranges containers and text inside a window, then bring that structure into the task list.

## 1. Minimal demo: `Column`, `Row`, and `Text` {#concept}

In `mirui`, a screen is a `ui!` tree returned by a `#[compose]` function. `Column` stacks children vertically, `Row` places them side by side, and `grow: 1.0` shares the remaining space evenly:

<MiruiSnippet recipe="mini-window" label="CONCEPT" file="recipes.rs" height="210px">

<template #exercise>

Drag the grip at the bottom-right corner to resize the viewport and see the two `grow: 1.0` cards split the row width.

</template>

</MiruiSnippet>

---

## 2. In the task list: the first screen {#app}

In chapter 1 we mount a dark `Task list` header block with two static rows underneath. Click the **`desktop.rs`** tab or toggle the **eye button** to see how `SdlSurface` creates the native window and calls `install`:

<TutorialWorkspace :chapter="1" file="lib.rs">

<template #exercise>

Switch to the `desktop.rs` tab at the top of the source pane to inspect how the native launcher passes `SdlSurface` into `install`.

</template>

</TutorialWorkspace>

### What changed in this step {#source}

1. **Host vs. UI tree**: In `desktop.rs`, `SdlSurface::new("Task list", 420, 440)` claims an OS window; in `lib.rs`, `app.spawn_root().id()` creates a viewport-filling root entity and `app.compose(root, first_window())` mounts the widget tree onto it.
2. **Semantic color tokens**: Instead of hardcoded hex values, the header uses `ColorToken::Secondary` and `ColorToken::OnSecondary`. Toggle the site's light/dark theme button in the top bar—the canvas adapts automatically.

## Run on your machine {#run}

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 1
```
