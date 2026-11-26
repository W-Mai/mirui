---
pageClass: tutorial-page
aside: false
---

# Run and verify it in the browser

Hand the same `install` function to `WebCanvasSurface`, then resize the live viewport down to phone width.

<TutorialWorkspace :chapter="7" file="web.rs">

<<< @/examples/first-app/src/web.rs#web-host{rust}

<template #exercise>

Drag the circular grip at the bottom-right corner of the preview to narrow the app to a phone screen, add a task, and click the dimension readout to restore the default size.

</template>

</TutorialWorkspace>

## Inside this frame {#source}

1. Desktop and Web entry points call the exact same `install(&mut app, chapter, reduced, storage)`. The Web host binds a `<canvas>` via `WebCanvasSurface` and `WebCanvasRendererFactory`, drives `app.tick()` on `requestAnimationFrame`, and frees the instance when the preview unmounts.
2. Run the two commands below: the integration suite mounts every chapter headlessly, exercises model mutations, and verifies persistence round-trips; the `playground` build compiles the WebAssembly bundle running inside this page.

## Run on your machine {#run}

From the repository root for this manual version:

```sh
cargo test -p mirui-first-app
bun run --cwd docs playground
```

If you start the docs server in a fresh checkout, run `bun run --cwd docs playground` first so the live canvas has its compiled Wasm bundle.
