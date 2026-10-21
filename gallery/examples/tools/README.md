# Snapshot tools

The embedded font asset can be regenerated with `cargo xtask gen-mirx builtin-font`; `cargo xtask gen-mirx builtin-font --check` verifies it against the glyph source. The generator uses the MIRX font encoder and does not require external font tooling.

Run these commands from the repository root. Headless tools use the software framebuffer and do not open a window. PNG tools that convert PPM files require Pillow in the `python3` environment; the effect and modal tools write PPM directly.

```sh
mkdir -p .local/screenshots
cargo run -p gallery --example tabbar_snapshot --no-default-features --features snapshot -- 1 .local/screenshots/tabs.png
MIRUI_SNAPSHOT_DIR=.local/screenshots cargo run -p gallery --example lazy_list_snapshot --no-default-features --features snapshot -- mid
cargo run -p gallery --example effect_demo_snapshot --no-default-features --features snapshot -- .local/screenshots/effects.ppm
cargo run -p gallery --example offscreen_modal_snapshot --no-default-features --features snapshot -- .local/screenshots/modal
cargo run -p gallery --example demo_previews_snapshot --no-default-features --features snapshot -- .local/screenshots editor_lab
MIRUI_SNAPSHOT_DIR=.local/screenshots cargo run -p gallery --example text_input_snapshot --no-default-features --features snapshot -- selection
```

The tab snapshot accepts a selected index from 0 to 2. The list accepts `top`, `mid`, or `bottom`. The modal tool captures 30 frames for each of the inline (`inl-*.ppm`) and offscreen (`off-*.ppm`) rendering modes.

The preview tool accepts an optional demo slug after the output directory; omit it to capture the catalog. It writes PNG directly without Pillow.

The Web pre-build hook invokes the same tool for all entries in `gallery::DEMOS` and caches the previews under `target/gallery-site/demo-previews`. A failed generation preserves the previous preview directory; a successful generation replaces it. Adding a demo does not require maintaining a separate image count.

For a regression check, capture before and after a change with identical arguments into separate directories. Compare corresponding files and inspect the images for clipping, layering, and animation artifacts. These snapshots support manual visual checks; they do not replace unit tests for layout, event, or lifecycle contracts. Keep generated images under `.local/screenshots/` or a temporary directory, outside version control.

Scene construction belongs in `#[compose]` functions called through `App::compose`. Tools that inspect render internals keep those diagnostics outside the composition body.

Tools that prepare layout directly use `try_update_layout` and stop on failure before drawing or writing an image. Text and projective backend snapshots use `App::snapshot()`, including normal frame preparation, presentation, and painted-state tracking. Readback occurs before presentation; unsupported readback returns an error instead of using another renderer.

Capture the same text scene with the software, SDL-GPU, or native WGPU renderer:

```sh
cargo run -p gallery --release --example text_backend_sw_snapshot --no-default-features --features snapshot -- .local/screenshots/text-sw.png
cargo run -p gallery --release --example text_backend_sdl_snapshot --no-default-features --features sdl-gpu -- .local/screenshots/text-sdl.png
cargo run -p gallery --release --example text_backend_wgpu_snapshot --no-default-features --features wgpu -- .local/screenshots/text-wgpu.png
cargo run -p gallery --release --example text_backend_compare --no-default-features --features snapshot -- .local/screenshots/text-sw.png .local/screenshots/text-sdl.png
```

GPU tools open native windows and require a working display server. Compare images at the same physical dimensions; the software fixture uses 2× scale. These PNG tools accept packed RGBA8888 textures, while the App API preserves the renderer's actual pixel format and stride.

Projective snapshots advance the selected animation with a fixed 16 ms step per frame. The software capture accepts an output path, frame count, and demo name (`flip_card`, `image_flip`, `book_flip`, or `cover_flow`):

```sh
cargo run -p gallery --example projective_sw_snapshot --no-default-features --features snapshot -- .local/screenshots/projective.png 45 flip_card
```

## Interactive diagnostics

```sh
cargo run -p gallery --example sim_input_demo --features sdl
cargo run -p gallery --example lifecycle_suspend --features sdl
cargo run -p gallery --example zorder_baseline --features sdl
cargo run -p gallery --example widget_demo --features sdl
cargo run -p gallery --example perf_collect --features sdl,perf
```

The input demo repeatedly taps and drags its composed targets. In the lifecycle demo, Enter toggles suspension and Escape exits. The z-order diagnostic displays overlapping squares and reports their child order. The performance collector drives list scrolling and writes `/tmp/mirui-perf.ndjson`; it is a host-side diagnostic, not a hardware performance measurement.

### SDL-GPU rendering samples

The same collector can measure native SDL-GPU full-frame rendering without the simulated transfer delay:

```sh
cargo run --release -p gallery --example perf_collect --features sdl-gpu -- --sdl-gpu nearby 300 gl 0
target/release/examples/perf_collect --sdl-gpu page_studio 300 gl 0
target/release/examples/perf_collect --sdl-gpu shapes 300 gl 0
target/release/examples/perf_collect --sdl-gpu text 300 gl 0
target/release/examples/perf_collect --sdl-gpu digits 300 gl 0
target/release/examples/perf_collect --sdl-gpu coverage_text 300 gl 0
target/release/examples/perf_collect --sdl-gpu coverage_digits 300 gl 0
```

Arguments are scene, measured frame count (default 300), driver (`gl`, `gles2`, or `auto`; default `gl`), and requested MSAA samples (default 0). `auto` accepts only 0 because it does not request GL multisampling. Vsync is disabled. Each invocation opens one native window, reports the first rendered frame separately, warms 30 frames, then reports wall-time mean, P50, P95, P99 and maximum. Driver, logical/physical size, scale, configuration, build debug assertions and the SDL batching hint are printed with the results. Record the source commit and build flags alongside the output when comparing runs.

These samples call `App::render()` on every frame, including layout preparation and presentation, without advancing systems or dispatching user input. Scene clocks are fixed. `digits` changes its Model before each render; its timing includes that update and reactive text formatting. `text` holds the same content fixed. The other scenes use existing Demo compositions. This measures full-frame submission and presentation wall time, not idle scheduling, page animation, input responsiveness or isolated GPU execution time. No readback or explicit GPU completion wait is inserted; queued GPU work may complete after a call returns. Compare identical scene/configuration/physical-size runs on the same device, and inspect snapshots separately.

`text` and `digits` use Play distance-field glyphs. `coverage_text` and `coverage_digits` use the 14 px coverage representation in MiSans; the latter updates the counter while keeping the surrounding paragraphs unchanged. Compare each scene against itself when checking a representation-specific path.

Existing `InspectCaches` statistics are printed after warmup and after measurement, outside the timed region. Counters are cumulative; subtract the warm values to obtain measured-frame hits, misses and evictions. Cache limits are not measured GPU memory usage. Closing the window aborts collection rather than reporting an incomplete sample set. The tool does not modify global SDL settings or the library's default backend configuration.

`widget_demo` constructs and renders through a headless App, then presents the framebuffer in an SDL texture. `perf_bench` builds image and solid-color tile scenes through the same construction path; run it with `cargo run -p gallery --example perf_bench --no-default-features --features snapshot,perf --release` to collect host-side rendering and resource lookup timings.
