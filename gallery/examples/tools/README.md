# Snapshot tools

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

## Interactive diagnostics

```sh
cargo run -p gallery --example sim_input_demo --features sdl
cargo run -p gallery --example lifecycle_suspend --features sdl
cargo run -p gallery --example zorder_baseline --features sdl
cargo run -p gallery --example widget_demo --features sdl
cargo run -p gallery --example perf_collect --features sdl,perf
```

The input demo repeatedly taps and drags its composed targets. In the lifecycle demo, Enter toggles suspension and Escape exits. The z-order diagnostic displays overlapping squares and reports their child order. The performance collector drives list scrolling and writes `/tmp/mirui-perf.ndjson`; it is a host-side diagnostic, not a hardware performance measurement.

`widget_demo` constructs and renders through a headless App, then presents the framebuffer in an SDL texture. `perf_bench` builds image and solid-color tile scenes through the same construction path; run it with `cargo run -p gallery --example perf_bench --no-default-features --features snapshot,perf --release` to collect host-side rendering and resource lookup timings.
