---
pageClass: tutorial-page
aside: false
---

# Open a window

Start with a self-contained 30-line demo to see how `mirui` arranges boxes and text inside a window, then bring that structure into our task list app.

## 1. Minimal demo: `Column`, `Row`, and `Text` {#concept}

In `mirui`, a screen is a `ui!` tree returned by a `#[compose]` function. `Column` stacks children vertically, `Row` places them side by side, and `grow: 1.0` shares the remaining space evenly.

The source on the left matches the canvas on the right **1 : 1** (click the **eye button** in the top bar to expand the full `recipes.rs` file):

<MiruiSnippet recipe="mini-window" label="CONCEPT" file="recipes.rs" height="210px">

<template #exercise>

A dark banner and two equal-width cards sit inside the outer column. Drag the circular grip at the bottom-right corner to resize the viewport.

</template>

</MiruiSnippet>

---

## 2. In the task list: lighting up the first screen {#app}

With basic box layout in hand, let's open the task list crate. In chapter 1 we mount a dark `Task list` header block with two static rows underneath.

Lines marked with **`+` and a soft blue wash** are added in this step. Click the **`desktop.rs`** file tab or toggle the **eye button** to see how `SdlSurface` creates the native window and calls `install`:

<TutorialWorkspace :chapter="1" file="lib.rs">

<template #exercise>

The canvas opens the first screen of our task list. Click the `desktop.rs` tab at the top of the source pane to inspect the 24-line native launcher.

</template>

</TutorialWorkspace>

### What changed in this step {#source}

1. **Host vs. UI tree**: In `desktop.rs`, `SdlSurface::new("Task list", 420, 440)` claims an OS window; in `lib.rs`, `app.spawn_root().id()` creates a viewport-filling root entity and `app.compose(root, first_window())` mounts the widget tree onto it.
2. **Semantic color tokens**: Instead of hardcoded hex values, the header uses `ColorToken::Secondary` and `ColorToken::OnSecondary`. Toggle the site's light/dark theme button in the top bar—the canvas adapts automatically.

## Run on your machine {#run}

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 1
```
