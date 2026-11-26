---
pageClass: tutorial-page
aside: false
---

# Open a window

Light up the first surface: a dark header block with two task rows underneath.

<TutorialWorkspace :chapter="1" file="lib.rs">

<<< @/examples/first-app/src/lib.rs#window{rust}

<template #exercise>

The canvas on the right opens a window with `Task list` in the header, followed by `Connect the display` and `Open a window`.

</template>

</TutorialWorkspace>

## Inside this frame {#source}

1. Check `examples/desktop.rs` in the tutorial crate. `SdlSurface` claims an OS window and feeds input events; `App` registers the built-in widgets and systems, spawns a viewport-sized root entity, and mounts the UI tree onto it.
2. The `#[compose]` function above is the entire first screen. An outer `Column` sets `20 px` padding and a `16 px` row gap; an inner block holds the heading, and two `Text` nodes sit below it. The window title bar belongs to the host OS, while the `Text` nodes are shaped and rasterized by `mirui`. Change the first task string and run the checkpoint again.

## Run on your machine {#run}

From the repository root for this manual version:

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 1
```

If the linker cannot find SDL2, install `libsdl2-dev` (or `sdl2` via Homebrew on macOS) and keep `--features desktop` enabled.
