# gallery-web — `web-canvas` backend gallery

All registered `mirui::gallery::demos` run on a `<canvas>` through the `web-canvas` backend, driven by `requestAnimationFrame`. The sidebar nav is generated from the `register_demos!` table in `src/lib.rs`.

## Prerequisites

```bash
rustup target add wasm32-unknown-unknown
cargo install --locked trunk
```

`trunk` bundles the wasm build, runs `wasm-bindgen` and `wasm-opt`,
serves the page, and live-reloads on file changes — no manual
`wasm-bindgen` / `python -m http.server` steps.

## Develop

```bash
cd gallery/web
trunk serve
```

Opens a dev server at <http://127.0.0.1:8080/?demo=orbit_console>, rebuilds and reloads the browser on edits. Orbit Console is the default; pick another demo with `?demo=<slug>`.

The watcher includes the gallery, mirui, mirx, and macro sources outside this
directory, so backend and demo edits rebuild the running WebAssembly bundle.

## Gallery structure

Layout Lab, Typography Lab and Interaction Lab group related framework capabilities into responsive, inspectable scenarios. Focused entries remain for subsystems with distinct runtime behavior, including scrolling, animation, effects, text input and specialized controls.

The Product group contains Signal Scope, an interactive instrument surface with allocation-stable custom drawing, responsive controls and lossless MIRX artwork.

The Play group contains ten complete 480×320 applications with bounded models, fixed-step simulation, backend-neutral drawing, touch controls, and standalone native launchers. Tidal Atlas and Echo Walker also exercise versioned command-log persistence through the Gallery storage adapter.

The source panel reads the selected scenario module directly. Each Lab keeps its primary `ui!` tree inside the focused source region and moves named sections into `#[compose]` functions.

## Build for release

```bash
cd gallery/web
trunk build --release
```

Output lands in `gallery/web/dist/`.

## A note on artifact size

`trunk serve` and bare `trunk build` use the **dev** profile with `opt-level = 0`, debug symbols retained, and no `strip` or LTO. Development artifacts prioritize rebuild speed and are not deployment-size references.

`trunk build --release` uses the workspace `[profile.release]` with `opt-level = "z"`, LTO, one codegen unit, stripped symbols, and aborting panics. Measure the emitted `.wasm` after every Gallery feature batch because embedded fonts, artwork, and complete application modules intentionally remain in the single browser artifact.

`wasm-opt` runs only in `--release` and needs `--all-features` to accept the bulk-memory and sign-extension operations emitted by current wasm32 Rust toolchains; `index.html` supplies that flag through `data-wasm-opt-params`.

## Adding a demo

Add one line to the `register_demos!` invocation in `src/lib.rs`:

```rust
("my_slug", "my label", "Category", my_module, 480, 320),
```

Append `false` after the registered height when a demo must remain at its native size instead of filling spare Gallery space.

The sidebar entry, query-string route, and canvas sizing all derive
from that row. The demo body must already exist at
`mirui::gallery::demos::my_module` with a `setup_app(app, parent)`.
