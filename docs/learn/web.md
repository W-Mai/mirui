---
pageClass: tutorial-page
aside: false
---

# Run and verify it in the browser

None of the code we wrote in `model.rs`, `interface.rs`, or `motion.rs` needs to change for the browser—only a 20-line host entry point swaps `SdlSurface` for `WebCanvasSurface`.

## Compare the Web and desktop entry points {#app}

In the workspace below, the focused view shows `web.rs`. Click the **`desktop.rs`** tab in the top bar to compare the two hosts side by side: both prepare their surface and storage backend, then call the exact same `install(&mut app, chapter, reduced, storage)`.

You can also click the **eye button (`Focus` → `Full`)** at any time to browse every line of all six source files in the crate:

<TutorialWorkspace :chapter="7" file="web.rs">

<template #exercise>

Drag the circular grip at the bottom-right corner of the preview to narrow the app to a phone screen, add a task, and click the dimension readout to restore the default size.

</template>

</TutorialWorkspace>

### What changed in this step {#source}

1. **One `install`, multiple surfaces**: Models, stylesheets, layout, and animations live in `install`. The Web host binds a `<canvas>` with `WebCanvasSurface` and drives `app.tick()` from `requestAnimationFrame`.
2. **Headless integration tests & Wasm bundle**: Run the two commands below. `cargo test` mounts all seven chapters and every concept demo on `App::headless`, exercising taps and persistence round-trips; `playground` builds the WebAssembly bundle running on this page.

## Run on your machine {#run}

```sh
cargo test -p mirui-first-app
bun run --cwd docs playground
```
