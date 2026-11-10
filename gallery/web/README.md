# mirui.rs — Web Canvas site

The root route presents the mirui.rs homepage with one live `<canvas>` running Iris in a device workbench, the Play applications, generated preview cards for the demo registry, a pipeline walkthrough, targets and setup. `?demo=<slug>` opens the selected demo workspace directly. Both routes share the same `web-canvas` backend, `requestAnimationFrame` driver, and application instance. Demo routes are generated from the `register_demos!` table in `src/lib.rs`.

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

The homepage keeps one application live. The workbench frames the Iris canvas as an ESP32-C3 board with a 128×160 ST7735 panel, a 240×240 round watch, a 390×844 phone or a 1280×800 browser window, and morphs between them by animating the logical viewport, so the layout reflows through the intermediate sizes. Dragging the screen corner switches to a resizable window; arrow keys on the grip resize in 8 px steps. Next to the stage, the `IrisLayout::classify` and `match ${ model.layout() }` excerpts are taken from the embedded Iris source at runtime: a glass rod marks the branch and arm that run, the gutter marks evaluated conditions, and clicking an arm selects the matching screen. The pixel loupe magnifies canvas pixels under the pointer and reads the logical position in Q24.8 steps. First visits in a session tour the four screens once; any input stops the tour. The hero mark's iris follows the pointer. With reduced motion, the tour, morphs and eye movement are off.

Game and demo cards use generated previews. Opening one reuses the live canvas instead of creating another renderer or application instance. Opening a demo from the homepage preserves the scroll position in browser history, and returning restores the workbench screen.

Page text uses the bundled Geist and Geist Mono Latin subsets in `fonts/`; Chinese falls back to the platform CJK face.

The Play section contains thirteen fixed 480×320 applications. Each application has a standalone native launcher, and the same setup function drives its Web Canvas route and generated card image.

The demo workspace groups the remaining entries by their registry category and retains the canvas size controls, source panel, theme selection, and query-string routes.

Resizable demos without upper size bounds open at 640×480, constrained by the available space and their minimum dimensions. Dragging the resize handle can enlarge either axis beyond that initial size; reset restores the default size.

`?demo=nearby` opens the Nearby local launcher and connection demo. Favorites, the expanding catalog, search keyboard, game actions and nested settings share the same retained UI. Click Simulate above the canvas or press F2 to open a scrollable simulation inspector to its left with six flow entries, nine case presets and state-gated remote replies and faults. The stage adds 300 logical pixels while preserving the Demo size and scale; a 640×480 Demo uses a 940×480 canvas while open. The size label, rotate, reset and resize controls continue to describe the Demo, and closing removes the added panel width. Narrow browser windows allow horizontal scrolling. Deliver reply progresses the simulated peer or network; Local A/B preserve application confirmation and cancellation. Repeat case resets the attempt, and Stop case clears the connection and restores the launcher. No external process or network connection is started. Discovery retains both room and action panes, and matching-game selection continues the join flow. The canvas resizes independently in width and height, with a minimum logical size of 360×480. Native 12px and 24px Chinese pixel fonts, palettes and artwork are bundled with the demo; font sources and licenses are recorded alongside the assets.

Run the same Nearby UI in a resizable SDL window with `cargo run -p gallery --example nearby_demo` from the repository root. It opens at 640×480. Page transitions distinguish entering from returning; rapid returns reverse the active motion.

Use `cargo run -p gallery --example nearby_demo --no-default-features --features wgpu,sdl` for WGPU video with SDL audio output. The runner selects WGPU video when both features are enabled.

Nearby uses bundled sound cues for navigation, confirmation, return, refresh, room discovery, join requests, readiness, starting, errors and restored connections. Its sound and volume settings control actual audio output. In the browser, enable sound beside the canvas to unlock playback; the in-canvas sound setting stays synchronized with that control. Muted or locked interactions are skipped without later replay. Native audio uses the optional SDL audio feature independently of the selected video backend; builds without audio remain silent.

The source panel embeds each demo's root module and listed child modules. Each Lab keeps its primary `ui!` tree inside the focused source region and moves named sections into `#[compose]` functions.

Fixed demos keep their logical coordinate space while the browser changes CSS size or device pixel ratio. The canvas maps pointer coordinates from CSS pixels into that logical space, and a viewport change invalidates retained layout geometry before the next hit test. Audio-enabled demos expose a mute button beside the canvas orientation control; the first visit starts muted, while an explicit choice is remembered by the browser for later visits. A remembered unmuted preference is shown as pending until the first trusted page interaction unlocks the browser audio context, as required by autoplay policies.

## Build for release

```bash
cd gallery/web
trunk build --release
```

Output lands in `gallery/web/dist/`.

The Trunk pre-build hook renders the demo card previews through the software backend. Generated PNG files live under `target/gallery-site/demo-previews` and are copied into `dist`; preview bitmaps are not stored in the source tree. Unchanged Rust sources reuse the generated set on subsequent builds.

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
