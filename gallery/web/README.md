# mirui.rs — Web Canvas site

The root route presents the mirui.rs homepage: a split hero with Iris in a device workbench and a Play application on a second `<canvas>`, generated preview cards for the demo registry, a card-stack walkthrough, targets and setup. `?demo=<slug>` opens the selected demo workspace directly. Both routes share the same `web-canvas` backend, `requestAnimationFrame` driver, and primary application instance. Demo routes are generated from the `register_demos!` table in `src/lib.rs`.

## Prerequisites

```bash
rustup target add wasm32-unknown-unknown
cargo install --locked trunk
```

`trunk` bundles the Wasm build, runs `wasm-bindgen` and `wasm-opt`, serves the page, and reloads it after source changes. No separate bindgen or static-file server step is required.

## Develop

```bash
cd gallery/web
trunk serve
```

This opens <http://127.0.0.1:8080/> and starts Iris in the homepage workbench. Use `?demo=orbit_console` or another registered slug to open a demo directly.

The watcher includes the gallery, mirui, mirx, and macro sources outside this directory, so backend and demo edits rebuild the running WebAssembly bundle.

## Site structure

The hero has two halves. Small screens holds the Iris workbench; Big play runs one of the Play applications on `#mirui-play`. Clicking a half's title, its expand button or the folded half widens it to about three quarters of the row while the other half folds to a vertical title; Esc restores the even split. Below 1024 px the halves stack, and the expand button reveals the layout source or the game shelf instead.

The workbench frames the Iris canvas as an MCU board with a 128×160 SPI LCD, a 240×240 round watch, a 390×844 phone or a 1280×800 browser window, and morphs between them by animating the logical viewport, so the layout reflows through the intermediate sizes. Dragging the screen corner switches to a resizable window; arrow keys on the grip resize in 8 px steps. When the left half is expanded, the `IrisLayout::classify` and `match ${ model.layout() }` excerpts appear next to the stage, taken from the embedded Iris source at runtime and sized so every line fits without scrolling: a glass rod marks the branch and arm that run, the gutter marks evaluated conditions, and clicking an arm selects the matching screen. The pixel loupe magnifies canvas pixels under the pointer and reads the logical position in Q24.8 steps. First visits in a session tour the four screens once; any input stops the tour. With reduced motion, the tour and morphs are off.

The game half is a second `App`. `play_start(canvas_id, slug)` creates it, `play_switch(slug)` replaces the running game, `play_set_logical_size(w, h)` fixes its logical viewport and `play_set_paused(true)` parks it while the half is folded or off screen, the page is hidden or a demo workspace is open. Arrow keys, dragging the backdrop, the neighbouring cartridges or the shelf select another game. Each cartridge is the screen at 72% scale, and a switch moves every one a slot on: the chosen cartridge lands exactly on the glass, the screen shrinks into the slot behind over a cartridge of its game, the far neighbour leaves and the next one slides in, so no cartridge in view changes game. The screen returns under the landed cartridge with the same preview, the cartridge dissolves, and the preview gives way to the canvas after the new game has drawn. Play full screen opens it in the demo workspace on the primary canvas. `set_theme` reaches both applications; a game with its own fixed palette keeps it.

Game and demo cards use generated previews. Opening one reuses the primary canvas instead of creating another renderer or application instance. Opening a demo from the homepage preserves the scroll position in browser history, and returning restores the workbench screen.

Page text uses the bundled Geist and Geist Mono Latin subsets in `fonts/`; Chinese falls back to the platform CJK face.

The Play section contains thirteen fixed 480×320 applications. Each application has a standalone native launcher, and the same setup function drives its Web Canvas route and generated card image.

The demo workspace groups the remaining entries by their registry category and retains the canvas size controls, source panel, theme selection, and query-string routes.

Resizable demos without upper size bounds open at 640×480, constrained by the available space and their minimum dimensions. Dragging the resize handle can enlarge either axis beyond that initial size; reset restores the default size.

`?demo=nearby` opens the Nearby local launcher and connection demo. Favorites, the expanding catalog, search keyboard, game actions and nested settings share the same retained UI. Click Simulate above the canvas or press F2 to open a scrollable simulation inspector to its left with six flow entries, nine case presets and state-gated remote replies and faults. The stage adds 300 logical pixels while preserving the Demo size and scale; a 640×480 Demo uses a 940×480 canvas while open. The size label, rotate, reset and resize controls continue to describe the Demo, and closing removes the added panel width. Narrow browser windows allow horizontal scrolling. Deliver reply progresses the simulated peer or network; Local A/B preserve application confirmation and cancellation. Repeat case resets the attempt, and Stop case clears the connection and restores the launcher. No external process or network connection is started. Discovery retains both room and action panes, and matching-game selection continues the join flow. The canvas resizes independently in width and height, with a minimum logical size of 360×480. Native 12px and 24px Chinese pixel fonts, palettes and artwork are bundled with the demo; font sources and licenses are recorded alongside the assets.

Run the same Nearby UI in a resizable SDL window with `cargo run -p gallery --example nearby_demo` from the repository root. It opens at 640×480. Page transitions distinguish entering from returning; rapid returns reverse the active motion.

Use `cargo run -p gallery --example nearby_demo --no-default-features --features wgpu,sdl` for WGPU video with SDL audio output. The runner selects WGPU video when both features are enabled.

Nearby uses bundled sound cues for navigation, confirmation, return, refresh, room discovery, join requests, readiness, starting, errors and restored connections. Its sound and volume settings control actual audio output. In the browser, enable sound beside the canvas to unlock playback; the in-canvas sound setting stays synchronized with that control. Muted or locked interactions are skipped without later replay. Native audio uses the optional SDL audio feature independently of the selected video backend; builds without audio remain silent.

The source panel embeds each demo's root module and listed child modules, and shows them as one tab per file with the file's own line numbers. When a demo marks its `ui!` tree between `//~focus-start` and `//~focus-end`, the eye button, on by default, shows only those regions, with their real line numbers and without the scaffolding around them. A row with the file and line introduces each region after a skip, and the first region when the tree spans several files; a dot marks the tabs of those files. Turning the eye off opens the file that fills most of the view, and turning it on returns to the tree; either way the line being read keeps its place on screen. A click on the eye also applies to the next demos, while a click on a file tab opens that file for the current demo only. In a file tab, the lines of the tree carry a mark in the gutter. A path bar shows the repository path and the line count, the GitHub button opens the same file (and the line range for a single-file tree), and Copy copies what is shown without line numbers; selecting rows copies plain source as well. Each Lab keeps its primary `ui!` tree inside the focused source region and moves named sections into `#[compose]` functions.

Fixed demos keep their logical coordinate space while the browser changes CSS size or device pixel ratio. The canvas maps pointer coordinates from CSS pixels into that logical space, and a viewport change invalidates retained layout geometry before the next hit test. Audio-enabled demos expose a mute button beside the canvas orientation control; the first visit starts muted, while an explicit choice is remembered by the browser for later visits. A remembered unmuted preference is shown as pending until the first trusted page interaction unlocks the browser audio context, as required by autoplay policies.

## Build for release

```bash
cd gallery/web
trunk build --release
```

Output lands in `gallery/web/dist/`.

The Trunk pre-build hook renders the demo card previews through the software backend, once in the dark and once in the light host appearance, as lossless WebP files under `target/gallery-site/demo-previews/{dark,light}/<slug>.webp`, which are copied into `dist`. Demos with their own palette, such as the Play games, get the same picture in both sets, as they do on the live canvas. The page loads the set that matches its theme and swaps it when the theme changes. Preview bitmaps are not stored in the source tree. Unchanged Rust sources reuse the generated set on subsequent builds.

## A note on artifact size

`trunk serve` and bare `trunk build` use the **dev** profile with `opt-level = 0`, debug symbols retained, and no `strip` or LTO. Development artifacts prioritize rebuild speed and are not deployment-size references.

`trunk build --release` uses the workspace `[profile.release]` with `opt-level = "z"`, LTO, one codegen unit, stripped symbols, and aborting panics. Measure the emitted `.wasm` after every Gallery feature batch because embedded fonts, artwork, and complete application modules intentionally remain in the single browser artifact.

`wasm-opt` runs only in `--release`. `index.html` enables bulk-memory, sign-extension, and nontrapping float-to-integer operations through `data-wasm-opt-params`; experimental WebAssembly features remain disabled.

## Adding a demo

Add one line to the `register_demos!` invocation in `src/lib.rs`:

```rust
("my_slug", "my label", "Category", my_module),
```

The demo module declares its supported canvas contract with `pub const DEMO_SIZE: DemoSize`. `DemoSize::fixed(480, 320)` preserves a 480×320 logical viewport and scales its displayed canvas uniformly, `DemoSize::range(320, 240, 1024, 720)` supplies responsive bounds, and `DemoSize::constraints` accepts one-sided optional bounds. At least one bound must be present.

The navigation entry and query-string route derive from the registry row, while canvas sizing derives from the module-owned contract. The demo body must already exist at `mirui::gallery::demos::my_module` with a `setup_app(app, parent)`.
