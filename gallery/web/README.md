# gallery-web — `web-canvas` backend gallery

All 46 `mirui::gallery::demos` running on a `<canvas>` through the
`web-canvas` backend, driven by `requestAnimationFrame`. The sidebar
nav is generated from the `register_demos!` table in `src/lib.rs`.

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

Opens a dev server at <http://127.0.0.1:8080/>, rebuilds and reloads
the browser on edits. Pick a demo with `?demo=<slug>`, e.g.
<http://127.0.0.1:8080/?demo=three_body>.

## Build for release

```bash
cd gallery/web
trunk build --release
```

Output lands in `gallery/web/dist/`.

## A note on artifact size

`trunk serve` and bare `trunk build` use the **dev** profile:
`opt-level = 0`, debug symbols retained, no `strip` / `lto`. The wasm
comes out around 3.5 MB — fast to rebuild (~2.4 s incremental), large
on disk. That size is dev-only and never shipped.

`trunk build --release` uses the workspace `[profile.release]`
(`opt-level = "z"`, `lto = true`, `codegen-units = 1`, `strip = true`,
`panic = "abort"`). The wasm drops to ~410 KB, ~166 KB gzipped — the
size a server actually serves. Incremental release rebuilds take ~6 s.

`wasm-opt` runs only in `--release` and needs `--all-features` to
accept the bulk-memory / sign-extension ops that rustc emits by
default for wasm32 since 1.82; that flag is wired in
`index.html`'s `data-wasm-opt-params`.

## Adding a demo

Add one line to the `register_demos!` invocation in `src/lib.rs`:

```rust
("my_slug", "my label", "Category", my_module, 480, 320),
```

The sidebar entry, query-string route, and canvas sizing all derive
from that row. The demo body must already exist at
`mirui::gallery::demos::my_module` with a `setup_app(app, parent)`.
