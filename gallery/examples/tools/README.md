# Snapshot tools

## README artwork

The README uses self-contained light and dark SVGs of the Iris demo captured at 128×160, 240×240, 390×844 and 1280×800: an eight-second loop recorded at 20 frames per second with a stepped clock, and a static presentation for reduced-motion preferences. Each capture runs one silent loop first so eased values start where they end. Captured glyph positions, vector contours, path clips and shared gradients do not require external images, fonts or scripts. Glyphs use distance-field contours when available and retain coverage samples otherwise; text that changes between frames swaps between pre-built runs. A draw that the renderer culls on some frames, such as the saved notice while it is parked above its stage, appears only on the frames that draw it.

`cargo run -p gallery --example iris_snapshot --no-default-features --features snapshot -- <output-directory> [milliseconds]` renders the same sizes through the software rasterizer for comparison.

Regenerate the artwork from the repository root:

```sh
cargo run -p gallery --example readme_capture --no-default-features --features snapshot -- .local/readme-hero/scenes.json
uv run --python 3.13 --with psd-tools==1.26.0 --with vtracer==0.6.15 python gallery/examples/tools/readme_assets.py --capture .local/readme-hero/scenes.json
```

Add `--logo-psd path/to/logo.psd` to export the light and dark wordmarks from the layered source. Background layers are omitted and foreground colors are preserved. Intermediate captures stay under `.local/readme-hero/`; the exported SVGs are written to `docs/assets/`.

## Demo captures

Chord Prompter is available as `chord_prompter` and through `cargo run -p gallery --example chord_prompter_demo --release --features sdl`. The Demo starts at 320×240 and supports live resizing down to 128×128 without recreating its widgets. The embedded presentation uses fixed 24px chords and lyrics, with 12px auxiliary labels. Both are integer multiples of the original 12px one-bit bitmap; native MIRX strikes avoid scaled bitmap interpolation. A fixed central needle marks musical time: chord changes, counted beats and strum accents activate at the center. The bar counter aligns with the right padding at the top; a separate 3×3 indicator at the top-left reflects the device BLE connection state. Playback controls stay outside the small device surface; desktop previews retain a toolbar. The beat and strum tracks occupy 10px and 18px, with 2px arrow strokes and 5px picked-string marks. Down/up arrows show sweep direction and string range; squares identify individual picked strings, with low E at the top and high E at the bottom.

The Chord Prompter Demo toolbar links to the fullscreen workspace at `/chord-prompter/`; the workspace has a return link to the Demo. Both surfaces share the Gallery palette and `mirui-theme` preference, with a theme toggle in the workspace. Editor drafts up to 8192 UTF-8 bytes survive same-tab round trips through browser session storage. Playback and Bluetooth connections are not resumed automatically. Start it with `cd gallery/web && trunk serve`. The center shows the current chord, lyrics, beats and strum position; a bottom glass controller provides playback, bar seeking and tempo, and the right editor opens from the panel button. Space toggles playback when focus is outside controls. Browser preview and the physical display share the Rust score parser and musical transport through the existing Gallery WASM bundle. A newly connected display with an unknown source shows an unsynced state; successful upload associates lyrics with the acknowledged device generation.

Chrome or Arc on desktop and Chrome on Android can connect to `MIRUI Chord` over HTTPS or localhost. Upload progress stays visible above playback controls, reports acknowledged bytes, and separates transfer from validation and activation. Duplicate uploads are blocked; status reads and playback commands share the ordered stream between chunks. Song uploads and controls use the [device protocol](chord-prompter-protocol.md). Run transport contract tests with `bun test gallery/web/tests/chord-prompter.test.js`.

Hover over the timeline to inspect all chord slots and lyrics in the pointed bar and the following bar without seeking. Focusing the timeline previews its selected bar; arrow keys select a destination, and Escape dismisses the preview. Unknown device sources stay explicitly unsynced, and the final bar has an end-of-song successor.

The workbench displays a horizontal strip of past, current and upcoming bars. Content moves at a uniform horizontal speed from right to left against a fixed musical center and enlarges as it approaches the center; chord and lyric fragments remain highlighted at their actual slot boundaries. The embedded view retains three complete neighboring bars with pixel-snapped uniform horizontal scrolling and fixed chord sizes. Lyrics wrap within each bar, preserve bracketed source accents and scroll vertically when needed; the incoming bar starts at its top. The current bar's vertical position follows its chord-slot interval. Both views derive motion from the same musical clock and freeze on pause. The existing scroll containers allow manual inspection while paused. Source limits bound storage; display density adds no lyric-length rejection. Reduced-motion preferences keep the browser strip stationary without removing surrounding context.

Capture an exact embedded presentation with `CHORD_VIEWPORT=128x128 CHORD_TIME_MS=2350 cargo run -p gallery --example demo_previews_snapshot --no-default-features --features snapshot -- .local/screenshots/chord-prompter chord_prompter`. The time is an absolute musical playback position in milliseconds; use 320x240 or larger dimensions to inspect the responsive layout. Set `CHORD_SOURCE` to a song-sheet file to inspect custom lyrics and vertical overflow.

Capture the desktop layout with `cargo run -p gallery --example demo_previews_snapshot --no-default-features --features snapshot -- .local/screenshots/chord-prompter chord_prompter`.
Nearby save panels can be captured with `NEARBY_SCENE=saves-0` through `saves-6`, `NEARBY_VIEWPORT=480x320` or `640x480`, `NEARBY_PALETTE=0`, `1` or `2`, and `NEARBY_SETTLE_FRAMES=0`, using `demo_previews_snapshot <output-directory> nearby`. These scenes replay browsing, scrolling, load confirmation, return, operations, game progress and read-only session information with a fixed clock and synthetic save records. `NEARBY_SCENE=actions` captures the seven game actions; `saves-actions` uses the save-reference game and focus. Ordinary Nearby simulation controls expose save read/write failures, record conflicts, empty inventory and room occupancy without accessing host save files.

`cargo run -p gallery --example surface_size_probe --no-default-features --features sdl` checks owner-thread content resizing, viewport feedback, rapid replacement, Nearby dock rendering and handle retirement in a temporary native window. Select `sdl-gpu` or `wgpu` to exercise those backends; this tool requires a windowed desktop session.

Compatible single-strike MIRX font assets can be combined with `cargo run -p gallery --example merge_font_strikes --no-default-features --features snapshot -- input12.mirx input24.mirx output.mirx`. The tool requires identical face metrics, advances and glyph identities, and preserves the native raster metadata and one-bit atlases. Chord Prompter asset inputs and checksums are recorded in `src/gallery/demos/assets/prompter-font-source.json`.

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

The preview tool accepts an optional demo slug after the output directory; omit it to capture the catalog. It writes PNG directly without Pillow. Put `--site` before the output directory to write the homepage set instead: lossless WebP files under `dark/` and `light/`, rendered in each host appearance.

Set `NEARBY_SCENE` when selecting `nearby` to capture a deterministic scene (640×480 by default): `NEARBY_SCENE=actions cargo run -p gallery --example demo_previews_snapshot --no-default-features --features snapshot -- .local/screenshots/nearby-actions nearby`. Available scenes are `favorites`, `actions`, `platforms`, `games`, `connection-games`, `empty-games`, `search`, `search-results`, `search-empty`, `settings`, `controls`, `tools`, `power`, `exit`, `network-menu`, `lobby`, `discovery-actions`, `discovery-current`, `discovery-empty`, `candidates`, `panel`, `panel-game`, `panel-host`, `mode`, `game-entry`, `choice`, `host`, `joined`, `joined-game`, `approval`, `ready`, `connection-failed`, `restore-failed`, `rooms`, `leaving`, `network` and `unavailable`. Set `NEARBY_VIEWPORT=320x240` or another nonzero logical size to inspect responsive geometry, and `NEARBY_PALETTE=0`, `1` or `2` for forest, sunset or night. `NEARBY_SETTLE_FRAMES=0` captures the first paint; the default advances 40 frames before capture.

`NEARBY_SCENE=simulation` captures the network simulation inspector beside the retained launcher. The output adds a 300-pixel left panel to `NEARBY_VIEWPORT`, preserving the application dimensions; the default output is 940×480.

`NEARBY_SCENE=favorites-full` supplies a bounded 24-entry inventory for adaptive grid and paging captures. `controls-muted`, `controls-loading`, `controls-pending` and `controls-unavailable` capture the typed slider states. These fixtures simulate device values and do not access hardware controls.

`details`, `details-core`, `details-firmware`, `details-saves` and `details-expanded` capture game metadata and field expansion. `details-loading` and `details-failed` exercise status presentation; `details-game` captures the game-action menu with matching reference inventory. Metadata, paths and file states in these scenes are synthetic fixtures.

`configuration-0` through `configuration-6` capture settings home, the core list, a later selection, field sources, source scrolling and both return levels. These scenes advance the existing clock in 16 ms steps through the original input sequence. `NEARBY_CONFIG_SETTLE_MS=800` adds an 800 ms settling interval after each input group. Game settings support A/Enter for source/confirmation, B/Escape for return, left/right for values, X for saving and Y for game/core/frontend scope. The F2 dock selects simulated save success, external revision conflict, write failure or read failure; outcomes apply to the corresponding load/save request. Settings and path previews stay in Demo memory and do not read, write or move system files.

`configuration-slot-default`, `configuration-slot-entry` and `configuration-slot-auto` capture save-source facts with configured slot 3 and respectively no explicit entry, explicit entry 7, or frontend auto-indexing. Add `-draft` to each scene to preview configured slot 5 without changing the explicit entry or startup result. The facts distinguish configured, explicit and startup slots, the save directory and the state path. Automatic indexing remains a frontend-owned result rather than a simulated numeric slot.

`wifi-list`, `wifi-password`, `wifi-hidden`, `wifi-security`, `wifi-connecting`, `wifi-cancelling`, `wifi-restoring`, `wifi-success`, `wifi-info`, `wifi-details`, `wifi-forget`, `wifi-off`, `wifi-empty`, `wifi-busy` and `wifi-failed` capture ordinary Wi-Fi settings with frozen synthetic network data. The live Demo's F2 simulation dock selects success, wrong-password, connection-failure, restoration-failure, radio-off, empty and room-busy cases. Wi-Fi controls use A/Enter to select, B/Escape to return or cancel, X to refresh/delete, Y for information/reveal and K for START/submit. These controls do not read or modify host networks, saved profiles or credentials.

Wi-Fi character, delete and submit actions use the software input method and a bounded document; queued actions retain their order and submission reads the committed text. Search and Wi-Fi use the same key surfaces. Characters follow the selected key and the loading span's leading edge; static Wi-Fi fixtures sample the span at 800 ms.

The Web pre-build hook invokes the same tool with `--site` for all entries in `gallery::DEMOS` and caches the previews under `target/gallery-site/demo-previews`. A failed generation preserves the previous preview directory; a successful generation replaces it. Adding a demo does not require maintaining a separate image count.

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

Glass Light Toggle uses a fixed interaction replay with separate initial-render, first-16-frame and warmed measurements. Run `cargo run --release -p gallery --example perf_collect --no-default-features --features sdl,sdl-gpu,perf -- --glass-sdl-gpu 90`; select `--glass-sdl` for software drawing in an SDL window or `--glass-sw` for a headless software target. Replace `--release` with `--profile release-opt` to use optimization level 3 while retaining release LTO, code generation, stripping and panic settings. This profile trades code size for execution speed; ordinary `--release` retains size optimization. Omit the profile flag to measure unoptimized Rust. `initial_render_us` includes the initial drawing and presentation; `warmup_mean_us` and `warmup_max_us` include first-use resources reached during early interaction. The remaining mean, percentiles and spans cover only warmed frames. Vsync is disabled, animation uses a fixed clock, and these are wall times without a forced GPU completion wait. Set `GLASS_SNAPSHOTS` to an output directory for visual captures; capture runs should be kept separate from performance comparisons.

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
