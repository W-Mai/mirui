---
pageClass: tutorial-page
aside: false
---

# Run and verify it in the browser

Use the same interface and Model from a Web Canvas host, then verify the tutorial checkpoints.

<TutorialWorkspace :chapter="7" file="web.rs">

<<< @/examples/first-app/src/web.rs#web-host{rust}

<template #exercise>

Resize the runner, add a task, and switch filters. The interface and animation are driven by the same Rust source as the desktop example.

</template>

</TutorialWorkspace>

## Work through the code {#source}

1. The desktop and Web entry points call the same `install` function. Web constructs `WebCanvasSurface` and supplies `WebCanvasRendererFactory`. The host drives `App::tick()` on animation frames and releases it when a lesson preview closes.
2. Run the two checks below. The integration tests mount the chapter views, exercise Model updates, and reopen a saved task list. The playground build generates the wasm package used by the tutorial.

## Run locally {#run}

Run this in the repository checkout for the current documentation version.

```sh
cargo test -p mirui-first-app
bun run --cwd docs playground
```

If a compiled preview is unavailable locally, build it from the repository checkout before running the documentation server.
