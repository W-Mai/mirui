# mirui.rs — Web Canvas site

The root route presents the mirui.rs homepage with one live `<canvas>`, featured Play applications, generated preview cards, source inspection, and links to the complete demo registry. `?demo=<slug>` opens the selected demo workspace directly. Both routes share the same `web-canvas` backend, `requestAnimationFrame` driver, and application instance. Demo routes are generated from the `register_demos!` table in `src/lib.rs`.

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

This opens <http://127.0.0.1:8080/> and starts Marble Play in the live stage. Use `?demo=orbit_console` or another registered slug to open a demo directly.

The watcher includes the gallery, mirui, mirx, and macro sources outside this directory, so backend and demo edits rebuild the running WebAssembly bundle.

## Site structure

The homepage keeps one application live while neighboring featured cards use generated previews. Selecting a card reuses the live canvas instead of creating another renderer or application instance.

The Play section contains thirteen fixed 480×320 applications. Each application has a standalone native launcher, and the same setup function drives its Web Canvas route and generated card image.

The demo workspace groups the remaining entries by their registry category and retains the canvas size controls, source panel, theme selection, and query-string routes.

The source panel reads the selected scenario module directly. Each Lab keeps its primary `ui!` tree inside the focused source region and moves named sections into `#[compose]` functions.

Fixed demos keep their logical coordinate space while the browser changes CSS size or device pixel ratio. The canvas maps pointer coordinates from CSS pixels into that logical space, and a viewport change invalidates retained layout geometry before the next hit test. Audio-enabled demos expose a mute button beside the canvas orientation control; the first visit starts muted, while an explicit choice is remembered by the browser for later visits. A remembered unmuted preference is shown as pending until the first trusted page interaction unlocks the browser audio context, as required by autoplay policies.

## Build for release

```bash
cd gallery/web
trunk build --release
```

Output lands in `gallery/web/dist/`.

The Trunk pre-build hook renders the Play card previews through the software backend. Generated PNG files live under `target/gallery-site/play-previews` and are copied into `dist`; preview bitmaps are not stored in the source tree. Unchanged Rust sources reuse the generated set on subsequent builds.

## A note on artifact size

`trunk serve` and bare `trunk build` use the **dev** profile with `opt-level = 0`, debug symbols retained, and no `strip` or LTO. Development artifacts prioritize rebuild speed and are not deployment-size references.

`trunk build --release` uses the workspace `[profile.release]` with `opt-level = "z"`, LTO, one codegen unit, stripped symbols, and aborting panics. Measure the emitted `.wasm` after every Gallery feature batch because embedded fonts, artwork, and complete application modules intentionally remain in the single browser artifact.

`wasm-opt` runs only in `--release` and needs `--all-features` to accept the bulk-memory and sign-extension operations emitted by current wasm32 Rust toolchains; `index.html` supplies that flag through `data-wasm-opt-params`.

## Adding a demo

Add one line to the `register_demos!` invocation in `src/lib.rs`:

```rust
("my_slug", "my label", "Category", my_module),
```

The demo module declares its supported canvas contract with `pub const DEMO_SIZE: DemoSize`. `DemoSize::fixed(480, 320)` preserves a 480×320 logical viewport and scales its displayed canvas uniformly, `DemoSize::range(320, 240, 1024, 720)` supplies responsive bounds, and `DemoSize::constraints` accepts one-sided optional bounds. At least one bound must be present.

The navigation entry and query-string route derive from the registry row, while canvas sizing derives from the module-owned contract. The demo body must already exist at `mirui::gallery::demos::my_module` with a `setup_app(app, parent)`.
