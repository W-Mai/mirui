---
pageClass: tutorial-page
aside: false
---

# Open a window

Start one desktop window with a heading and two rows. This is the first checkpoint.

<TutorialWorkspace :chapter="1" file="lib.rs">

<<< @/examples/first-app/src/lib.rs#window{rust}

<template #exercise>

A window opens with Task list, Connect the display, and Open a window.

</template>

</TutorialWorkspace>

## Work through the code {#source}

1. Open `examples/desktop.rs` in the tutorial package. `SdlSurface` supplies the window and input. `App` installs the widgets and default systems, creates a root, then composes the first view into it.
2. The composition below is the entire first screen. Change the first task title and run the same checkpoint again. The window title identifies the host, while the `Text` nodes belong to the UI.

## Run locally {#run}

Run this in the repository checkout for the current documentation version.

```sh
cargo run -p mirui-first-app --features desktop --example desktop -- 1
```

Install the SDL2 development library and use the `desktop` feature if the linker cannot find SDL2.
