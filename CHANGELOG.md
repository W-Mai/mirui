# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.25.3] - 2026-06-01

Experimental Linux fbdev backend — `feature = "linux-fb"` runs mirui directly against `/dev/fb0` and `/dev/input/event*` on Linux, bypassing X11 / Wayland for SBCs and embedded panels. Ships with full input (keyboard / mouse / wheel / SIGINT-as-Quit), auto DPI scale from physical-mm hints, and an opt-in HDMI overscan inset. Two long-standing per-frame allocation leaks in the perf-instrumentation path are also fixed.

### Added

- **`feature = "linux-fb"`** — opt-in Linux backend on `memmap2` 0.9, `evdev` 0.13, `libc` 0.2, and `signal-hook` 0.3. Fbdev resolution and pixel format come from `FBIOGET_VSCREENINFO`; the renderer writes through `&mut self.mmap[..]` directly and respects `line_length` so padded scanlines on driver-aligned panels render correctly.
  - **Input** — USB tablet / touchscreen (`EV_ABS` + multi-touch slot tracking), USB mouse (`EV_REL` accumulated into a clamped cursor position), scroll wheel (`REL_WHEEL` / `REL_HWHEEL` coalesced per `SYN_REPORT`), and a keyboard fd that maps eight editing keys (`KEY_BACKSPACE` / `KEY_LEFT` / `KEY_RETURN` / ...) to mirui's SDL-style codes; unmapped scancodes pass through raw.
  - **Quit signal** — `signal-hook` registers a flag for `SIGINT` / `SIGTERM`; `poll_event` turns the next tick into `InputEvent::Quit` so demos exit cleanly without `SIGKILL`.
  - **Auto DPI scale** — `LinuxConfig::scale` defaults to `ScaleMode::AutoDpi { baseline_dpi: 96 }`. Reads `var.width` / `var.height` (mm), computes panel DPI, divides by `baseline_dpi`, quantises to quarter-steps, and clamps to `[1.0, 4.0]`. Drivers reporting 0 mm (qemu ramfb, EFI fb, simple-framebuffer) fall back to `Fixed::ONE`. `ScaleMode::Fixed(Fixed)` overrides for known-broken drivers.
  - **HDMI overscan inset** — `LinuxConfig::overscan_inset_percent` (capped at 25%) lets `gallery::run` shrink the rendered view symmetrically when an HDMI panel eats the panel's outer rim. `framebuffer()` exposes a sub-slice with `tex.stride = line_length` so the renderer never touches the unsafe border.
- **`gallery::demos::hello`** — minimal LinuxFb-friendly scene (single card + text). Hoisted out of `linux_fb_demo` so other backends can run the same body.
- **`gallery::demos::widgets`** — port of the ESP32-C3 widgets showcase (`LazyList` + `Slider` + `Switch` + `TabBar` + theme cycling) scaled for desktop / fb-class panels.
- **`mirui::plugins::FrameRateCapPlugin`** — sleeps in `post_render` to a target FPS read from `MonoClock`. Native backends in `gallery::run` install it at default 120 Hz (override via `MIRUI_FPS_CAP=<n>`, `0` opts out for benchmarks). Needed because every native backend skips `present` / `flush` on idle frames, which is also where vsync would have waited — the tick loop otherwise runs at 60 000+ fps and tears against the host compositor.
- **`FpsSummary::wall_ns`** — wall-clock span over the reporting window (`MonoClock`-based) so sinks can compute the visible FPS (`frames * 1e9 / wall_ns`) separately from the per-frame work rate (`1e9 / avg_frame_ns`). Mixed idle / active frames inside a window made the latter swing wildly without context.

### Fixed

- **`App::snapshot_system_perf` no longer allocates per frame when no plugin reads it.** Each tick used to build a fresh `Vec<SystemStat>` + `Box<SystemPerfSnapshot>` regardless of consumer. With `PerfReportPlugin` absent the snapshot was thrown away every frame, but musl / glibc returned the freed memory to size-class arenas rather than the OS. Vsync-free backends (linux-fb, headless benches) hit OOM in 3-4 minutes. `PerfReportPlugin::build` now inserts the resource as an opt-in flag; `snapshot_system_perf` early-returns without it, and reuses the resource's `Vec` via `clear() + push()` when present.
- **`trace_span!` / `#[trace_fn]` no longer leak `PerfEvent`s when no plugin drains them.** `Guard::drop` pushed a `PerfEvent` into a thread-local `Vec` regardless of whether anyone called `drain_events`. Without `PerfReportPlugin` the `Vec` grew via doubling (40 → 80 → 160 → 320 MB allocations confirmed via backtrace sampling) until OOM. `perf::set_enabled(bool)` now gates both `enter()` and `Guard::drop`; `PerfReportPlugin::build` flips it on. The `no_std` ring-buffer path is unchanged.
- **Text widgets without an explicit size now contribute a fallback intrinsic measurement to layout.** A flex parent could otherwise hand `Text` `width = 0`, which the render walker culled via strict-less-than `rects_intersect`, and a covering parent `fill_rect` then erased the previous frame's glyphs — visible as text disappearing where the cursor passed. `build_layout_tree` and `build_rects` apply a local `LayoutNode` measurement (`Px(text.bytes.len × CHAR_W + 4, CHAR_H + 4)`) when both axes are `Auto` / `Content` and `flex_grow == 0`. ECS state is untouched.
- **Web-canvas backend** — perspective-warped quads now draw rounded corners through `Path::rounded_quad`'s cubic-bezier approximation. Canvas 2D has no homography, so the affine `setTransform + roundRect` fast path does not apply.

### Changed

- **`mirui::surface::linux::LinuxFbSurface` writes directly to mmap** — the previous staging `Vec<u8>` + per-band `memcpy` was the source of partial-flush coherence bugs. `framebuffer()` now hands the renderer `&mut self.mmap[..]` straight, with `tex.stride = line_length`. `flush()` becomes a no-op. Side-effect: the R↔B byte swap path is also gone, so BGRX fbdev drivers (qemu ramfb, most modern PC fbs) render with reversed colour until the planned `BGRA8888` `ColorFormat` lands. RGB565 drivers (Pi 4 HDMI) are unaffected.

### Internal

- `gallery` / `gallery-web` / `xtask` / `.cha/plugin-src` workspace members bumped from 0.25.1 to 0.25.2 (the 0.25.2 release commit had only touched `mirui` + `mirui-macros`), then to 0.25.3 alongside the rest of the workspace.

## [0.25.2] - 2026-05-31

Experimental web-canvas backend — `feature = "web-canvas"` runs mirui in `wasm32-unknown-unknown` against an HTML `<canvas>` 2D context. `App::tick` is driven from `requestAnimationFrame`, DOM pointer / wheel / touch / keyboard events are bridged into the input queue, and texture uploads are cached per-canvas through `OffscreenCanvas`.

### Added

- **`feature = "web-canvas"`** — opt-in browser backend on `web-sys` 0.3.99, `js-sys` 0.3.99, `wasm-bindgen` 0.2.122, `wasm-bindgen-futures` 0.4.72, and `web-time` 1.
  - `mirui::surface::web_canvas::WebCanvasSurface` wraps a DOM `<canvas>`, sizes the backing store to `logical × devicePixelRatio` (fractional DPR preserved), captures the pointer on `pointerdown` so drags survive crossing the canvas edge, and unregisters every DOM listener through `Drop`.
  - `mirui::draw::web_canvas::WebCanvasRendererFactory` paints fills / strokes / blits / text via Canvas 2D. Quad blit subdivides the source rect into an 8 × 8 affine triangle mesh — Canvas 2D has no homography, so subdivision approximates the projective warp.
  - `Runner::start_animation_frame()` (under `cfg(target_arch = "wasm32", feature = "web-canvas")`) drives the run-loop from `requestAnimationFrame` and returns to the wasm-bindgen entry so the browser keeps owning frames.
- **`gallery-web` crate** — `?demo=<name>` query parameter selects which `gallery::demos::*::build` runs (`dsl` is the default).
- **`gallery::demos::*`** — demo bodies hoisted out of `examples/` so the same `build(setup)` feeds both the native cargo examples and the wasm crate.
- **`xtask wasm-check`** — `cargo check --target wasm32-unknown-unknown --no-default-features --features web-canvas --lib`. Skips silently when the rustup target is missing.
- **`xtask wasm-build`** — release build of `gallery-web` plus a `wasm-bindgen --target web` post-process into `gallery/web/pkg/`.

### Changed

- `web_time::Instant` replaces `std::time::Instant` in `StdInstantClockPlugin` and the perf path. Native re-exports `std::time`; wasm32 reads `performance.now()`.

### Limitations

- Effect widgets that read or modify the framebuffer — `BackgroundBlur`, `MirrorOf`, `TemporalMix` — are unimplemented on Canvas 2D. `gallery-web` does not route the `effect` demo.
- `RGB565` / `RGB565Swapped` textures fall through to a no-op blit.
- A non-trivial 2D `WidgetTransform` outside a quad path is multiplied as `dpr × widget_tf` once per command. Quad branches paint under a DPR-only matrix because the four points already encode every parent transform.

## [0.25.1] - 2026-05-31

wgpu desktop backend (experimental) — new `feature = "wgpu"` runs mirui on Vulkan / Metal / DX12 / OpenGL ES through a winit window, with one render pass per frame, a uniform ring buffer for batched draws, and shared bind groups whose binding 1 carries a dynamic offset.

### Added

- **`feature = "wgpu"`** — opt-in cross-platform GPU backend on `wgpu` 29 and `winit` 0.30.
  - `mirui::surface::wgpu_surface::WgpuSurface` + `mirui::draw::wgpu_render::WgpuRendererFactory`.
  - 4× MSAA, perspective-correct quad blit (`Transform3D::from_quad` recovers the homography), rounded-rect SDF for both axis-aligned and 2.5D quad widgets, glyph atlas, lyon-tessellated paths.
  - `Mailbox > Immediate > Fifo` present-mode preference; `Fifo` is the universal fallback.
  - Per-draw `set_scissor_rect` derived from each command's logical clip, clamped to the swapchain extent.
- **`gallery::run`** picks the backend at compile time: `--features wgpu` > `--features sdl-gpu` > `--features sdl` (default).
- **`gallery/examples/wgpu_smoke.rs`** — minimal hello window for the wgpu path.

### Changed

- `mirui::cache::Cache` backs the wgpu render-pipeline cache. `Count(N)` is the right knob for "admit everything, never evict"; `MaxSize::Disabled` means "no insertion at all" and is no longer used for that purpose.
- `gallery::run` requires a `Send + 'static` builder closure so wgpu shares one demo entrypoint with sdl / sdl-gpu.

### Fixed

- `Canvas::clear(area, color)` honours `area`. The wgpu impl was overwriting the whole display, defeating dirty-region renderers that called `clear` with a sub-rect.

### Limitations on the wgpu backend

- macOS trackpad two-finger scroll feels less responsive than on the sdl backend.
- Pinch and rotation gestures are not delivered as input events.
- `ColorFormat::RGB565` / `RGB565Swapped` textures are skipped by `Canvas::blit`; convert to `RGB888` / `RGBA8888` first.

## [0.25.0] - 2026-05-30

Main loop refactor for non-blocking event loops (browsers, custom embedded schedulers).

### Added

- `App::tick(&mut self) -> bool` — runs one frame; returns `true` after `Quit` + `on_quit` hooks.
- `Runner<B, F>` + `App::into_runner()` — `Runner::run_blocking()` (native, `-> !`) is the existing `App::run` with `process::exit` on quit; on no_std it spins after quit. `Runner::start_animation_frame()` (wasm) is a stub that the WebAssembly canvas backend will wire to `requestAnimationFrame`.

### Changed

- `App::run` body now wraps `App::tick` — pure refactor, no semantic change.
- `cargo xtask bump <level>` now also rewrites the `mirui-macros` pin in the root Cargo.toml and the `mirui = "X.Y"` literals in README, `docs/quickstart.md`, and the crate-root rustdoc.

## [0.24.0] - 2026-05-29

Onboarding pass: a long-form Quickstart guide, the [`W-Mai/mirui-templates`](https://github.com/W-Mai/mirui-templates) cargo-generate template repo, and infrastructure to keep the two repos pinned to matching mirui versions on every release.

### Added

- **`docs/quickstart.md`**: 6-section walkthrough — toolchain prerequisites, the desktop SDL hello (Cargo.toml + main.rs verbatim, verified to build on a fresh project), the ESP32-C3 path (delegates to `mirui-examples` for BSP wiring rather than duplicate a recipe that bit-rots with esp-hal releases), the Cargo workspace layout that shares UI code across multiple targets, the cargo-generate shortcut, and a "where to go next" pointer set.
- **Crate-level rustdoc Quick Start** (`src/lib.rs`): the docs.rs landing page now shows the SDL hello inline and links at `docs/quickstart.md` for embedded, the workspace template, and the multi-target recipe. Maps the public modules so users can find `app` / `ecs` / `widget` / `draw` / `surface` / etc. without hunting through the sidebar.
- **`cargo xtask templates-bump`**: walks `../mirui-templates/templates/` for `cargo-generate.toml` files and rewrites the `[placeholders.mirui-version]` default to match mirui's current major.minor. Designed to commit the change locally and let the maintainer review and push, the same pattern `mirui-examples` uses for its `Cargo.lock` bumps. Auto-invoked from `cargo xtask release` after the crates.io publish step. Importantly, it leaves `{{mirui-version}}` placeholders inside template `Cargo.toml` files alone — those are substituted by cargo-generate at generation time.

### Changed

- **`cargo xtask bump <level>`**: now also rewrites the `mirui-macros` dependency pin in the root Cargo.toml and the `mirui = { version = "X.Y", ... }` literals in README, `docs/quickstart.md`, and the crate-root rustdoc. Previous releases needed those edits made by hand; the bump is now a single command. Path-only deps (e.g. `mirui-macros = { path = "../mirui-macros" }` in `gallery/Cargo.toml`) without a `version = "..."` field are left alone.
- **README Quick Start**: bumped pinned version to `0.24`, declared the `sdl` feature explicitly (recent versions stopped linking SDL2 by default), reshaped the inline `ui!` example to use a column container around `header`/`content`/`footer` (the xrune `:( :)` block accepts a single root widget below it, so siblings need a parent), and switched to the multi-line `:( parent: ... world: ... :)` form (the comma-separated single-line variant the previous README carried did not actually parse).
- **Quickstart mirui pin literals**: README, `docs/quickstart.md`, and the crate-level rustdoc all now pin `mirui = "0.24"` matching this release.
- **Crate-root doctest** (`src/lib.rs`): switched from `ignore` to `no_run` so `cargo test --doc` actually compiles the snippet on default features. Uses `FramebufSurface` with a no-op flush callback to exercise the prelude, `App::new`, `WidgetBuilder`, `ui!` macro syntax, `set_root`, and `run` end-to-end at type-check time.

### Removed

- **`mirui-macros = "0.23"` from Quick Start examples**: the mirui crate already re-exports the proc-macros it ships (`ui!`, `system`, `animate!`, `timer!`, `trace_span!`, `trace_fn`, `compose_backend!`). Declaring `mirui-macros` directly was redundant and gave a reader two version pins to keep in sync. Verified by building a fresh project with only `mirui = { version = "0.24", features = ["sdl"] }` declared.

## [0.23.2] - 2026-05-29

### Fixed

- **`fill_axis_aligned` Blend fallback now uses the caller's folded `opa`** (`src/draw/sw/rect_fill.rs`): regression in v0.23.0 wrote alpha ≈ `color.a` instead of `color.a × opa / 255` on alpha-aware buffers. Effect widgets (`MirrorOf` / `TemporalMix` / `BackgroundBlur`) sampling the buffer saw a thicker silhouette than intended. Default Opaque path unaffected.
- **`SimAction::rotate_smooth` ceil-divides `total_ms` by `|ticks|`** (`src/event/sim.rs`): floor division could retire early when `total_ms` was not divisible by `|ticks|`. The per-detent step now rounds up so the effective duration is `≥ total_ms`, with at most `|ticks| − 1` ms of overshoot. Doc updated to mark the value as approximate.
- **Offscreen buffers promote to RGBA8888 when alpha matters** (`src/widget/render_system.rs`): on RGB565 backends, an entity with `OffscreenAlphaMode::clear_transparent()` previously produced an opaque-black halo around the silhouette because the buffer had no alpha channel. The offscreen format is now overridden to RGBA8888 in this case; framebuffer rendering and offscreen entities without `clear_transparent` keep their native format.

## [0.23.1] - 2026-05-29

`SimAction::rotate_smooth` distributes encoder detents by an ease curve instead of fixed intervals, letting tests reproduce realistic accelerate / decelerate patterns rather than the always-equal tempo of `SimAction::rotate`.

### Added

- **`SimAction::rotate_smooth(ticks, total_ms, ease_fn)`** (`src/event/sim.rs`): distributes `ticks` detents across `total_ms` according to `ease_fn(elapsed / total_ms) ∈ [0, 1]`. `ease::linear` reproduces the legacy fixed-tempo path; `ease_in_out_cubic` and similar back-loaded curves emit detents slowly at start, accelerate through the middle, and decelerate near the end. The `Rotary` events themselves are unchanged — only their wall-clock distribution shifts.

### Changed

- **`RotateAction.ease_fn: Option<fn(Fixed) -> Fixed>`** (`src/event/sim.rs`): `None` keeps `SimAction::rotate(...)` running on the existing fixed-tempo path; `Some` is populated by `SimAction::rotate_smooth(...)`.

### Breaking

- **`RotateAction` gains a public `ease_fn` field** (`src/event/sim.rs`): callers constructing the struct via literal must add `ease_fn: None` for the legacy fixed-tempo behaviour, or switch to `SimAction::rotate(...)` / `SimAction::rotate_smooth(...)`. Pattern-match destructuring should add `..` to remain forward-compatible.

## [0.23.0] - 2026-05-29

Alpha-aware software rasterisation. Offscreen buffers can now signal that their alpha channel matters downstream, and `SwRenderer` writes accumulate `dst.a` via non-premultiplied source-over instead of clobbering it to 255. Unblocks effect widgets (DropShadow and similar) that read the buffer's alpha as a silhouette mask.

### Added

- **`AlphaMode` enum + `Texture::alpha_mode` field** (`src/draw/texture.rs`): `Opaque` (default — keeps the framebuffer path's "always write 255" behaviour) and `Blend` (accumulates `dst.a` via non-premultiplied source-over). `AlphaMode` is also re-exported from `crate::draw::sw` so existing `use mirui::draw::sw::*` callers see it alongside `SwRenderer`.
- **`SwRenderer::with_alpha_mode(mode)` builder** (`src/draw/sw/mod.rs`): sets the underlying target's mode. Default `SwRenderer::new(target)` is unchanged — `Opaque`.
- **Four unit tests covering Opaque vs Blend semantics** (`src/draw/sw/mod.rs`): default-mode regression, transparent-buffer first-hit alpha pass-through, two-fill source-over composition (±2 u8 rounding tolerance), and the `src.a = 255` source-over identity short-circuit.

### Changed

- **`Texture::blend_pixel_int` reads `self.alpha_mode`** (`src/draw/texture.rs`): the existing 4-arg signature is preserved (no caller migration needed). In `Blend` mode the destination alpha is composed via `out.a = src.a + dst.a × (255 − src.a) / 255` instead of writing 255. The `a == 255` short-circuit still writes 255 in both modes — that's the source-over identity at full source alpha.
- **`fill_axis_aligned` falls back from the memcpy fast path when `Blend` mode + partial alpha** (`src/draw/sw/rect_fill.rs`): `fill_first_row_then_replicate` would otherwise clobber the destination's alpha byte. The Opaque path and the fully-opaque source case in Blend mode keep the memcpy fast path.
- **`blit_1to1_argb_to_argb` accumulates `dst.a` in Blend mode** (`src/draw/sw/blit_fast.rs`): partial-alpha source pixels compose into the destination's alpha channel rather than overwriting it.
- **`render_system::try_draw_offscreen` selects `AlphaMode` from `OffscreenAlphaMode`** (`src/widget/render_system.rs`): an entity carrying `OffscreenAlphaMode::clear_transparent()` makes the inner `SwRenderer` use `Blend`. Other offscreen entities keep `Opaque` so the framebuffer's alpha gets pre-seeded as before.

## [0.22.1] - 2026-05-29

Bug-fix release for scroll-blit idle short-circuits, `LastDirtyRegions` idle-frame semantics, and regression coverage for negative sub-pixel and nested scroll.

### Added

- **Regression tests for negative sub-pixel and nested scroll** (`src/widget/render_system.rs`): three-frame -0.4 accumulation pinning residue against an explicitly-computed Q24.8 reference, plus a HiDPI viewport variant covering that the walker quantises in logical-pixel space regardless of physical scale. Nested scroll gains a pixel-equivalence test that applies inner + outer shifts in DFS post-order and checks every output row matches a fresh full-repaint of the composed source mapping.

### Changed

- **`App::render_dirty` writes `LastDirtyRegions::default()` on idle frames** (`src/app.rs`): idle frames previously skipped the resource write, leaving the resource at whatever the last non-idle frame published. Consumers that read it as "shifts produced last frame" — notably the cursor feedback short-circuit gate added in this release — would otherwise see a stale shift signal forever, defeating the gate's optimisation once any scroll happened. The resource now consistently means "plan from the most recently completed frame".

### Fixed

- **`cursor_feedback_system` short-circuit recomputes when layout shifts under a static pointer** (`src/feedback/cursor.rs`): the pointer-unchanged short-circuit returned early on cached `(x, y, down)` even when entities under the cursor had moved between frames. A magnetic-rect overlay therefore froze on the previous target's rect when scroll-blit shifted children below. New `world_has_layout_motion` gate bypasses the short-circuit when the previous frame published any `RegionShift` (via `LastDirtyRegions`) or any entity carries pending `ScrollDelta`. Both signals are O(1) on hash-indexed storages.
- **`lazy_list_system` clears stale slot bindings on shrink and over-sized pools** (`src/components/lazy_list.rs`): the idle short-circuit (visible_start unchanged, all targets bound) skipped cleanup, so two cases left slots painting wrong content — `pool_size > item_count` at startup (slots beyond `item_count` never bound and never hidden) and `item_count` shrinking under live bindings (slots whose binding now points past the live tail). New `clear_extra_slots` sweep runs before the idle short-circuit; `apply_bindings` removes `Hidden` from slots it reuses when the data set grows back.

## [0.22.0] - 2026-05-29

Software-render dispatch overhead reduction. Four independent paths cut per-entity and per-system work in the dirty + render flow: hash-indexed ECS lookups, an opt-in component filter on `View`, a scheduler skip hint on `System`, and a one-frame layout cache shared between the dirty and render walkers.

### Added

- **`World::has_any_by_id(type_id) -> bool`** (`src/ecs/world.rs`): true iff any live entity owns a component of the given `TypeId`. Pairs with the existing per-entity `has_type` for a whole-storage emptiness probe in O(1).
- **`View::with_filter::<T>()` builder** (`src/widget/view.rs`): restricts `render` dispatch to entities owning component `T`. The walker checks `world.has_type(entity, TypeId::of::<T>())` before invoking the view's render fn, hoisting the early-return guard most built-in views already had into the walker. All thirteen rendering built-in views opt in (`Style`, `Button`, `Checkbox`, `Image`, `Text`, `Slider`, `Switch`, `TabBar`, `ProgressBar`, `TextInput`, `MirrorOf`, `TemporalMix`, `BackgroundBlur`); user views without the filter behave as before.
- **`System::expect: &'static [fn() -> TypeId]` field + `with_expect(...)` builder** (`src/ecs/system.rs`): a non-empty slice gates the system on `world.has_any_by_id(tid)` for any of the listed types. Empty slice (the default) preserves unconditional run. The slice element is `fn() -> TypeId` rather than `TypeId` so callers can build it in const context on stable Rust.
- **`#[mirui::system(expect = T)]` and `#[mirui::system(expect = [T1, T2])]` macro arguments** (`mirui-macros/src/lib.rs`): forward the listed types to `System::with_expect`. Multi-entry slices use OR semantics — the system runs if any listed type has a live entity. Type paths resolve at the call site, supporting bare names, `crate::...`, `::other_crate::...`, and nested module paths.
- **Seven built-in systems gain `expect` tags** (`src/components/switch.rs`, `src/components/text_input.rs`, `src/components/lazy_list.rs`, `src/components/tab_pages.rs`, `src/widget/offscreen.rs`): `switch_init_system` (`Switch`), `animate_switch_bg_t_system` (`AnimateSwitchBgT`), `animate_thumb_x_system` (`AnimateThumbX`), `cursor_blink_system` (`TextInput`), `lazy_list_system` (`LazyList`), `tab_pages_system` (`TabBar`), `maintain_widget_texture_refs` (`WidgetTextureRef` or `OffscreenAutoAdded`). Apps that don't instantiate these widgets avoid running those system bodies entirely.

### Changed

- **`World::storages` switches from `Vec<(TypeId, Box<dyn ...>)>` to `HashMap<TypeId, Box<dyn ...>>`** (`src/ecs/world.rs`): every `world.get<T>` / `has_type` / `storage<T>` lookup was a linear scan over a Vec; with dozens of registered component types and per-entity render dispatch hitting them thousands of times per frame, the scan dominated the hot path. `despawn` iterates `values_mut` instead of indexed positions. Public API signatures are unchanged; see Breaking for drop-order consequences.
- **`World::resources` switches from `Vec` to `HashMap`** (`src/ecs/world.rs`): same lookup shape as storages, applied to resource access. Public API signatures unchanged; see Breaking for drop-order consequences.
- **Dirty walker publishes a `LayoutSnapshot` resource consumed by the render walker on the same frame** (`src/widget/render_system.rs`, `src/app.rs`): `collect_dirty_regions` writes its solved layout tree + entity preorder; `App::render_dirty` hands the snapshot to a new internal `render_region_cached` entry point that skips the build / compute / collect phases. The public `pub fn render_region` keeps building from scratch — the cached path is `pub(crate)` and only invoked when the same-frame dirty walker has just produced the snapshot, so external callers cannot consume a stale cache.

### Breaking

- **`System` gains a public `expect: &'static [fn() -> TypeId]` field** (`src/ecs/system.rs`): callers constructing `System` via struct literal must add `expect: &[]` for unconditional run, or switch to the `System::new(name, priority, run)` builder (and chain `.with_expect(...)` if needed). The builder path is unchanged.
- **`World` no longer drops resources or component storages in insertion order** (`src/ecs/world.rs`): both backing containers switched from `Vec` to `HashMap`, so cross-type drop order during `World::despawn` (per-entity component drops) and during `World` teardown (resource + storage drops) follows hash order instead. Code whose `Drop` implementations rely on cross-type ordering must be updated to be order-independent. Single-type drops, and drops that touch only the world (not other types' state), are unaffected.

### Fixed

- **`View::install` forwards `System::expect` to the scheduler** (`src/widget/view.rs`): the previous version rebuilt each registered system with `System::new(name, priority, run)`, dropping the `expect` slice that `#[mirui::system(expect = ...)]` had attached. Built-in widget systems registered through views (switch / text-input / lazy-list / tab-pages / texture-refs) carried tags that the scheduler then never saw. Now `view.install` chains `.with_expect(s.expect)` so the tag round-trips. A regression test in `view::tests` installs a tagged dummy view and asserts the slice survives.

## [0.21.3] - 2026-05-27

Perfetto trace timeline names the previously unattributed input / systems / finalize / post_render phases, and `PerfReportPlugin`'s sink callback switches to one batched buffer per frame.

### Changed

- **`frame.input` / `frame.systems` / `frame.finalize` / `frame.post_render` trace spans** (`src/app.rs`): the existing `frame.collect_dirty` / `frame.render_region` / `frame.flush` / `frame.layout` / `frame.render` / `frame.seed_prev` spans covered the render path, but the input poll + gesture dispatch, the systems run, `finalize_frame_stats`, and the plugin `post_render` loop appeared as gaps on the timeline. The work was real (event poll, scroll system, animations, lazy-list pool maintenance, plugin sinks); it just wasn't named.
- **`PerfReportPlugin` perfetto sink receives one batched buffer per frame** (`src/plugins/perf_report.rs`): the sink callback was previously invoked once per chrome-trace event. The JSON event payload is unchanged; batching removes the per-call overhead, and host-side collectors receive one frame batch instead of one callback per event.

### Breaking

- **Perfetto sink callback contract changed** (`src/plugins/perf_report.rs`): the `&str` passed to a sink registered via `with_perfetto_line_sink` now contains the frame's chrome-trace events joined by `\n`, with a trailing `\n` after the last event. Bundled sinks (`with_perfetto_writer` and the example sinks) already handle the new form. User code that registered a custom sink expecting exactly one event per call should write the buffer through unchanged when the downstream tool accepts joined events, or use `lines()` (which skips the trailing empty segment) when it needs them split.

## [0.21.2] - 2026-05-27

Scroll-blit framework — scroll containers move their existing pixels in place inside the framebuffer instead of redrawing the whole subtree every frame. Driven through a new `DirtyRegions` plan that `App::render_dirty` consumes each frame.

### Added

- **`mirui::event::scroll::ScrollDelta` component** (`src/event/scroll/components.rs`): per-frame `ScrollOffset` increment. Input / inertia systems write it; the dirty walker reads it to plan a framebuffer self-blit and subtracts the integer pixels it consumed, leaving any sub-pixel residue for the next frame.
- **`Renderer::supports_scroll_blit() -> bool`** + **`Renderer::scroll_target_region(area, dx, dy)`** (`src/draw/renderer.rs`): backend capability flag and in-place framebuffer shift. `SwRenderer` implements both with a row-wise / column-wise `copy_within` walk; backends that can't read the framebuffer inherit the default `false` + `unimplemented!`. `App::render_dirty` checks `supports_scroll_blit` before executing the plan's shifts and folds them into redraw rects when the renderer doesn't opt in.
- **`Surface::begin_flush()` / `end_flush()` hooks** (`src/surface/mod.rs`): default no-op, called before / after the frame's `flush(area)` calls. Backends with a swap chain can amortise vsync / texture creation across the per-rect flushes.
- **`mirui::widget::dirty::{DirtyRegions, RegionShift}`** (`src/widget/dirty.rs`): plan returned by the new walker — `rects` to redraw + `shifts` to memmove in place. `DirtyRegions::flatten_shifts` folds shifts into rects for non-scroll-blit backends; `bounding_rect` returns the union over both lists.
- **`mirui::widget::render_system::collect_dirty_regions(world, root, viewport) -> DirtyRegions`** (`src/widget/render_system.rs`): plan-style dirty walker; `collect_dirty_region` (singular) is now a thin wrapper that returns `DirtyRegions::bounding_rect`.
- **`mirui::widget::set_position_quiet`** (`src/widget/mod.rs`): like `set_position` but skips the `Dirty` mark. Use when the entity's pixels are about to be moved by something else (e.g. the enclosing scroll container's self-blit) so a redundant redraw is undesirable.
- **`Fixed::trunc_to_int()`** (`src/types/fixed.rs`): truncate-toward-zero counterpart to `to_int` (which is arithmetic-shift floor). Required for residue-keeping quantisation where the integer part subtracted off must leave a residue with the same sign as the original.
- **`mirui::widget::render_system::LastDirtyRegions` resource** (`src/widget/render_system.rs`): the plan from the last `render_dirty` frame, for probes / debug overlays. Production code should not depend on it.

### Changed

- **`render_dirty` consumes the new `DirtyRegions` plan** (`src/app.rs`): runs region shifts first, unions all redraw rects into a single bbox, walks the layout tree once at the union, then flushes each dirty rect inside one `begin_flush` / `end_flush` envelope. Replaces the previous "render each rect separately, flush each rect separately" path; saves repeated tree walks on frames with multiple dirty rects.
- **`SwRenderer::scroll_target_region` quantises sub-pixel deltas toward zero** (`src/draw/sw/mod.rs`): a `-0.5` shift truncates to no movement (caller keeps the residue), not to `-1` (which would clobber a row). Matches the walker-side quantisation so residue accumulates across frames consistently.
- **`SdlSurface` flushes each dirty rect into a cached streaming texture, then copies + presents once per frame** (`src/surface/sdl.rs`): replaces the per-flush `create_texture_streaming` + full-surface upload. The cached texture's lifetime is `'static`; one program-lifetime allocation per `SdlSurface`.
- **`SdlSurface` field declaration order ensures the streaming texture is dropped before the canvas** (`src/surface/sdl.rs`): SDL textures must be destroyed before the renderer that created them, and Rust drops struct fields in declaration order.
- **`LazyList` uses a ring-buffer slot mapping `slot[target % pool_size]`** (`src/components/lazy_list.rs`): a one-row scroll only rebinds one slot; the rest keep their content and only their layout position moves through `set_position_quiet`. The pre-ring linear mapping rebound every slot on every `visible_start` change, marking all rows Dirty and bloating the dirty bbox to the entire list area.
- **Dirty walker emits `RegionShift` for every scroll container with a whole-pixel `ScrollDelta` after truncation toward zero** (`src/widget/render_system.rs`): DFS post-order so a nested inner shift runs in the child's local frame, then the outer shift carries the moved pixels along. The strip exposed by each shift is added to `plan.rects` for repaint. Sub-pixel-only deltas keep their residue in the component without emitting a shift.
- **Walker quantises `ScrollDelta` toward zero, clips dirty rects to the innermost scroll container's screen area, and skips Dirty push for scroll containers that emitted a `RegionShift` or still carry only sub-pixel residue this frame** (`src/widget/render_system.rs`): a LazyList row whose `node.rect.y` sits below `visible_start` would otherwise blow bounds out vertically; the container's own Dirty marker is already expressed by the `RegionShift` (or has no on-screen effect when only the residue moved), so unioning its rect would re-stretch the bbox over the area self-blit handles.
- **Scroll input / inertia systems only set Dirty on non-zero applied delta** (`src/event/scroll/system.rs`): clamp-at-edge frames produce a PointerMove that boils down to no movement, and there's nothing to repaint.
- **`RegionShift` is kept (no demote) when an overlay sits on the scroll area; the overlay's old + shifted rects are added to `plan.rects` instead** (`src/widget/render_system.rs`): a stationary cursor / rotary overlay stays put logically but its framebuffer pixels would otherwise ride the self-blit. Repainting the overlay's old position (covers the row content under it) and the shifted position (covers the residue) preserves the overlay's place while the rest of the area still benefits from self-blit.

### Fixed

- **Negative sub-pixel `ScrollDelta` quantises toward zero, not floor** (`src/widget/render_system.rs`, `src/draw/sw/mod.rs`): `Fixed::to_int` is arithmetic-shift floor (`-0.5 → -1`); using it for residue-keeping quantisation would emit a `RegionShift.dy = +1` and leave a `+0.5` residue with the wrong sign, so the next frame's `-0.5` input would cancel it instead of accumulating. Walker and `SwRenderer::scroll_target_region` now both use `Fixed::trunc_to_int`. Regression tests cover walker accumulation across two `-0.5` frames and renderer no-op on a single `-0.5` shift.

## [0.21.1] - 2026-05-27

Idle-frame short-circuits across high-frequency systems, plus a `Hidden`-related correctness fix.

### Added

- **`mirui::widget::dirty::clear_subtree_dirty(world, root)`** (`src/widget/dirty.rs`): walks a subtree and removes every `Dirty` marker. Mirror of `mark_subtree_dirty`. Called by the hide path in `tab_pages_system` so leftover markers don't strand once the walker stops descending through the subtree.

### Changed

- **`collect_dirty_region` bails at the top when the `Dirty` storage is empty** (`src/widget/render_system.rs`): the full 5-step layout pipeline (build_tree → compute_layout → collect_entities → write_computed → walk) used to run every frame, even on idle frames where no entity had a Dirty marker.
- **`mark_subtree_dirty` now skips `Hidden` subtrees** (`src/widget/dirty.rs`): the walker only descends visible `LayoutNode`s, so a Dirty marker placed under a Hidden subtree (e.g. by `set_theme` walking the root tree) could never be swept and would defeat the storage-empty fast path.
- **`tab_pages_system` rewrites both visibility transitions** (`src/components/tab_pages.rs`):
  - Hide: clears the subtree's leftover Dirty markers and bumps Dirty on the parent so its `ComputedRect` (which covers the area being hidden) repaints. Marking the now-Hidden entity itself wouldn't reach the walker.
  - Unhide: marks the whole subtree Dirty, not just the entity. While it was Hidden any global event walking from the root via `mark_subtree_dirty` skipped it, leaving descendants — including any cached offscreen buffers (mirror, blur, ...) — with stale data. Subtree-wide marking on unhide bumps every `OffscreenGeneration` inside so the cache misses on the next render.
- **`cursor_feedback_system` fast-path on stationary pointer** (`src/feedback/cursor.rs`): the unchanged-check used to run *after* `current_visual`, so the expensive hit-test happened on every idle frame. Now compares the pointer's `(x, y, down)` against the cached visual's same fields up front. `event_seq` was the wrong key — it only ticks on PointerDown / PointerUp, not on Move, so a moving cursor would silently freeze if the short-circuit gated on it.
- **`lazy_list_system` skips `apply_bindings` when scroll position unchanged** (`src/components/lazy_list.rs`): rewriting every pool slot's position via `set_position` every frame was a meaningful chunk of the idle frame budget. Now `continue`s when both `visible_start` and the existing `bound_indices` match the cached values.
- **`press_system` skips `hit_test` mid-drag when pointer stays inside the pressed entity's rect** (`src/widget/state.rs`): during a drag every PointerMove ticks `PointerCursor`'s `(x, y)`, so the existing `snap == last` short-circuit couldn't fire. New fast path looks up the existing Pressed entity's `ComputedRect` and returns early if the cursor is still inside, cutting the per-frame cost on continuing-drag frames roughly in half.

### Fixed

- **Hidden subtree's `OffscreenRender` descendants no longer keep stale cached buffers across theme swaps** (`src/components/tab_pages.rs`): a theme swap while a tab is hidden used to leave that tab's mirror / blur / other offscreen buffers painted in the previous theme's colours, because `mark_subtree_dirty` skipped the Hidden subtree and the unhide path only marked the tab content entity itself. Subtree-wide marking on unhide now invalidates every `OffscreenGeneration` inside. Regression test in `src/components/tab_pages.rs::unhide_marks_whole_subtree_dirty`.

## [0.21.0] - 2026-05-27

Software-renderer fast paths and a new in-place framebuffer-edit trait method.

### Added

- **`Renderer::modify_target_region(rect, |&mut tex| { ... })`** (`src/draw/renderer.rs`): hands the closure a borrowed `Texture` over the framebuffer's own pixels (same `stride` as the framebuffer, no temporary buffer) for "read pixels under me, transform them, write back" operations. Replaces the alloc + sample-copy + blit-back round-trip that effects like `BackgroundBlur` previously did. Default implementation panics; backends opt in by overriding (`SwRenderer` does).
- **2 unit tests for RGB565 blur visual correctness** (`src/draw/sw/blur.rs`): constant-input preservation + bright-pixel spread, covering the new RGB888 scratch path.

### Changed

- **`BackgroundBlur` uses `modify_target_region` on the identity / translate transform path** (`src/components/background_blur.rs`): the common case skips the per-frame scratch allocation. Rotated / 3D-projected widgets still go through the sample-copy + blit-back round-trip because the blur source must stay axis-aligned before the post-transform.
- **`SwRenderer::read_target_region` row-wise `copy_from_slice` fast path for matching formats** (`src/draw/sw/mod.rs`): both shipped callers (`try_draw_offscreen` pre-seed and `sample_target_region`) allocate the dst at the target's own format, so this is the hot path. The format-mismatch path falls through to the original per-pixel loop.
- **`SwRenderer::fill` splits Y-fractional X-aligned rects into top AA strip + memcpy mid + bottom AA strip** (`src/draw/sw/rect_fill.rs`): a row mid-scroll (X-aligned, integer width, sub-pixel top) used to run per-pixel coverage across every interior pixel even though only the top / bottom bands needed it. The split sends the bulk of the rect through `fill_axis_aligned`'s memcpy fast path. Workloads that already produced integer-aligned rects are byte-identical.
- **RGB565 IIR blur uses an RGB888 scratch buffer instead of RGBA8888** (`src/draw/sw/blur.rs`): the alpha channel was dead weight (written to 255 and never read). 25% less scratch memory and 25% less per-pixel scan work. RGBA8888 framebuffers keep their specialised `iir_blur_rgba8888`.

## [0.20.4] - 2026-05-26

### Changed

- **`BackgroundBlur::radius` is `Fixed` instead of `u8`** (`src/components/background_blur.rs`): so the radius can be driven by `Tween` / `Spring` / any animation system through fractional values. The IIR alpha lookup table linearly interpolates between adjacent integer entries, producing a smooth ramp instead of staircase steps. `BackgroundBlur::new(N)` keeps accepting plain integers via `impl Into<Fixed>`. Callers reading `bg.radius` for arithmetic / comparison need `Fixed` ops.

### Added

- **SDL_GPU backend handles trackpad pinch / rotate and scroll wheel** (`src/draw/sdl_gpu/mod.rs`): same `MultiGesture → 2 virtual fingers` translation and `MouseWheel` forwarding the SDL CPU backend already had. `cover_flow_demo` and any scroll demo now responds to two-finger trackpad gestures under `--features sdl-gpu`.

## [0.20.3] - 2026-05-26

### Fixed

- **SDL backend dropped events on busy frames** (`src/surface/sdl.rs`): `poll_event` collected the SDL pump's events into a Vec, returned the first translated one, and dropped the rest. A burst of Down + Move + Up in one frame lost the tail. Now every translated event is queued in a `pending` VecDeque and `poll_event` returns one per call.
- **SDL backend left `Hovered` stuck after the cursor exited the window** (`src/surface/sdl.rs`): `WindowEvent::Leave` was unhandled. Now it produces a far-off-screen `PointerMove` so `hover_system`'s hit-test misses, clearing `Hovered`. The wheel anchor cache (`last_mouse_x/y`) is intentionally unchanged so a Wheel arriving before the next motion still anchors at the last in-window position.
- **SDL_GPU backend had the same pump-drain bug**, plus `MouseMotion` was gated on `mousestate.left()` so it never delivered hover motion (`src/draw/sdl_gpu/mod.rs`). Both fixed; same pending-VecDeque pattern.

### Added

- **`MirrorOf` supports RGB565 / RGB565Swapped source buffers** (`src/components/mirror.rs`): the view fn used to bail when the source's offscreen buffer wasn't RGBA8888.
- **`tests/sdl_poll_event.rs`**: headless regression test under `SDL_VIDEODRIVER=dummy` covering both SDL fixes.

## [0.20.2] - 2026-05-26

Effect widgets — read another widget's rendered texture from inside a view fn.

### Added

- **`mirui::components::MirrorOf`** — flipped + faded copy of a `source` entity, e.g. cover-flow reflections.
- **`mirui::components::TemporalMix`** — per-frame IIR blend of `source`'s current frame with the widget's own previous output. `out_n = α·out_{n-1} + (1-α)·source_n`. Smooths content changes (colour shifts, sprite animation) over many frames at the configured `α = mix/255`. Source-side positional animation does not produce a trail; that's not what this widget is for.
- **`mirui::components::BackgroundBlur`** — frosted-glass IIR blur of the framebuffer pixels behind the widget. Samples at the widget's transformed on-screen position, so an animated translate doesn't leave the source frozen.
- **`World::texture_of(entity)` / `prev_texture_of(entity)`** (`src/widget/offscreen.rs`): current / previous frame's offscreen buffer for any entity in the offscreen pool, returned as `TextureSnapshot`.
- **`App::snapshot_widget(entity) -> Option<Texture<'static>>`**: owned copy of any widget's current rendering. One-off cost; for sustained access use `WidgetTextureRef`.
- **`WidgetTextureRef(Entity)` consumer marker** + **`OffscreenAlphaMode::clear_transparent` source marker**: the consumer holds a refcount on the source's offscreen buffer; the source's buffer init switches from framebuffer pre-seed to alpha=0 clear so effects that read the buffer don't see leftover pixels.
- **`maintain_widget_texture_refs` system** (`SystemSlot::PreRender`): reconciles consumer-source pairs each frame — adds `OffscreenRender` to fresh sources, removes it when the last ref drops, copies the source's `WidgetTransform` onto the consumer (so dirty rects track moves), dirties consumers when the source's content generation changes.
- **`SystemSlot::PreRender` (priority 700)**: final reconciliation slot after user systems write the frame's state but before render.
- **`PaintInflate { left, top, right, bottom }`** (`src/widget/dirty.rs`): per-entity dirty-rect expansion for effects whose paint area extends past the layout rect.
- **`Renderer::sample_target_region(src) -> Option<Texture<'static>>`** (`src/draw/renderer.rs`): logical-pixel rect in, physical-resolution texture out. Effect widgets that read the framebuffer no longer compute the physical buffer size themselves. Default implementation panics; backends opting into framebuffer read must override.
- **Render-time prerender pass** (`src/widget/render_system.rs`): walks `WidgetTextureRef`'d sources and renders their offscreen buffers into the cache before consumers' view fns run. Skipped at zero cost when the tree contains no `WidgetTextureRef`.
- **`gallery/examples/effect_demo`**: three-pane demo (MirrorOf / TemporalMix / BackgroundBlur). `effect_demo_snapshot` headless variant for visual regression. `zorder_baseline` minimal z-order sanity test.
- **`draw/sw/blur::iir_blur_inplace`**: IIR exponential blur via forward + backward 1D passes per axis. O(1) per pixel regardless of radius.
- **`draw/sw/mix::mix_inplace`**: linear blend `out = (1 - mix)·a + mix·b` for RGBA8888 / RGB888 / RGB565.

### Changed

- **`Renderer::read_target_region` default is now `unimplemented!()`** (was: fill `dst` with opaque black). A backend that returns `Some` from `offscreen_format` must override. This surfaces missing implementations loudly instead of silently returning a black region.
- **`SwRenderer::read_target_region`** clips the copy to `min(src_phys, dst)` rows/cols. A caller that allocated `dst` at logical size on a HiDPI viewport used to receive the framebuffer's top-left logical_w × logical_h pixels stretched across the widget; the clip avoids that without changing behaviour at scale=1.

## [0.20.1] - 2026-05-25

Two render artifacts surfaced in v0.20 when an `OffscreenRender` entity carried a `WidgetTransform` translate animation. Both are fixed.

### Fixed

- **Off-screen `Blit` no longer paints into a wrong scanline** (`src/draw/sw/blit_dispatch.rs`): the dispatcher now clips its destination to the target's physical bounds before handing off to the 1× / 2× / DDA paths, so a negative dst x can no longer wrap through `dx0 as usize * 4` into a far-positive byte offset.
- **`OffscreenRender + WidgetTransform` no longer skips render** (`src/widget/render_system.rs`): a redundant cull inside `try_draw_offscreen` used the entity's untransformed layout rect and silently returned when the transform moved the entity off that rect. The duplicate check is removed; the caller's cull already uses the transform-applied bbox.

### Added

- **`gallery/examples/offscreen_modal_demo`** — sliding-modal animation that exercises the `OffscreenRender + WidgetTransform` path, with on-screen render-time readout for both modes.
- **`gallery/examples/offscreen_modal_snapshot`** — manual loop + PPM dump of the same animation, for inspecting the dirty-region path frame by frame without a window server.

## [0.20.0] - 2026-05-25

OffscreenRender: render an entity's subtree into a cached buffer once, blit on subsequent frames when the subtree hasn't changed.

### Added

- **`OffscreenRender` component** (`src/widget/offscreen.rs`): tag any entity with `OffscreenRender::default()` to render its subtree through a buffer cache. `OffscreenRender::with_scale(s)` allocates the buffer at `s × ComputedRect` so the cache trades a smaller buffer for upscaled blit at draw time. Generation tracking via `OffscreenGeneration` invalidates the buffer when the dirty walker sees a self-Dirty or any subtree Dirty.
- **`OffscreenBufferPool` World resource**: byte-budget LRU cache keyed by `(entity, w, h, format, generation)`. `App::with_offscreen_pool_budget(bytes)` opts in; the default budget is 0 so user code must size the pool to its target. Pool seeds buffers from the framebuffer pixels under the entity rect so partial-alpha raster blends against the real background instead of transparent black.
- **`Renderer::supports_offscreen` / `offscreen_format` / `read_target_region`** trait methods (`src/draw/renderer.rs`): backend capability flag, buffer format pick, and framebuffer read for buffer pre-seed. SwRenderer implements all three; SDL_GPU returns `supports_offscreen = false` and the walker falls through to inline.
- **`WidgetTransform` on OffscreenRender entities** (`src/widget/render_system.rs`): translate / scale / rotate apply to the outer Blit so the buffer ends up positioned the same way as inline raster.
- **`MaxSize::Bytes` cache mode + `HasSize` trait** (`src/cache/budget.rs`): byte-aware LRU eviction. `Texture` and `RefCell<T: HasSize>` get blanket impls. SDL GPU `label_cache` wraps `SdlTexture` in a `SizedSdlTexture` newtype to bridge the orphan rule.
- **`or_insert_with_status` API** (`src/cache/factory.rs`): `Entry` extension that returns `(Handle, EntryStatus::Hit | Inserted)` so callers branching on hit avoid a second hashmap lookup.
- **`gallery/examples/offscreen_demo.rs`**: 6×9 rounded-tile dashboard, mode auto-toggles every 5s between inline and offscreen, on-screen `render avg` readout shows the per-frame raster cost difference directly.
- **35 OffscreenRender invariant + edge tests** in `src/widget/render_system.rs::offscreen_render_check`: cold/warm/dirty-bump byte-equivalence vs inline across rounded panels, form pages, single Switch widgets, fractional scale, Hidden interaction, nested-OffscreenRender panic, 3D-transform panic, oversized buffer fall-through, cache-hit skip-raster proof, dirty walker promotion, and WidgetTransform parity (translate / scale byte-equal, rotate within AA tolerance).

### Changed

- **Cache hit skips inner raster** (`src/widget/render_system.rs::try_draw_offscreen`): on hit, skip `read_target_region`, the inner SwRenderer raster, and `Canvas::flush`; only the outer Blit runs. On miss, the full pre-seed + raster path still runs and the new buffer enters the cache.
- **Dirty walker promotes subtree Dirty to OffscreenRender entity**: any Dirty marker under an `OffscreenRender` entity contributes the entity's full rect to the dirty union and bumps the entity's `OffscreenGeneration`, so the next render misses the cache and re-rasters with fresh content. Single-widget OffscreenRender markers (no children) also bump on self-Dirty.
- **`OffscreenBufferPool::with_budget(bytes)`** replaces the previous count-based ctor; `App::with_offscreen_pool_budget(bytes)` is the public entry. Default `Bytes(0)` keeps the cache disabled until the user sizes it.

### Fixed

- **Switch `AnimatedThumbX` is entity-local, not screen-absolute** (`src/components/switch.rs`): `off_thumb_x` / `on_thumb_x` derive from `rect.w` only; the render pass adds `rect.x`. Previously the offset was world-absolute, so the thumb drifted off the track when the panel moved.
- **Buffer pre-seed reads framebuffer pixels** (`src/widget/render_system.rs`): the previous transparent-black clear blended through the alpha channel and painted black fringes around AA edges. SwRenderer's `read_target_region` copies physical pixels under the entity's rect.
- **Inner clip stays in logical coords**: `inner_clip = entity_rect` (logical) instead of physical, so `OffscreenRender::with_scale(0.5)` no longer clips children to a quarter of the buffer.

### Performance

OffscreenRender targets dashboard / form-page UIs where a large subtree is static between frames but other parts of the screen redraw. With the cache hit fast path:

- Inline render of a 280×260 panel containing 54 rounded tiles re-rasters every tile every frame.
- OffscreenRender hit path runs one Blit, skipping all 54 tile rasters and the framebuffer pre-seed.

The desktop `offscreen_demo` example shows the gap directly via the on-screen `render avg` readout when toggling between modes.

## [0.19.2] - 2026-05-23

Software raster speed-up for rounded widget fills on ESP32-C3.

### Changed

- **`fill_rect_inner` axis-aligned + rounded fast path** (`src/draw/sw/rect_fill.rs`): solid-fill the inner cross via `fill_axis_aligned`, leaving only the four `r×r` corner bboxes for the SDF coverage path. Triggers when the area is integer-aligned and `2r < w/h`; circular pills (`2r ≥ w/h`) and fractional-origin areas fall through to the general `aa_loop`.
- **`fill_rect_inner` aa_loop short-circuit**: hoist the row / column / mid-zone integer ranges before the inner loop so each pixel only checks three booleans. When all three plus opa = 255 align, write the pixel directly via `set_pixel`, skipping four Fixed multiplies, the SDF call, and `blend_pixel`'s integer-coordinate dispatch.
- **Direct `blend_pixel_int` calls**: `fill_rect_inner` now feeds integer pixel coordinates straight into `blend_pixel_int(i32, i32, ...)`, skipping the `is_integer()` dispatch in `blend_pixel(Fixed, Fixed, ...)`.

### Fixed

- **`color.a` no longer drops on the rounded fast paths**: a half-transparent colour drawn with `opa = 255` previously rendered as fully opaque source RGB on the axis-aligned + rounded path and on the aa_loop short-circuit. `fill_rect_inner` now folds `color.a * opa / 255` into a single `effective_opa` at entry; the short-circuit also requires `effective_opa == 255` so it only fires when the colour is genuinely opaque.
- **`fill_axis_aligned` empty sub-rect panic**: when the clip overlapped only one corner bbox of a rounded rect, two of the inner-cross calls collapsed to negative-width sub-rects and the `(px_x1 - px_x0) as usize` cast underflowed into a huge byte count that panicked on slice access. Entry guard returns early on empty sub-rects.

### Added

- **Four pixel-level regression tests** in `corner_check`: `rounded_fill_respects_color_alpha_fast_path`, `rounded_fill_respects_color_alpha_aa_loop`, `rounded_fill_respects_color_alpha_general_loop`, and `rounded_fill_clip_covers_only_corner` — covering the axis-aligned fast path, the aa_loop straight-zone short-circuit, the SDF corner path, and the clip-only-corner case respectively.

### Performance

ESP32-C3 demo-widgets reload-storm trace (1300 `sw.fill_aa_loop` calls per 100-frame window):

- `sw.fill_aa_loop` avg: **3211 µs → ~2400 µs** (−25 %).
- Reload-storm fps: 10 → 11–13.
- Idle fps: unchanged at ~104 fps.
- Visual output: pixel-equivalent to v0.19.1 within the existing 4-way symmetry tolerance (folding `color.a` before coverage shifts a fully-covered half-transparent pixel by ±1 alpha vs the pre-fix path; below visual threshold).

## [0.19.1] - 2026-05-22

Runtime cache observability and a friendlier `WithFactory` entry API.

### Added

- **Cache observability primitives** in `mirui::cache`:
  - **`CacheInspect`** — type-erased read-only view (`cache_name` / `cache_stats` / `cache_len` / `cache_max_size`). Blanket-impl'd for every `Cache<K, V, A, L>` so any cache hands out `&dyn CacheInspect` without exposing `K`/`V`.
  - **`InspectCaches`** — multi-cache walk for containers that own several caches (a backend with label + shape caches, a user struct with a mix). Default returns an empty iterator; one-line empty impl is enough for caches-free containers. Wired into `Surface` as a super-trait so backends opt in by overriding a single method.
  - **`CacheStatsSnapshot`** + **`CacheRegistry`** world resource — `App::run` walks the backend's caches once per frame and republishes the snapshot vector, so `post_render` systems read live stats without poking the backend.
  - **`CacheReportPlugin`** — periodic dump every N frames following the `FpsSummaryPlugin` template. Default sink prints `entries / hit / miss (rate%) / max_size` per cache; `with_sink` swaps in a custom sink for telemetry export.
- **`WithFactory` entry API** — `entry(key).or_insert(...)` (no-ctx ctor) / `or_insert_with(|ctor, k| ...)` (per-call ctx via the build closure). Replaces the v0.19.0 `acquire_with(ctx)` shape, which threaded `Ctx` as a generic parameter and required an internal lifetime transmute to plumb backend handles through.

### Changed

- **`LabelCache`** moved onto `mirui::cache::WithFactory`. The hand-rolled `lru = "0.12"` dependency is gone, the lifetime transmute is gone, and the cache now reports itself to the registry as `sdl_gpu/label`.
- **`cache-stats` feature flag** (development-time `LabelCache` print toggle) removed. Subsumed by `CacheReportPlugin` + `CacheRegistry`.

## [0.19.0] - 2026-05-22

### Added

- **`mirui::cache` — generic caching framework**, suitable for any `K → V` workload (offscreen render buffers, font glyphs, decoded images, anything user-defined). Built natively on Rust generics: algorithm and lookup are independent type parameters, and the public surface hands out opaque `Handle<V>` values rather than `Rc<T>` / `Arc<T>` directly.
  - **`Cache<K, V, A: Algorithm, L: Lookup<K>>`** — main type. Algorithm and lookup strategy are independent type parameters; the v0.19.0 release ships `Lru` plus three lookups (`OrdLookup` / `HashLookup` / `LinearLookup`).
  - **Type aliases** `LruCache<K, V>` (default, hash-keyed), `LruBTreeCache<K, V>` (ordered keys), `LruLinearCache<K, V>` (small caches under ~10 entries).
  - **`Handle<V>`** wraps the cache entry without exposing `Rc`/`Arc`. `Handle: Clone` for cross-frame retention; `Handle::is_invalid()` reports whether the entry is still resident in the cache; `Drop` releases the reference automatically.
  - **`CacheBuilder<K, V, A, L>`** with `max_size(MaxSize)` / `on_evict(closure)` / `name(&str)`. `build()` panics if `max_size` was never set — there is no sensible default, since `Disabled` would silently swallow inserts.
  - **Three API shapes** depending on need:
    - `cache.acquire(&key)` — query only, returns `Option<Handle<V>>`.
    - `cache.entry(key).or_insert_with(|| factory(...))` — std `HashMap::Entry`-style, factory closure passed each call. Also `or_try_insert_with` for fallible factories.
    - `WithFactory::new(cache, |k| factory(k))` then `acquire_or_create(key)` — factory configured once at construction time, no closure at every call site.
  - **`Cache::drop(&key)`** marks an entry invalid; **`Cache::evict_one()`** lets the algorithm pick a victim. `on_evict` only fires on algorithm-driven eviction so callers can tell user-initiated removals from capacity pressure.
  - **`MaxSize::Disabled` / `Count(0)`** make the cache reject all inserts; the value still flows through `or_insert_with` as a detached, already-invalid `Handle`, so chains keep typing but `h.is_invalid()` flags it as "never reached the cache".
  - **`CacheStats`** records hit / miss / evict / insert / drop counts; `hit_rate()` is the convenience accessor.
  - **`sync-cache` feature** (off by default) flips `Handle`'s storage from `Rc` to `Arc` and the invalidation flag from `Cell<bool>` to `AtomicBool`. Single-threaded code (including ESP RV32IMC, which lacks atomic load/store on `u32`) does not pay the cost.
  - **No new external dependencies** beyond `hashbrown 0.15` (used internally by `HashLookup`; default-features = false, alloc-only). Slab arena and intrusive linked list are hand-rolled in ~50 lines each.

### Changed

- `Cargo.toml` adds `hashbrown` as a direct dependency. It already ships in `std` internally, so the binary impact is limited to the no_std path that previously had no hash map at all.

## [0.18.1] - 2026-05-22

### Added

- **`BudgetReportPlugin`** — watches `FrameStats` and fires a sink when avg or p99 frame time crosses a configured threshold. Configure with `with_avg_budget(ns)` / `with_p99_budget(ns)` / `with_sink(fn(BudgetViolation))`; `0` disables a threshold. Reads `FrameStats` without draining, so it composes with `FpsSummaryPlugin` and `PerfReportPlugin`.
- **`mirui::perf::format_chrome_event(&PerfEvent, &mut impl core::fmt::Write)`** — emits one Chrome-trace JSON event into any `fmt::Write`, with proper escaping for span names containing `"`, `\`, newlines or control chars. Available on both `std` and `no_std`.
- **`PerfReportPlugin::with_perfetto_line_sink(PerfettoLineSink)`** — accepts a `Box<dyn FnMut(&str)>` so embedded backends can stream Chrome-trace JSON over UART or any other transport. The plugin's perfetto path no longer requires `std`.
- **`PerfettoLineSink` type alias** (`mirui::plugins::PerfettoLineSink`) for the boxed writer.

### Changed

- **BREAKING: `FrameTimings.event_poll_nanos` renamed to `input_nanos`.** The stage covers all per-frame input handling, not just polling; the old name was misleading. Source compatibility is not preserved — update field accesses and any pattern destructures.
- **`FrameTimings.frame_nanos` is now the explicit sum of disjoint stages** (`input + systems + layout + render + flush + seed_prev`) instead of a wall-clock measurement. Plugin `post_render` time is intentionally excluded so reporters don't inflate the budget they're measuring.
- **`FrameStats` and `FrameTimings` land before `Plugin::post_render`** is dispatched, so reporters that read them (e.g. `BudgetReportPlugin`) see the just-finished frame's stats instead of the previous frame's.
- **`FrameStats::p99`** now uses `((len * 99).div_ceil(100) - 1).min(len - 1)`, matching the standard nearest-rank definition. Small-window p99 values are slightly higher than before; tail-latency budgets may need re-tuning.
- **`FpsSummary` no longer pre-drains perf events.** Sinks that want per-span detail call `crate::perf::drain_events()` explicitly; this makes the mutual exclusion with `PerfReportPlugin` visible at the call site.
- **`no_std` perf recorder narrows its critical sections.** The clock function and event allocations run outside the lock; `drain_events` copies the ring atomically inside one critical section so concurrent `trace_span!` invocations can't tear the snapshot.
- `PerfReportPlugin::with_perfetto_writer(path)` is now a thin `std`-only wrapper around `with_perfetto_line_sink`; behaviour is unchanged.

### Fixed

- `FrameStats::p99` no longer underflows on small windows.
- Chrome-trace JSON escaping: span names with quotes, backslashes, or control characters previously produced invalid JSON that downstream parsers (e.g. `tools/esp-trace.py`) would silently drop.

## [0.18.0] - 2026-05-21

### Added

- **`FrameTimings` resource** (`mirui::ecs::FrameTimings`) — `App::run` writes a 7-stage breakdown each frame (`frame_nanos`, `event_poll_nanos`, `systems_nanos`, `layout_nanos`, `render_nanos`, `flush_nanos`, `seed_prev_nanos`). Plugins read via `world.resource()`.
- **`FrameStats` resource** (`mirui::ecs::FrameStats`) — 256-sample sliding window of `frame_nanos`. Exposes `avg / min / max / p99 / jitter` for tail-latency analysis.
- **`crate::perf::set_clock(fn)`** — clock plugins inject the time source so `trace_span!` / `#[trace_fn]` start recording on `no_std` targets. `StdInstantClockPlugin` calls it automatically.
- **`no_std` perf recorder** — 256-event ring buffer guarded by `critical_section`. Drains via `crate::perf::drain_events()`; `aggregate()` produces per-name stats.

### Changed

- **BREAKING: `FpsSummaryPlugin::with_sink`** signature changed from `fn(frames: u32, avg_render_ns: u64)` to `fn(FpsSummary<'_>)`. The new struct exposes per-stage averages, a borrow of `FrameStats`, and the drained `crate::perf` event list, so a single sink can report fps + tail latency + per-span detail.
- **BREAKING: `Plugin::post_render`'s `render_nanos`** now reports rasterization time only (excludes flush + `seed_prev_rects`). For end-to-end frame time, read `FrameTimings.frame_nanos`. Already shipped in v0.17.2; documented here for completeness.
- `crate::perf::aggregate` is now available on both `std` and `no_std`.

### Dependencies

- Adds `critical-section = "1.2"` (default-features = false) for the no_std recorder. Pulls `critical-section/std` automatically when the `std` feature is enabled.

## [0.17.3] - 2026-05-21

### Added

- **`mirui::prelude`** — re-exports `App`, layout types, `Color` / `Dimension` / `Fixed`, `Entity` / `World`, `WidgetBuilder`, theme tokens, and the `ui!` macro. Surface backends, plugins, and individual widget kinds stay on their canonical paths.
- **`mirui::components` re-exports widget types directly** — `use mirui::components::{Button, Slider, Switch};` instead of reaching into each submodule. The deeper `mirui::components::button::Button` path still works.

### Documentation

- README updated for the current API surface.

## [0.17.2] - 2026-05-21

### Fixed

- **`Plugin::post_render` `render_nanos` reports rasterization time only.** Flush and `seed_prev_rects` are excluded; both are still observable as the `frame.flush` and `frame.seed_prev` trace spans.

## [0.17.1] - 2026-05-21

### Added

- **`SystemSlot` enum** (`mirui::ecs::SystemSlot`) with variants `SimInput`, `DeltaTime`, `InteractionState`, `Animation`, `Timer`, `ScrollInertia`, `LazyList`, `TabPages`, `Normal`. `priority()` is `const`; `From<SystemSlot> for i32` is provided. Prefer this over the raw `run_order::FOO` constants in new code.

## [0.17.0] - 2026-05-21

### Changed

- **BREAKING: `App::with_theme` / `with_widget` / `with_widgets` / `with_default_widgets` / `with_default_systems` now take and return `&mut Self` instead of consuming `Self`.** Migration: replace `let mut app = App::new(b).with_X().with_Y();` with `let mut app = App::new(b); app.with_X().with_Y();` — split the constructor onto its own line, then chain the configurators on the binding.

## [0.16.3] - 2026-05-21

### Added

- **`InputFeedbackPlugin`** — opt-in cursor + rotary feedback overlays. Cursor mode `Dot` (default) draws a small circle following the pointer; rotary mode renders a magnetic-membrane water drop on the right edge that responds to rotary detents, wheel scroll, and rotary clicks.
- **`MagneticMembrane`** (`mirui::draw::membrane`) — path-generating helper for the rotary overlay; supports `Flat` and `Arc` boundaries.
- **`DrawCommand::FillPath`** — any `View` can now emit filled paths through `&mut dyn Renderer`.
- **`IgnoreHitTest`** marker (`mirui::widget`) — excludes an entity from `hit_test` while keeping it in layout and render.
- **`Style::absolute_at(rect)`** — convenience constructor for absolutely-positioned widget styles.

## [0.16.2] - 2026-05-20

### Added

- **Multi-touch recognizer events from raw pointer streams.** `GestureRecognizer` now emits `GestureEvent::Pinch { scale_delta, .. }` and `GestureEvent::Rotate { angle, .. }` from two-finger `PointerDown` / `PointerMove` / `PointerUp` sequences. `scale_delta` uses `Fixed64` so repeated incremental pinch updates do not drift under Q24.8 truncation.
- **`SimAction::pinch` and `SimAction::rotate_gesture`.** Simulated input can now drive multi-touch gestures through the normal input pipeline, including anchored gestures that clamp their center / span / radius to the target `ComputedRect`.
- **`gallery/examples/pinch_rotate_demo`.** Demonstrates simulated pinch + rotate, including live delta readout and visual `WidgetTransform` feedback.
- **`Transform::apply_rect`.** 2D affine transforms can now return transformed rect corners directly; `apply_rect_bbox` is derived from that quad.

### Fixed

- **2D transformed widgets now render and dirty correctly.** Dirty collection, full-render `PrevRect` seeding, culling, and HiDPI transformed fill/blit now account for 2D `WidgetTransform` bounds. This fixes stale pixels, clipped rotations, and double-scaled transformed rects on HiDPI surfaces.
- **2D transformed rect fills route through quad rendering.** Fill/border commands under non-translate 2D transforms now use the quad path instead of inverse-sampled rect fill, reducing visible edge jitter during rotation/scale.
- **Anchored multi-touch simulation no longer silently misses targets.** Wide anchored pinch/rotate gestures clamp generated virtual fingers inside the target rect, and out-of-bounds local centers are re-centered to the target.

## [0.16.1] - 2026-05-19

### Fixed

- **`hover_system` / `press_system` short-circuit when `PointerCursor` is unchanged.** v0.16.0 ran a full `hit_test` tree walk twice per frame regardless of input activity; on the ESP32-C3 three-body demo (lots of entities, no pointer input) that dropped fps from ~165 to ~120. v0.16.1 caches the last `(x, y, down, event_seq)` per system and returns immediately when nothing changed. Three-body fps recovers to the v0.15.3 baseline.

  Trade-off: a *static* cursor over *moving* widgets won't re-evaluate hover until the cursor itself moves. Demos that need that behaviour can bump `PointerCursor.event_seq` to invalidate.

### Changed

- **`interactive_states_demo` redesigned for visual clarity.** Each card now starts from its own dim base hue (green / red / blue) so the resting state already differentiates the three. State overlays sit on top: 8% / 12% on_surface for hover/press, 16% error tint for Errored, 38% surface blend for Disabled.

## [0.16.0] - 2026-05-19

Theme: **Interaction Polish**. v0.15.3 left `WidgetState::Hovered` / `Pressed` / `Error` as enum placeholders that fell through to the base colour. v0.16.0 wires them end-to-end: state markers, scheduling slot, free-hover input, and overlay routing.

### Added

- **`UserState` / `InteractionState` enum components** in `mirui::widget::state` (re-exported from `mirui::widget`). `UserState` is user-set (`Disabled` propagates to descendants, `Errored` is self-only). `InteractionState` is system-set (`Hovered`, `Pressed`).
- **`hover_system` / `press_system`** maintain `InteractionState` from `PointerCursor`. Hover only when `cursor.down == false`, press only when `down == true`. `with_default_systems` registers both.
- **`run_order::INTERACTION_STATE = 80`** slot between `DELTA_TIME` and `ANIMATION` for the new systems.
- **`Theme::resolve_in` / `blend_color_in` overlays** for the remaining states: `Hovered` = base + on_surface @ 8%, `Pressed` = base + on_surface @ 12%, `Error` = base + error @ 16%. Disabled keeps the existing 38% / 12% blend.
- **`gallery/examples/interactive_states_demo`** shows all four state transitions on one screen.

### Changed

- **SDL `MouseMotion` forwards `PointerMove` regardless of button state.** Previously only drag motions reached `PointerCursor`; that left `hover_system` permanently inert because the cursor never updated. `scroll_system` was already gated on `ScrollDragState.active`, so existing drag handling is unchanged.

### Removed (BREAKING)

- **`mirui::widget::Disabled` marker.** Migrate `world.insert(e, Disabled)` → `world.insert(e, UserState::Disabled)`, and `world.remove::<Disabled>(e)` → `world.remove::<UserState>(e)`. The `entity_or_ancestor_disabled` helper still exists and walks `UserState::Disabled` instead.

## [0.15.3] - 2026-05-19

### Removed (BREAKING)

- **`ColorToken::OnSurfaceDisabled`** — disabled is a state modifier, not a colour role. Migrate `Token(OnSurfaceDisabled)` → `Token(OnSurface)` and add a `Disabled` marker on the entity.
- **`Style.disabled_alpha`**, **`ViewCtx.disabled_alpha`**, and **`disabled_visual_system`** — `with_default_systems()` no longer registers the system; the field stops existing on `Style` and `ViewCtx`.

### Added

- **`WidgetState` enum** in `mirui::widget::theme` — `Enabled` / `Disabled` route today; `Hovered` / `Pressed` / `Error` are placeholders for follow-up minors and currently fall through to `Theme::resolve`.
- **`Theme::resolve_in(token, state) -> Color`** and **`ThemedColor::resolve_in(theme, state) -> Color`** — Disabled state blends text/icon roles to 38% on Surface and container roles to 12%, computed via `Color::lerp`. Output is opaque RGB so the renderer fast path skips per-pixel alpha modulation.
- **`Theme::blend_color_in(color, state) -> Color`** — applies the same Disabled blend (38% towards Surface) to a free-standing `Color`. `ThemedColor::Raw(c).resolve_in(theme, Disabled)` flows through it, so widgets carrying literal colours dim alongside token-routed ones.
- **`Color::blend_with(self, other, t)`** — self-style alias for `Color::lerp(a, b, t)`. New code reads better with the chain form; existing `Color::lerp(a, b, t)` callers stay valid.
- **`ViewCtx.state: WidgetState`** — `render_system` fills via parent walk for `Disabled`. Custom views read `ctx.state` and call `color.resolve_in(theme, ctx.state)`.

### Changed

- All built-in views — Style, Text, button / checkbox / progress_bar / slider / switch / tabbar / text_input — now resolve `ThemedColor` fields through `resolve_in(theme, ctx.state)`. Image stays at full opacity (the surrounding container conveys disabled state).
- `DrawCommand::Fill` / `Border` / `Label` `opa` returns to `255` across the built-in widget set; disabled tinting is precomputed in the resolved RGB, not a runtime alpha multiplier.

### Note

v0.15.2 (released ~8 hours before v0.15.3) introduced `Style.disabled_alpha` driving per-pixel `opa` modulation. That's a renderer hot path — every fill / blit had to multiply by alpha — and Disabled is a token-state route, not a transparency property. The fix-up window is narrow enough that v0.15.3 is a clean BREAKING bump rather than a deprecation cycle.

## [0.15.2] - 2026-05-19

### Added

- **`mirui::widget::Disabled` marker** disables interaction on the entity and its descendants. Layout and render still run, but pointer events are swallowed at `dispatch_input` entry, focus traversal skips, and visuals dim through `Style.disabled_alpha`. Walk semantics mirror `Hidden` — an ancestor carrying `Disabled` disables the whole subtree. Toggle by `world.insert(e, Disabled)` / `world.remove::<Disabled>(e)`.
- **`ColorToken::OnSurfaceDisabled`** joins the builtin token list as the 15th variant. Default values: dark `rgb(120, 120, 130)`, light `rgb(180, 180, 185)`, tracking Material 3's disabled token role.
- **`Style.disabled_alpha: Option<Opa>`** drives the runtime dim. `disabled_visual_system` (registered by `App::with_default_systems()`) walks every Style entity each frame and writes `Some(97)` (38% × 255, the M3 spec value) on Disabled subtrees, `None` elsewhere. Built-in `Style` background/border and the Text view multiply emitted command opa by this value; custom views opt in by reading `ViewCtx.disabled_alpha`.
- **`mirui::types::Opa` type alias** now carries a doc explaining the 0..=255 convention, and `Style.disabled_alpha` plus `ViewCtx.disabled_alpha` use the alias instead of bare `u8`.
- **`disabled_demo` gallery example** demonstrates toggling `Disabled` on a card, the visual dim effect, and the swallowed gesture path.

### Changed (BREAKING)

- **`ViewCtx` gains a `disabled_alpha: Opa` field.** Code that *constructs* `ViewCtx { ... }` directly (test fixtures, custom render dispatch) must add `disabled_alpha: 255` — `255` for normal entities. User views that just receive a `&mut ViewCtx` (the common case via `render_system`) keep building. New views should multiply emitted command `opa` by `ctx.disabled_alpha`.

### Note

v0.15.2 dims Style-derived colours (`bg_color` / `border_color` / `Text`). Built-in widgets that emit their own fill (Slider track, Switch thumb, ProgressBar bar, etc.) currently render at full alpha when their parent carries `Disabled` — only the surrounding Style fills dim. A future minor will thread `disabled_alpha` through every built-in widget's draw path.

## [0.15.1] - 2026-05-19

### Added

- **`#[mirui::system]` attribute macro** for ergonomic system registration. Annotating `fn(&mut World)` generates a sibling module sharing the fn ident with a `pub const fn system() -> System` constructor. Direct fn calls remain valid (value namespace) while `fn_name::system()` exposes the metadata builder (type namespace) and is `const`-callable for `with_systems` arrays. Defaults: `name` follows the fn ident; `order` falls back to `run_order::NORMAL`. Override either with `#[mirui::system(name = "...", order = ANIMATION)]`.
- **LazyList view auto-registration.** `mirui::components::lazy_list::view()` joins the default registry as a `systems_only` view, matching tab_pages. Demos no longer need `app.add_system(lazy_list_system::system())` — `with_default_widgets()` is enough.
- **`mirui::perf` span tracing infrastructure** with `trace_span!("name")` (RAII) / `trace_span!("name", { block })` / `#[trace_fn]`. On `std` builds spans land in a thread-local ring; `no_std` paths compile to no-ops. `SystemScheduler::run_all` records per-system call count and total wall-clock when a `MonoClock` is wired.
- **`PerfReportPlugin`** prints a console summary on demand and can `with_perfetto_writer(W)` to dump Chrome trace ndjson (drag into ui.perfetto.dev). Exposes `SystemPerfSnapshot` and `PerfResetFlag` resources for in-app dashboards.
- **`SlowSurface<S>` host harness** simulates SPI display latency on the desktop, so frame-budget regressions surface in SDL runs instead of waiting for ESP. Default `NS_PER_PIXEL_SPI_80MHZ_RGB565 = 200` matches a typical RGB565 panel.
- **Software renderer fast path.** `fill_axis_aligned` writes the first scanline then row-replicates without any per-frame `Vec::with_capacity`, taking host fills from ~870 µs/call to ~6.7 µs/call (87× speed-up on the perf_collect scenario). The macro tooling (`#[trace_fn]`, `trace_span!`) is what surfaced this hotspot.

### Changed

- **System registration** across mirui internals (switch / text_input / tab_pages / timer / scroll_inertia / sim_input / sim_timeline / sync_delta_time_ms / lazy_list) now uses `#[crate::system(order = SLOT)]` and `with_systems(const { &[fn::system()] })` instead of explicit `System::new` calls and free-standing `const SYSTEMS` arrays. End-user demos pick up the same form: `app.add_system(my_system::system())`.
- **`mirui-macros` is now a normal dependency** of `mirui` (was dev-only). Library code uses `trace_fn!`/`trace_span!`/`#[system]`, so users get the macros automatically with no opt-in.

### Fixed

- **Scroll demos lost throw animation after v0.15.0** because the new prioritised scheduler doesn't carry `scroll_inertia` unless `with_default_systems()` is called. nested_scroll, scroll, lazy_list, lazy_list_snapshot, snapshot_cover_flow, and cover_flow now wire it explicitly.

## [0.15.0] - 2026-05-19

### Added

- **`mirui::ecs::System` struct + `mirui::ecs::run_order` named slots**. `System` carries a name, a priority, and the `fn(&mut World)`. `run_order` exposes the standard frame phases — `SIM_INPUT` (50), `DELTA_TIME` (60), `ANIMATION` (150), `TIMER` (150), `SCROLL_INERTIA` (250), `LAZY_LIST` (350), `TAB_PAGES` (350), `NORMAL` (500). Lower runs earlier each frame; registration order breaks ties at the same priority. Pick a slot by role; the spacing leaves room for user systems between built-ins.
- **`App::with_default_systems`** now bundles `delta_time` (dt sync), `timer` (declarative timers), and `scroll_inertia` (the inertia spring tick) at their respective `run_order` slots. Demos that previously hand-registered `mirui::anim::sync_delta_time_ms` can drop the call.

### Fixed

- **Inertia spring tail no longer leaves residue.** `scroll_inertia_system` previously gated its `Dirty` mark on `(new - old).abs() >= 1px`, which silently dropped sub-pixel writes during a settling spring even though `ScrollOffset` itself was still changing. Across a tail that adds up to several visible pixels, so the screen kept rendering the position from when the spring launched while ScrollOffset had already snapped to its target. Now any actual numeric change marks Dirty and lets the dirty pass repaint.
- **`Spring::retarget` is idempotent on the same target.** Calling `retarget(0, ...)` every frame while the spring is rebounding past a boundary used to reset `origin = position` each call, collapsing the `is_settled` span toward zero so the spring stalled a pixel out of bounds forever. Same-target calls are now a no-op (the optional config still applies for mid-flight curve swaps), and the boundary rebound converges cleanly.
- **LazyList no longer exposes blank strips during fast drags.** `lazy_list_system` ran in the `systems` phase, which used to execute *before* event/inertia mutated `ScrollOffset`. The same-frame render then saw the new scroll value but the old row positions, so rows entering the viewport hadn't been re-bound. Reordering `systems.run_all` to run after event dispatch and the inertia tick (driven by the new prioritised scheduler) lets `lazy_list_system` slot in at `run_order::LAZY_LIST` and observe the post-event state immediately.

### Migration from v0.14.3

**`add_system` signature changed.** Bare function pointers no longer work:

```rust
// before
app.add_system(my_system);
app.add_system(mirui::anim::sync_delta_time_ms);

// after
app.add_system(System::new("my_system", run_order::NORMAL, my_system));
// dt sync is now part of with_default_systems(); drop the explicit add.
let mut app = App::new(backend)
    .with_default_widgets()
    .with_default_systems();
```

`View::with_systems` likewise expects `&'static [System]` instead of `&'static [fn(&mut World)]`. Custom widgets that contributed systems (cursor blink, tab page visibility, switch animation) need to wrap their fns in `System::new`.

The main `App::run` loop now executes `systems.run_all` *after* event/gesture dispatch (previously it ran before). User systems that observed `ScrollOffset`, `TabBar.selected`, or any other input-driven state will now see this frame's mutations same-frame; systems that produced events for downstream handlers should register at `run_order::SIM_INPUT` (50) so they still run early.

`scroll_inertia_system` is no longer hard-called between event polling and render — it's a normal system at `run_order::SCROLL_INERTIA`, registered automatically by `with_default_systems`. Apps that opt out of `with_default_systems` must register it manually.

## [0.14.3] - 2026-05-18

### Added

- **`mirui::types::DimPoint`**: 2D point with each axis as a `Dimension` (px / percent / auto / content). Resolves against a parent rect into concrete pixel coordinates. `From<Point>` keeps existing fixed-pixel call sites working; `From<(X, Y)>` lets ergonomic literals like `(64, 7)` flow into APIs that take `impl Into<DimPoint>`.
- **`mirui::event::PointerCursor`**: World resource holding the last screen-space pointer position seen by `dispatch_input`, with an `event_seq` counter that bumps on PointerDown / PointerUp (PointerMove leaves it). Single source of truth for "where is the cursor" across sim and real input.
- **`SimAction::tap(point)` / `drag(from, to, dur, ease)` / `wait(ms)`** + **`.on(entity)`** chain: builder API replacing the v0.11.1 enum-variant style. Each ctor returns `SimAction` directly; `.on(entity)` shifts the point's coord system to the entity's local rect, so anchored taps and drags survive layout changes. Coordinate inputs accept `impl Into<DimPoint>` — `Point::new(64, 7)`, `(64, 7)`, `DimPoint::CENTER`, and `DimPoint::percent(10, 50)` all work.
- **`SimAction::TapAction` / `DragAction`** structs (re-exported from `event::sim`): the wrapped values behind `tap(...)` / `drag(...)` for code that wants to inspect the configured action.

### Fixed

- **`render_dirty` path now refreshes ComputedRect** every frame. Previously `update_layout` only ran inside `App::render` (transient backends like SDL); persistent backends (ESP32, embedded LCDs) took the `render_dirty` path which never wrote ComputedRect back into ECS, so any consumer reading the component (sim TapOn, slider drag math, gesture fallbacks) saw stale coordinates from the single startup-frame full render. The fix piggybacks `write_computed_rects` onto the layout pass `collect_dirty_region` was already running — same data, no extra work.
- **Scroll containers with `elastic: false` no longer drift past the content edge**. The drag handler always ran the elastic-resist dampener, which only slowed overscroll, never blocked it; the spring on PointerUp was gated on `elastic` so non-elastic configs left the offset stuck out of bounds, and the next drag would push it further until the list disappeared off-screen. Drag now hard-clamps to `[0, max]` when `elastic = false`.

### Migration from v0.14.2

`SimAction` enum changed from `Tap(Point) | Drag { from, to, ... } | Wait(u32)` to `Tap(TapAction) | Drag(DragAction) | Wait(u32)`. Existing tests and demos using the variants directly need to switch to the constructors:

```rust
// before
SimAction::Tap(Point::new(64, 7))
SimAction::Drag { from: Point::new(20, 80), to: Point::new(100, 80), duration_ms: 600, ease: ease::linear }
SimAction::Wait(500)

// after
SimAction::tap((64, 7))
SimAction::drag((20, 80), (100, 80), 600, ease::linear)
SimAction::wait(500)

// new ergonomic form
SimAction::tap(DimPoint::CENTER).on(switch_entity)
SimAction::drag(DimPoint::percent(10, 50), DimPoint::percent(90, 50), 600, ease::linear).on(slider_entity)
```

The internal `event_seq` field on `PointerCursor` is new; resources holding `PointerCursor` from prior versions don't exist (the resource itself is new).

## [0.14.2] - 2026-05-18

### Added

- **`mirui::timer`** module: `Timer` ECS component, `TimerMode` enum (`After` / `Every` / `Repeat { remaining }` / `Until { deadline_ms }`), and a generic `timer_system` that drives every timer in the World from one walker. Time source is `MonoClock::now_ms()` with `wrapping_sub` comparisons, so schedules survive the 49.7-day u32 epoch wrap as long as `period_ms < 2^31`.
- **`Timer` ctors**: `Timer::after(ms, cb)` / `Timer::every(ms, cb)` / `Timer::repeat(times, ms, cb)` / `Timer::until(deadline_ms, period_ms, cb)`. Callbacks are bare `fn(&mut World, Entity)` — no closure capture; pair with marker components to thread state.
- **`Timer::pause(now_ms)` / `resume(now_ms)`**: idempotent. Resume pushes `next_at_ms` forward by the paused duration so the timer effectively slept through the pause.
- **`mirui_macros::timer!`** macro: declarative sugar over the four ctors. `timer!(Cycle, every: 3_000, |w, e| { ... })` expands to a unit struct with `Cycle::install(&mut World) -> Entity`. Schedule keywords: `after: ms`, `every: ms`, `repeat: N every: ms`, `until: D every: ms`. All four share the generic `timer_system`, so stamping out N invocations doesn't grow the binary.
- **`App::with_default_systems()`**: registers `anim::sync_delta_time_ms` and `timer::timer_system` in one call. Mirrors `with_default_widgets`. Both inner systems no-op when their component / resource is absent, so this is safe to call even if the app uses neither.

### Changed

- `gallery/examples/animation_demo` and other anim consumers can drop their explicit `app.add_system(anim::sync_delta_time_ms)` once they switch to `.with_default_systems()`. The standalone function stays public for apps that prefer to compose their own system list.

### Fixed

- `cargo xtask release` no longer aborts when a patch leaves `mirui-macros` unchanged. The publish step's "already on crates.io" detection now matches both the `already uploaded` and `already exists` wordings cargo emits.



### Added

- **`Theme::with(token, color)`** / **`Theme::with_many(pairs)`**: owning chainable builders next to the existing borrow-mut `set`. Lets palette factories spell `Theme::dark().with(Token, color)…` or seed several tokens from any iterable in one call.
- **`mirui::widget::theme::set_theme(world, theme)`**: free function that hot-swaps the active palette — replaces the World's `Theme` resource, finds the active root, and flags the live tree for repaint. Callable from gesture handlers and systems where an `App` reference isn't available.
- **`App::set_theme(theme)`**: thin forwarder around the free function above, for the common `app.set_theme(...)` style in `main`.
- **`widget::dirty::mark_subtree_dirty(world, root)`**: public helper that flags every entity in the subtree rooted at `root`. Exposed for any global property change (viewport resize, density swap) that needs to invalidate the whole scene.
- **`widget::WidgetRoot`**: World resource cached by `App::set_root` so handlers and systems can reach the active root without an `App` reference.

### Changed

- `gallery/examples/theme_swap_demo` collapses its tap handler to a single `theme::set_theme(world, ...)` call (gone: in-demo recursive `mark_subtree_dirty`, manual `Children` query, hand-walked Dirty insertion). The three palette factories build through `Theme::with` / `with_many` instead of mut-borrow chains.

## [0.14.0] - 2026-05-18

### Added

- **`ColorToken`** (`mirui::widget::theme::ColorToken`): an enum of fourteen builtin tokens (`Primary`, `OnPrimary`, `Secondary`, `OnSecondary`, `Tertiary`, `OnTertiary`, `Surface`, `OnSurface`, `SurfaceVariant`, `OnSurfaceVariant`, `Success`, `Error`, `Outline`, `Shadow`) plus `Custom(&'static str)` for user-defined tokens. `ColorToken::custom("name")` is a `const fn` so user code can declare module-level constants.
- **`ThemedColor`** (`mirui::widget::ThemedColor`): an enum wrapping either a fixed `Color` (`Raw`) or a `ColorToken` reference (`Token`). Resolves to a concrete `Color` against a `Theme` at render time. Implements `From<Color>` and `From<ColorToken>`, so any builder method taking `impl Into<ThemedColor>` accepts both literal colours and token references.
- **`Theme::set(token, color)`** / **`Theme::resolve(token) -> Color`** / **`Theme::unset(token)`**: uniform API for both builtin and custom tokens. Internally `Theme` stores builtins as struct fields (fast path) and custom tokens in a `BTreeMap`, but the public surface is the same regardless. `Theme::unset` is a no-op for builtins (which always have a value).
- **`Style::set_bg_color`** / **`set_border_color`** / **`set_text_color`** / **`clear_bg_color`** / **`clear_border_color`**: ergonomic setters for runtime mutation that accept `impl Into<ThemedColor>`. Replace `style.bg_color = Some(c.into())` with `style.set_bg_color(c)`.

### Breaking

Every colour field across the public API switches from `Option<Color>` (or `Color`) to `ThemedColor` or `Option<ThemedColor>`. The default values point at semantic tokens, so widgets follow the active `Theme` automatically. Per-instance overrides now accept either a literal `Color` or a `ColorToken` through the `impl Into<ThemedColor>` builder signature.

| widget / type | field | before (v0.13.1) | after (v0.14.0) default |
|---|---|---|---|
| Button | normal_color | `Option<Color>` (`None` → `theme.surface_variant`) | `ThemedColor::Token(SurfaceVariant)` |
| Button | pressed_color | `Option<Color>` | `Token(Primary)` |
| Checkbox | checked / unchecked_color | `Option<Color>` | `Token(Primary)` / `Token(SurfaceVariant)` |
| ProgressBar | fill / track_color | `Option<Color>` | `Token(Primary)` / `Token(SurfaceVariant)` |
| Slider | fill / track / thumb_color | `Option<Color>` | `Token(Primary)` / `Token(SurfaceVariant)` / `Token(OnPrimary)` |
| Switch | on / off / thumb_color | `Option<Color>` | `Token(Success)` / `Token(SurfaceVariant)` / `Token(OnPrimary)` |
| TabBar | indicator_color | `Option<Color>` | `Token(Primary)` |
| TextInput | text / placeholder / cursor / focus_border_color | `Option<Color>` | `Token(OnSurface)` / `Token(OnSurfaceVariant)` / `Token(OnSurface)` / `Token(Primary)` |
| Style | bg_color | `Option<Color>` | `Option<ThemedColor>` (default `None`) |
| Style | border_color | `Option<Color>` | `Option<ThemedColor>` (default `None`) |
| Style | text_color | `Option<Color>` (`None` → hardcoded `rgb(255,255,255)` in render) | `ThemedColor` (default `Token(OnSurface)`, always paints) |

`Theme` itself loses its public field access. Code reading `theme.primary` directly migrates to `theme.resolve(ColorToken::Primary)`; code writing `theme.primary = c` migrates to `theme.set(ColorToken::Primary, c)`. The Theme builder methods (`Theme::dark`, `Theme::light`) keep working unchanged.

Migration template:

```rust
// before
let mut t = Theme::dark();
t.primary = Color::rgb(255, 0, 0);

// after
let mut t = Theme::dark();
t.set(ColorToken::Primary, Color::rgb(255, 0, 0));
```

```rust
// before — colour pinned to the success token at construction
let theme = world.resource::<Theme>().unwrap();
let s = Slider::new(0, 100).with_fill_color(theme.success);
//   ^^ frozen at construction; subsequent theme swaps do not update it.

// after — slider's fill tracks success across theme swaps
let s = Slider::new(0, 100).with_fill_color(ColorToken::Success);
```

### Examples

- `gallery/examples/theme_swap_demo.rs` simplified: dropped the `ThemedSurface` / `ThemedOnSurface` marker components and the `theme_style_system` workaround that was needed in v0.13.1. Now `Style.bg_color` and `Style.text_color` are `ThemedColor` so binding them to a token is one line. Adds a `Custom("accent")` token bound by every preset theme; the demo's accent swatch reads `bg_color: ACCENT` directly.

### Internal

- `install_default_registry` (test-only helper) now seeds `Theme::default()` so render-system unit tests that build a `World` directly get the resource that `ViewCtx::theme` expects.
- 197 unit tests + 4 integration tests + 9 gallery snapshots all pass; snapshots remain pixel-equal across the migration because every fixture sets `text_color` explicitly. The `Style.text_color` default change from a hardcoded white fallback to `Token(OnSurface)` only affects code that didn't set it explicitly.
- ESP `demo-widgets` binary grows ~6 KB (504 KB → 510 KB) for the BTreeMap addition. Acceptable headroom on the 4 MB partition.

## [0.13.1] - 2026-05-18

### Added

- **`Theme` resource** (`mirui::widget::Theme`) carrying twelve semantic colour tokens: `primary`, `on_primary`, `secondary`, `on_secondary`, `tertiary`, `on_tertiary`, `surface`, `on_surface`, `surface_variant`, `on_surface_variant`, `success`, `error`, `outline`, `shadow`. Eight cover the existing built-in widgets; four (`secondary` / `tertiary` / `outline` / `shadow`) are reserved up front so future widgets don't force another `Theme` shape change.
- **`Theme::dark()`** (the default) and **`Theme::light()`** ship with the crate. `App::new` automatically inserts `Theme::default()` so render fns can rely on the resource being present.
- **`App::with_theme(Theme)`** builder. Runtime swap via `world.insert_resource(...)` also works (callers should mark widgets `Dirty` themselves; automatic invalidation is a follow-up).
- **`ViewCtx::theme(&self, world: &World) -> &Theme`** accessor for render fns. Render fns that don't read the theme don't pay the resource lookup.

### Breaking

Every built-in widget's colour fields are now `Option<Color>`. `None` means "fall through to the active `Theme`"; `Some(c)` is a per-instance override. Each widget's `::new()` constructor drops its colour arguments; per-colour builder methods cover overrides.

```rust
// before (v0.13.0)
let s = Slider::new(Fixed::ZERO, Fixed::from_int(100));
//   colour fields hardcoded to literal RGB inside ::new

// after (v0.13.1)
let s = Slider::new(Fixed::ZERO, Fixed::from_int(100));
//   colour fields default to None; render falls through to theme.primary etc.

// per-instance override
let s = Slider::new(Fixed::ZERO, Fixed::from_int(100))
    .with_fill_color(Color::rgb(255, 100, 100));
```

Migration table:

| widget | before | after |
|---|---|---|
| `Button` | `Button::new(normal, pressed)` | `Button::new()` + `.with_normal_color` / `.with_pressed_color` |
| `Checkbox` | `Checkbox::new(checked, unchecked)` | `Checkbox::new()` + `.with_checked_color` / `.with_unchecked_color` |
| `ProgressBar` | `ProgressBar::new(fill, track)` | `ProgressBar::new()` + `.with_fill_color` / `.with_track_color` |
| `Slider` | `Slider::new(min, max).with_colors(track, fill, thumb)` | `Slider::new(min, max)` + `.with_track_color` / `.with_fill_color` / `.with_thumb_color` |
| `Switch` | `Switch::new().with_colors(on, off, thumb)` | `Switch::new()` + `.with_on_color` / `.with_off_color` / `.with_thumb_color` |
| `TabBar` | `TabBar::new(n).with_indicator(color, height)` | `TabBar::new(n)` + `.with_indicator_color` / `.with_indicator_height` |
| `TextInput` | `TextInput::new()` (literal defaults) | `TextInput::new()` + `.with_text_color` / `.with_placeholder_color` / `.with_cursor_color` / `.with_focus_border_color` |

`Theme::dark()` reproduces the v0.13.0 hardcoded palette byte-equivalently — apps that didn't pass custom colours render pixel-identically across the upgrade. `Slider::with_colors(t, f, h)` and `Switch::with_colors(o, off, th)` and `TabBar::with_indicator(c, h)` three-arg helpers are gone; their two/three component builders cover the same need with one role per call.

`Style.text_color` / `Style.bg_color` / `Style.border_color` are unchanged. They're entity-level overrides and don't fall through to `Theme`.

### Examples

- New `gallery/examples/theme_swap_demo.rs` — three picker buttons swap between dark / light / a custom palette at runtime; the showcase below (Slider, Switch, Checkbox, ProgressBar, TextInput, TabBar) repaints in the new palette on the next frame.

### Internal

- `View` constructor builders (`View::new`, `.with_attach`, `.with_systems`) carried over from v0.13.0 made every per-widget migration mechanical — no changes to `app.rs`, `render_system.rs`, or `widget_input.rs`.
- All six gallery snapshots and three `text_input_snapshot` cases pixel-equal across the upgrade.
- ESP `demo-widgets` binary unchanged at 504 KB.

## [0.13.0] - 2026-05-17

### Breaking

Widget registry refactor — `App::new` no longer auto-installs every shipped widget, `View` is constructed through `View::new` + builder methods instead of struct literal, and the user-facing widget API condenses from four methods (`with_widget` / `with_default_widgets` / `register_view` / `register_default_widgets`) down to two.

Migration:

1. **`App::new(backend)` now returns an empty registry.** Recover the pre-v0.13 behaviour with `.with_default_widgets()`:
   ```rust
   // before
   let mut app = App::new(backend);
   // after
   let mut app = App::new(backend).with_default_widgets();
   ```
   For a leaner binary, skip `with_default_widgets()` and chain only the widgets you actually need.

2. **`App::register_view(view)` is now `App::with_widget(view)` (owned `self`).** Use it to add user-defined widgets next to the built-ins:
   ```rust
   let mut app = App::new(backend)
       .with_default_widgets()
       .with_widget(my_diamond::view());
   ```

3. **`View` struct fields are private; use `View::new` + builders.**
   ```rust
   // before
   View {
       name: "Diamond",
       priority: 60,
       render: diamond_render,
       auto_attach: None,
       systems: &[],
   }
   // after
   View::new("Diamond", 60, diamond_render)
       .with_attach(diamond_attach)   // optional
       .with_systems(&[my_system])    // optional
   ```
   Marker widgets (no rendering, only systems) keep `View::systems_only(name, &[…])`.

4. **`view.systems` slice no longer reachable from the App side.** `View::install` is the new install hook — App calls `view.install(&mut world, sink)` and `View` decides what to push into the scheduler. User code should never have read `view.systems` directly, but if you did, switch to `view.name()` for diagnostics or accept that the rest of the fields are no longer part of the public API.

### Changed

- **Per-widget gesture / key handlers and the cursor-blink resource live next to their renderers.** `button_handler`, `checkbox_handler`, `tabbar_handler`, `progress_bar_handler`, `textinput_gesture_handler`, `textinput_key_handler`, `cursor_blink_system`, and `CursorBlinkPhase` moved from `src/event/widget_input.rs` to the corresponding `src/components/<x>.rs` files. `CursorBlinkPhase` and `cursor_blink_system` are still re-exported from `event::widget_input` for backwards compatibility — the canonical path is now `mirui::components::text_input`.
- **Widgets contribute their own per-frame systems through `View`.** Switch's three animation drivers and `tab_pages_system` are no longer hard-coded in `App::with_factory`; each widget's `view()` constructor declares the systems it needs and `App` drains them at registration time.
- **`gallery/text_input_demo.rs`** no longer needs `app.add_system(cursor_blink_system)` — the `TextInput` view registers it automatically.

### Removed

- `App::register_view` / `App::register_default_widgets` (replaced by `with_widget` / `with_default_widgets`).
- `crate::widget::view::default_registry` (use `ViewRegistry::with_builtins` for non-`App` test fixtures, `App::with_default_widgets` for production).
- `ViewRegistry::register` / `ViewRegistry::sort_by_priority` (collapsed into `ViewRegistry::insert`, which keeps the vec sorted on each insertion).
- `ViewRegistry::all_systems` (the App side calls `view.install` per view; nobody else needed the bulk view).

## [0.12.3] - 2026-05-17

### Changed

- **`render_system::draw_tree` collapsed into `draw_tree_offset`.** The two walkers were near-duplicates — `draw_tree` was just `draw_tree_offset` specialised to `(0, 0)` offsets, and recursion already passed through the offset variant after the first frame. The two render entry points now call `draw_tree_offset` with `Fixed::ZERO` offsets directly. Pixel-equal across all gallery snapshots; ESP three-body per-frame timing unchanged; binary shrinks by ~14 KB.

### Examples

- New `gallery/examples/custom_view_demo.rs` — a fully user-defined widget (`Diamond`, four stroked Lines) registered through `App::register_view`. Demonstrates parity with built-in views: zero core changes required to ship a new widget kind. Tap any diamond to cycle through three colours.

## [0.12.2] - 2026-05-17

### Fixed

- **`hit_test` mis-routed pointer events when a Hidden subtree carried a non-zero `ScrollOffset`.** `build_rects` skipped Hidden subtrees but `compute_scroll_offsets`, `compute_transforms`, and `compute_transforms_3d` did not. They share the same per-entity Vec indexed by walk order, so any Hidden subtree's scroll/transform leaked into a visible cousin's slot. Symptom in v0.12.1's ESP `demo_widgets`: the Switch toggle in tab 2 worked the first time and silently dropped on later cycles after the LazyList in tab 0 had been scrolled. Fix: gate every recursive walker on the same `Widget && !Hidden && Style` triple `build_rects` uses, and document the per-entity-Vec walk-alignment invariant at the top of `event::hit_test`.

### Tests

- New unit and integration tests for systems whose regressions tend to be invisible until ESP runtime:
  - `hit_test_skips_hidden_subtree_scroll_offset` — minimal 5-entity tree that pins the walk-alignment invariant the v0.12.2 fix introduced.
  - `spring_settle_stress_1000` — 1000 randomised `(from, to, duration, bounce)` combinations must converge within `3 × duration` (excluding the documented unstable `bounce ≥ 0.8` region). Catches integration blow-ups and stiffness/damping table regressions.
  - `switch_n_tap_toggles_n_times` — 100 sequential Tap events must produce exactly 100 toggles.
  - `slider_handler_clamps_ratio_at_boundaries` — 7 probe positions (±1 px around the rect edges, plus far outside) all keep `ratio` in `[0, 1]`.
  - `tests/sim_demo_widgets.rs` — a host-only end-to-end smoke test that assembles TabBar + Slider + Switch through the public API and drives synthetic Taps through the same `dispatch_input` + `bubble_dispatch` path `App::run` uses. Build breaks if any of `Slider`, `Switch`, `TabBar`, `dispatch_input`, `bubble_dispatch`, `attach_widget_input_handlers`, `install_default_registry`, or `render_system::update_layout` becomes inaccessible to a third-party crate.

## [0.12.1] - 2026-05-16

### Added

- **Checkbox / ProgressBar / TabBar / TextInput / Image / Text now ship as registered `View`s** alongside v0.12.0's Button + Style. Each kind owns its render fn and (where applicable) auto_attach fn in `components/<name>.rs`; `App::default_views()` registers them all at startup. Built-ins:
  - Button (priority 40), Checkbox (40)
  - Style (50)
  - ProgressBar (60), TabBar (60), TextInput (70), Image (70), Text (80)
- `mirui::components::text::Text` is the new home for the `Text` component.

### Changed

- **`Text` component moved**: `mirui::widget::Text` → `mirui::components::text::Text`. **Breaking** for any user code importing the old path. Same `pub struct Text(pub Vec<u8>)` definition; only the path changed. The relocation aligns Text with every other widget kind (`components::<name>` is now the universal home for built-in widgets).
- **`render_system.rs` walker no longer issues `DrawCommand`s directly** — every paint flows through a registered view. The two duplicated render walkers (`draw_tree` for absolute coords, `draw_tree_offset` for scrolled descendants) shrank from 1262 to 942 lines (-25.4%). The if-else cascade in `attach_widget_input_handlers` is gone too — `auto_attach` runs against the registry.
- **`textinput_gesture_handler` / `textinput_key_handler` / `tabbar_handler` / `progress_bar_handler` / `checkbox_handler` / `button_handler`** are now `pub(crate)` instead of `fn`-private, so the per-kind `*_attach` fns in their respective `components/<name>.rs` files can install them. They're not part of the public API.

## [0.12.0] - 2026-05-16

### Added

- **`View` registry** — a per-kind dispatch entry (`render` fn pointer + optional `auto_attach` fn pointer + `priority: u8`) lifted out of the if-else chains in `render_system.rs` and `widget_input.rs`. Built-in widgets register through `App::default_views()`; user-defined kinds register via `App::register_view(my_kind::view())`. New `widget::view` module exports `View`, `ViewRegistry`, `ViewCtx`, `ViewRender`, `ViewAttach`, and `install_default_registry(&mut World)` (the last is for tests that build a `World` without `App`).
- **`ViewCtx.bg_handled` mutable flag**: explicit-bg widgets (e.g. Button) emit their own background fill and set the flag; the generic Style stage sees it and skips its own bg fill while still emitting a border. Replaces the old Button/Checkbox-bg cascade hardcoded into `style_render`.
- **Button now ships as a registered `View`** (priority 40). `components::button::view()` returns the entry; `button_render` emits its current-state fill, `button_attach` installs the gesture handler if user code hasn't.
- **Style ships as a registered `View`** (priority 50, no `auto_attach`). `widget::style_view::view()` returns the entry; `style_render` reads `ctx.bg_handled` to decide whether to emit a bg fill.

### Changed

- **`mirui::components::tab_view` module renamed to `mirui::components::tab_pages`**, and `tab_view_system` renamed to `tab_pages_system`. **Breaking change**: user code importing `mirui::components::tab_view::TabContent` or the system fn needs to swap the module path. The `TabContent` struct itself is unchanged. The rename frees the `View` noun for the registry abstraction so "View as widget kind definition" doesn't clash with "View as UI instance" reading inherited from iOS-style `tab_view`.
- **Render walkers (`draw_tree` / `draw_tree_offset`) now dispatch through the `ViewRegistry`** before falling back to the legacy hardcoded path for widget kinds that haven't migrated yet (`ProgressBar`, `TabBar`, `TextInput`, `Image`, `Text`). Snapshot output is pixel-equal across `tabbar_*`, `text_input_*`, `lazy_list_*`. ESP three-body baseline 5.45-6.16 ms (≤ 6.5 ms target, no regression).
- **`attach_widget_input_handlers` runs registry-driven `auto_attach` first**, then falls back to its existing cascade for unmigrated kinds. The Button branch is now driven by `button::view().auto_attach`; user-supplied `GestureHandler` overrides still win in both paths.
- **`style_view::style_render` no longer reads Button**; the bg cascade for Checkbox stays inline temporarily until Checkbox migrates.

## [0.11.5] - 2026-05-16

### Added

- **`Hidden` marker component** (`src/widget/visibility.rs`): toggling `Hidden` on an entity skips it and its entire subtree in layout, rendering, and hit-test (`display: none` semantics — siblings collapse up). Toggle by inserting or removing the marker; the existing dirty-region machinery handles the repaint on transition. Generic primitive — modal / accordion / conditional UI all build on it.
- **`TabContent { tab_bar, index }` component** + **`tab_view_system`** (`src/components/tab_view.rs`): pair an entity with a `TabBar` and a tab index. The built-in system, registered by default in `App::with_factory`, drives a 220 ms `Tween` on every `TabBar` whose `selected` changed (writing `indicator_offset`) and flips `Hidden` on every `TabContent` so only the active page is visible. `TabContent` entities can live anywhere in the tree, not just under the `TabBar`.

### Changed

- **TabBar demos drop the user-side `AnimateTabIndicator` macro and `LastTab` observer**: every v0.11.3 TabBar demo was duplicating an `animate!`-defined indicator slider plus an observer system to detect `selected` changes. That pattern is now built in. Existing user code can delete the macro and the observer; just attach `TabContent { tab_bar, index }` to each page entity.
- **Examples moved into a `gallery/` workspace member crate**. Root `Cargo.toml` no longer carries 35 `[[example]]` blocks; Cargo's `autoexamples` picks every `gallery/examples/*.rs`. Adding a new example is one new file in `gallery/examples/` and nothing else. Run with `cargo run -p gallery --example <name>`. `mirui`'s own `default-features` stay `["quad-aa"]` (no implicit `sdl` pull), so embedded consumers are unaffected. `sdl_gpu_demo` is the one example still requiring an explicit `[[example]]` block — it gates on the `sdl-gpu` feature inside `gallery/Cargo.toml`.
- **Snapshot examples write to `MIRUI_SNAPSHOT_DIR`** (env var) when set, otherwise to the current working directory. No source-level hardcoded output paths.

## [0.11.4] - 2026-05-16

### Changed

- **`Spring::is_settled` is now amplitude-aware**: threshold scales with the spring's own travel (`span/200` for distance, `2·span/sec` for velocity), with `Fixed::ONE` as the floor. The old absolute pixel thresholds (`dist < 1 && v < 50`) treated any normalized 0..1 spring as already-settled, which is why the slider example's switch bg fade had to use `Tween` as a workaround. Both desktop and ESP demos are back on `Spring`; ESP traces now show continuous `t` (e.g. `0.008 → 0.039 → … → 0.92`).
- **`Spring::retarget` re-anchors the spanScale origin** to the live position, so subsequent retargets pick up the new amplitude rather than the original `from → target` span.
- **`ColorFormat::ARGB8888` renamed to `ColorFormat::RGBA8888`** to match the actual `[r, g, b, a]` byte order. **Breaking change**: every caller of the old variant name needs to swap. ESP demos already use `RGB565Swapped` and are unaffected.
- **v0.11.3 widget demos rewritten with `ui!` + enchant blocks**: `text_input_demo`, `text_input_snapshot`, `lazy_list_demo`, `lazy_list_snapshot` were hand-rolling `WidgetBuilder + Parent + Children::insert`; they now use the DSL the way `slider_switch_demo` and `demo_widgets` do. `LazyList`'s pool entities go through `walk` and the `Children` list is read back to populate `LazyListPool`.

### Removed

- `DeltaTime(f32)` and `ElapsedTime(f32)` resources. They were registered in `App::with_factory` but never written or read by any system or example. `DeltaTimeMs(u16)` is the only delta-time resource the animation pipeline uses.

## [0.11.3] - 2026-05-15

### Added

- **`TabBar` component**: discrete-index horizontal tab strip with an animatable indicator. Tap snaps `selected` and `indicator_offset`; smooth slide is opt-in via an `animate!` component writing `indicator_offset`. Auto-attached gesture handler maps a tap inside the bar to the tab index.
- **`TextInput` component**: single-line ASCII text widget with a fixed 32-byte inline buffer (no heap, no `heapless` dep). Carries its own colors. Auto-attaches `Focusable`, a `KeyHandler` for Backspace / Delete / Left / Right / Home / End, and reads `CharInput` for printable ASCII insertion. Optional `Placeholder(&'static str)` companion component renders dimmed text when the buffer is empty. Cursor is a 1-px stripe rendered at `text_x + cursor * 8` (the bitmap font is fixed-width 8).
- **`CursorBlinkPhase` resource + `cursor_blink_system`**: flips a global bool every 500 ms, marks every focused TextInput as Dirty when the phase flips. Idle UI pays nothing.
- **`LazyList` virtual scroll**: equal-height vertical list backed by a fixed-size entity pool (`LazyListPool`) and a user-supplied `ItemBinder` fn. `lazy_list_system` rebinds pool slots whose mapped row index changed and repositions them at `index * item_height`; ScrollOffset / ScrollConfig drive the visible window.
- **`KEY_*` constants** for editing keys: `KEY_BACKSPACE`, `KEY_DELETE`, `KEY_LEFT`, `KEY_RIGHT`, `KEY_HOME`, `KEY_END`, `KEY_RETURN`, `KEY_ESCAPE`. SDL and sdl-gpu backends translate `Event::KeyDown` into `InputEvent::Key { code, pressed: true }` for these.

### Changed

- SDL and sdl-gpu backends emit non-text key presses (Backspace / arrows / Home / End / Return / Delete) as `InputEvent::Key`. Previously only `CharInput` reached the app.

## [0.11.2] - 2026-05-15

### Added

- **Spring animation system**: Apple WWDC23-style physical spring (`Spring::new(from, to, duration_ms, bounce)`) with `retarget(target, config)`, velocity inheritance, and presets `SMOOTH` / `SNAPPY` / `BOUNCY` / `INTERACTIVE`. `SpringMode::Once` and `Repeat`.
- **`Tween`** (renamed from `Animation`) — deterministic duration + ease curve animation.
- **`Motion` enum** unifying `Tween` and `Spring` behind a single `tick`/`value`/`is_done` interface.
- **`animate!` proc macro** (replaces `animation!`): generates a wrapper struct around `Motion`, callers attach a `Tween` or `Spring` via `.into()`.
- **`MotionComponent` trait** + `run_motion<T>` system helper.
- **`GestureEvent::DragEnd { vx, vy }`** carries pointer velocity (px/s) for natural gesture-to-spring handoffs.
- **Scroll inertia and elastic now use `Spring` physics** instead of velocity decay (`vel *= 9/10`). Scroll target stays inside bounds via `BOUNCY` retarget; only marks `Dirty` when the scroll position changes by ≥ 1 px.
- **`Style::clip_children: bool`** — when set, descendants are clipped to the widget's own rect (CSS `overflow: hidden` semantics). Buildable via `ui!`'s `clip_children: true` attribute.
- **`Color::lerp(a, b, t)`** — 8-bit channel-space linear interpolation, clamped to `[0, 1]`.
- **ESP framebuffer capture tooling** (in `mirui-examples/examples/esp32c3-animation`): periodic base64 dump over UART with a host-side decoder script.

### Changed

- **`Spring::tick` integration**: substep semi-implicit Euler with stability bound `ω₀·dt < 2`, capped at 32 substeps per frame; intermediate state in `Fixed64` (Q48.16) for sub-millisecond `sub_dt` precision.
- **`config_to_params` rewritten** in `Fixed64` arithmetic with `Fixed::PI`, removing hand-rolled raw integer math.

### Fixed

- **Nested scroll dirty regions**: `collect_dirty_walk` now accumulates ancestor `ScrollOffset` so widgets inside a scrolled container repaint at the right screen position. Without this fix, repaints of inner scrolls landed at the wrong rect after the outer scrolled.
- **Rounded corners read as flat-topped**: the 1-px AA boundary collapsed circular curvature into a single pixel row, so `r=16` corners looked like flat pills. The boundary now does 4×4 supersampling within a 2-px ring; inside `r-1` and outside `r+1` short-circuit. ~50 µs / frame on a 64×64 r=32 release benchmark.
- **`Spring` damping was 2× too large**: `config_to_params` used `4 * two_pi_raw` (= 8π) instead of 4π, so every spring landed at ζ=2 (overdamped). 200 ms toggles now settle in ~144 ms with proper critical damping.
- **`ScrollSpring` not cleared on `PointerDown`**: a still-running inertia spring could fight the new gesture's scroll resolution. Now cleared the moment the pointer goes down.

### Removed

- `Animation` struct, `animation!` macro, `AnimationComponent` trait, `run_animation` (replaced by `Tween` / `animate!` / `MotionComponent` / `run_motion`).
- `EaseCurve` struct and per-ease derivative functions (used only by the short-lived spatial-uniform animation mode).
- Spatial-uniform animation mode (its problem domain is better solved by `Spring`'s amplitude-aware physics).

## [0.11.1] - 2026-05-15

### Added

- `Texture::from_static` const constructor for compile-time texture data.
- `Point::new(impl Into<Fixed>, impl Into<Fixed>)` convenience constructor.
- `MonoClock::now_ns()` / `now_ms()` helper methods.
- Simulated input framework: `SimulatedInput` (low-level event replay) and `SimTimeline` (high-level eased actions: `Tap`, `Drag`, `Wait`).

### Changed

- **Unified time source**: removed `App::clock` field and `ClockFn` type. All timing now reads from the single `MonoClock` resource (written by `StdInstantClockPlugin` / `SystimerClockPlugin`).
- **Renamed `FrameClock` → `MonoClock`**, moved from `anim` to `ecs::time`. Breaking change for code referencing `mirui::anim::FrameClock`.
- `SimAction` uses `Point` instead of separate x/y fields.
- Examples no longer manually register `FrameClock`; the clock plugin handles it.

### Removed

- `App::clock` field (replaced by `MonoClock` resource).
- `ClockFn` type alias.

## [0.11.0] - 2026-05-14

### Added

- **Animation framework** (`src/anim/`):
  - `Animation` struct with `PlayMode` (Once / Loop / PingPong) and 6 easing curves.
  - `animation!` proc macro — one-line animation component definition.
  - `FrameClock` resource for `no_std`-compatible monotonic time.
  - `run_animation<T>` helper: tick + apply + auto-remove on completion.
- **Event system** (`src/event/`):
  - `InputEvent` unified enum: `PointerDown/Move/Up` (multi-touch `id`), `Rotary` (encoder/crown), `CharInput`, `Key` (with hardware button codes).
  - `GestureRecognizer` state machine producing `Tap`, `LongPress`, `DragStart/Move/End`.
  - `GestureHandler` component (fn pointer, no heap) + `bubble_dispatch` via `Parent` walk.
  - `FocusState` + `Focusable` + `KeyHandler` for keyboard/char routing to focused widgets.
  - Scroll system handles `Rotary` events (20px/step on last-resolved scroll target).
- **Interactive widget components** (`src/components/`):
  - `Slider` — Fixed-point value range with track/fill/thumb.
  - `Switch` — on/off toggle with animated thumb transition.
- **`ComputedRect`** — layout-computed screen rect on every entity, decoupled from dirty tracking.

### Changed

- **`InputEvent` variants renamed**: `Touch` → `PointerDown`, `TouchMove` → `PointerMove`, `Release` → `PointerUp`. Added `id: u8` field for multi-touch.
- **Event module reorganised**: `src/event/` now contains `input.rs`, `gesture/`, `scroll/`, `focus.rs`, `widget_input.rs`, `hit_test.rs`.
- **`button_system` replaced** by per-widget `GestureHandler` components. `App::set_root` auto-attaches handlers for Button/Checkbox/ProgressBar.
- **Legacy `EventHandler` (Box callback) + `WidgetEvent` + `dispatch.rs` removed**. Use `GestureHandler` with fn pointer instead.
- Scroll components moved from `components/scroll*` to `event/scroll/`.

### Fixed

- Long press not firing on desktop — was reading stale `ElapsedTime(0.0)`, now uses `App::clock` directly.
- Slider thumb offset — was reading declared `layout.left` instead of `ComputedRect` for screen position.

## [0.10.3] - 2026-05-14

### Fixed

- Dirty-region residue when widgets rotate and move on the same frame. `set_position` was overwriting the transform-aware `PrevRect` with a narrower axis-aligned rect; rotated corners leaked between frames. Now `set_position` unions over the existing `PrevRect` so the wider bbox survives.
- `collect_dirty_walk` stored `prev + curr` union back into `PrevRect`, causing the recorded bbox to grow without bound on moving widgets. It now stores curr-only; growth is bounded to one frame's delta.

### Added

- `Fixed::HALF` constant (replaces 18 occurrences of `Fixed::from_raw(128)`).
- `Rect::bounding_quad(&[Point; 4])` — deduplicates `quad_bbox` that was implemented twice (sw/quad.rs + render_system.rs).
- `Rect::union(&self, &Rect) -> Rect` — smallest containing rect of two inputs.
- Cha code-quality plugin (`api-misuse`) integrated into `cargo xtask ci`:
  - Upgraded to cha SDK v1.14.0 with tree-sitter AST query, file-role, and parsed comments.
  - Rules: `magic-fixed-half`, `magic-fixed-one`, `spec-id-leak` (error), `stale-naming`, `spelling-us`, `fixed64-hot-path`, `unimplemented-residue`, `viewport-scale-missing`, `chinese-comment`.
  - CI installs cha v1.14.0 via the official installer script.

## [0.10.2] - 2026-05-13

New `quad-aa` Cargo feature, on by default.

v0.10.1 left MCU targets with a 2×2 supersample that costs ~7 ms/frame vs the v0.9.2 binary fill on ESP32-C3 cover-flow (33 fps → 27 fps). That's the right trade-off for most MCU UIs, but not all — memory-tight builds, ultra-low-power modes, and anything that cares about raw frame rate more than edge quality now has an opt-out:

```toml
# Cargo.toml — keep binary fill, skip AA entirely
mirui = { version = "0.10.2", default-features = false, features = ["perf"] }
```

Without `quad-aa`, `fill_rect_quad` / `stroke_rect_quad` / `blit_quad` run the same hard-edge point-in-quad test as v0.9.x — corners still respect their disk, but edges are binary. ESP32-C3 cover-flow benchmark:

| config | ms/frame | fps |
|---|---|---|
| v0.9.2 baseline (no AA) | 23.5 | 42 |
| v0.10.1 / v0.10.2 with `quad-aa` (supersample) | 37 | 27 |
| **v0.10.2 without `quad-aa`** (binary) | **30** | **33** |

`std` builds with `quad-aa` still use the Fixed64 SDF for smooth coverage.

## [0.10.1] - 2026-05-13

Hotfix for the v0.10.0 quad AA regression on MCU targets. The shared Fixed64 signed-distance implementation that cover-flow edges rely on took ~2700 cycles per pixel on ESP32-C3 — cover-flow dropped from 42 fps (v0.9.2) to 10 fps. Unacceptable on any embedded target.

The fix splits the per-pixel coverage function by cfg:

- **`std` builds** keep the Fixed64 signed-distance field for smooth 256-step coverage. Desktop cover-flow stays at ~18 ms/frame (≈55 fps).
- **`no_std` builds** use a 2×2 supersample instead. Coverage quantises to `{0, 0.25, 0.5, 0.75, 1}`, but each sample test reduces to four integer adds plus a sign bit read per edge — no divides, no Fixed64 shim. ESP32-C3 cover-flow: back up to 26 fps (from the 10 fps regression), vs the 42 fps baseline of v0.9.2.

`PreparedEdge` now carries both sets of per-edge scratch (SDF path uses `inv_len` + `half_len_sq`, supersample path uses `qx` + `qy`) under cfg; the per-pixel entry point `quad_pixel_coverage_row` is a cfg alias that picks the right implementation. `EdgeRowState` is shared between both.

No API changes at the public surface — this is a behaviour fix.

## [0.10.0] - 2026-05-13

3D transforms finally look sharp. Two independent tracks landed together:

### Anti-aliasing for quad rasterization (software backend)

`DrawCommand.{Fill,Border,Blit}.quad` used to hard-clip pixel coverage — anything touching the quad edge became a binary in-or-out decision, so cover-flow cards and book-flip pages showed visible aliasing along the tilt. The software renderer now computes per-pixel coverage from a signed distance field:

- Each pixel's distance to the four quad edges is computed in Fixed64 (the Q24.8 precision that killed an earlier subpixel-AA attempt is gone), rounded corners are folded into the same SDF via each corner's wedge test, and the result is mapped linearly to a ±0.5 pixel coverage band.
- `fill_rect_quad`, `stroke_rect_quad` and `blit_quad` all route through the new sampler.
- `blend_pixel_int` was rewritten in plain u8 space to avoid the NormColor round-trip (eight Fixed divisions per call), and per-row pixel sweeps step `cx` by `Fixed::ONE` instead of rebuilding from `i32` each iteration.

Desktop cover-flow demo: 10 ms → 17.5 ms per frame, ~1.75× slower than the baseline but no more shimmering edges. ESP32-C3 measurement pending a board reconnect; the Fixed64 normalisation is the only per-pixel divide and may need further attention there.

### Real 3D quad rendering on the SDL GPU backend

The GPU backend used to `unimplemented!()` the moment render_system produced a pre-projected `DrawCommand.quad`, and silently mis-draw `Border.quad` by falling through to axis-aligned stroke. It now handles all three via `SDL_RenderGeometry`:

- `Path::rounded_quad(q, r)` — new constructor that builds a rounded polygon from any 4-vertex quad. Re-used by both backends' rounded-quad paths and friendly for Canvas-widget scenarios down the road.
- Fill and stroke tessellate the rounded quad path through the existing lyon pipeline and submit as a triangle mesh.
- Blit maps the source texture's UV corners to the quad's four vertices and lets `SDL_RenderGeometry` interpolate. Interpolation is affine — expect some foreshortening under very hard perspective tilt; the cover-flow range looks fine.
- 4× MSAA is requested on the GL context (with `SDL_RENDER_DRIVER=opengl` to force the driver on macOS where the Metal default would ignore it), so triangle edges antialias in hardware. Frame cost stays around 8 ms on M1 even with MSAA on — GPU headroom is plenty.

Desktop cover-flow demo on the SDL GPU backend: **122 fps** (vs 100 on the baseline software backend), and edges are sharp.

### Examples

- `cover_flow_demo` now picks the SDL GPU backend automatically when the `sdl-gpu` feature is enabled, falling back to SDL CPU otherwise. Run `cargo run --release --example cover_flow_demo --features sdl-gpu` to see the new GPU path.
- `cover_flow_demo` gained the `FpsSummaryPlugin` wire-up so it prints render timings at runtime.

## [0.9.2] - 2026-05-13

Documentation and example housekeeping after the v0.9 renames. No code changes, no API changes.

### Fixed

- `DrawCommand` module doc still described the v0.7 "always `Transform::IDENTITY`" invariant, which v0.8 broke. Rewrote it to match what the software backend actually handles today (2D affine + optional pre-projected `quad: [Point; 4]`).
- Broken rustdoc link in `src/draw/sdl_gpu/mod.rs`: `super::sdl::SdlSurface` still pointed at the pre-v0.9 layout where the module lived under `backend::`. Now points at `crate::surface::sdl::SdlSurface`.
- `README.md` referenced old trait / type names (`DrawBackend`, `SwDrawBackend`, `SdlBackend`, `FramebufBackend`, `mirui::backend::*`) throughout the quickstart and hybrid-backend sections, and pinned the `[dependencies]` example at `mirui = "0.5"`. All names updated, version bumped to `"0.9"`.

### Internal

- Registered `hello_sdl`, `layout_demo`, `widget_demo` under `[[example]]` with `required-features = ["sdl"]` so they no longer build spuriously without the SDL feature enabled.
- Removed four unused `use alloc::vec::Vec` imports from examples.
- Cleaned up stale doc comments in `src/draw/sw/blit_fast.rs` and `mirui-macros/src/compose.rs` that referred to work-in-progress state from earlier development rounds.

## [0.9.1] - 2026-05-13

Pure refactor of the `Canvas` implementations. No behaviour change, no user-facing API change. Both software and SDL_GPU renderers now follow the same file layout; adding a new renderer (wgpu / VG-Lite / ...) is now a predictable exercise.

### Changed

- **Software renderer** (`src/draw/sw/`) split further: `sw/mod.rs` shrank from 1564 to 1144 lines. Each `Canvas` method body moved into a dedicated submodule (`rect_fill.rs`, `rect_stroke.rs`, `blit_dispatch.rs`, `label.rs`, `path.rs`) as an inherent-impl `*_inner` method; the trait impl now holds one-line wrappers. `SwRenderer::draw` itself (20 KiB) was split into nine per-variant `dispatch_*` methods. The axis-aligned fast path in `fill_rect` extracted into its own helper. Also pulled `transformed.rs` (2D transformed fill/blit), deduped `build_inner_quad` against `build_corner_info`, and moved `encode_pixel` onto `ColorFormat::pack`.
- **SDL_GPU renderer** (`src/draw/sdl_gpu/`) mirrored the same split: `mod.rs` shrank from 602 to 439 lines; `rect_fill.rs`, `rect_stroke.rs`, `line.rs`, `blit.rs`, `path.rs`, `label.rs` each hold one method body.
- **`CornerInfo` refactor** (`src/draw/sw/quad.rs`): factored out `CornerShape { vertex, ua, ub }` so the inner quad's corners (which share the outer quad's unit vectors by construction) can be inset three times from a single shape computation instead of recomputing normalization per call. Renamed the internal `centre` field to `center` for consistency with the rest of the codebase. Inward-vector tuples become `Point` to match the vertex field.

### Internal

Tests unchanged (120 pass). ESP32-C3 cover-flow demo ROM footprint stays within ±400B of v0.9.0 (`mirui` crate `.text` ≈ 73.8 KiB). The refactor is motivated by source-reading ergonomics, not binary size.

## [0.9.0] - 2026-05-13

### ⚠️ Breaking: three renames to clarify the architecture

mirui has always had three concepts that share the word "backend": the **platform bridge** (window / framebuffer / input), the **low-level 2D primitives** (fill_rect / stroke / blit / label / ...), and the **per-frame DrawCommand consumer** (the thing `render_system` pushes commands to). They now have distinct names:

| role | old name | new name |
|---|---|---|
| platform bridge | `backend::Backend` trait, `SdlBackend`, `SdlGpuBackend`, `FramebufBackend` | `surface::Surface` trait, `SdlSurface`, `SdlGpuSurface`, `FramebufSurface` |
| 2D primitive sink | `draw::backend::DrawBackend` trait | `draw::canvas::Canvas` trait |
| frame renderer | `SwDrawBackend`, `SdlGpuRenderer` | `SwRenderer`, `SdlGpuRenderer` (unchanged) |

The module layout follows:

```
mirui::backend::*        → mirui::surface::*
mirui::draw::backend::*  → mirui::draw::canvas::*
```

The `compose_backend!` macro still exists under the old name (it composes _canvases_ now, but we're not renaming a macro just yet).

### Migration

Run this from your project root:

```sh
find src -name '*.rs' -exec perl -i -pe '
  s/\bmirui::backend\b/mirui::surface/g;
  s/\bdraw::backend\b/draw::canvas/g;
  s/\bSdlGpuBackend\b/SdlGpuSurface/g;
  s/\bSdlBackend\b/SdlSurface/g;
  s/\bFramebufBackend\b/FramebufSurface/g;
  s/\bSwDrawBackend\b/SwRenderer/g;
  s/\bSwDrawBackendFactory\b/SwRendererFactory/g;
  s/\bDrawBackend\b/Canvas/g;
  s/\bBackend\b/Surface/g;
' {} +
```

Double-check any hand-written `impl Backend for YourType` / `impl DrawBackend for YourType` — those pick up the new trait names, and `Canvas` in your own code is now shadowed by `mirui::draw::Canvas` if you re-exported it.

### Changed

- `sw_backend.rs` (2840 lines, since v0.8.1) split into `src/draw/sw/{mod,quad,blit_fast,perf}.rs`. mod.rs now holds the renderer struct + trait impls + tests; quad.rs the 3D scanline rasterizer; blit_fast.rs the per-format 1×/2× specializations; perf.rs the profiling counters. No behaviour change.
- `src/backend/sdl_gpu/` moved to `src/draw/sdl_gpu/`. `mirui::surface::sdl_gpu` remains as a re-export shim so `SdlGpuSurface` still lives under `surface::`.

## [0.8.5] - 2026-05-13

### Border renders under 3D perspective

`DrawCommand::Border` gained a `quad: Option<[Point; 4]>` field; when set, the software backend rasterizes the stroke as the difference of the outer rounded-rect scanline span and the inner one (outer quad shifted inward by `width`, inner radius `radius − width`). Covers framed cards in cover-flow-style layouts where the card is tilted.

### Added

- `TransformOrigin(x, y)` component — pivot for 2D / 3D transforms as fractions of the widget rect (`(0, 0)` = top-left, `(1, 1)` = bottom-right). Absent defaults to the widget centre, keeping the v0.8.x default. Book-flip effects (rotating around the spine) drop out of this.
- `WidgetBuilder::transform_origin(x, y)` convenience method.
- `examples/book_flip_demo.rs` — right page oscillates 0..120° around the spine with `TransformOrigin::new(0, 0.5)`.

### Fixed

- `stroke_rect` on the software backend applies the viewport scale like every other primitive does. Borders on retina / HiDPI (scale=2) setups previously drew at logical coordinates into the physical buffer, placing them at roughly the top-left quarter of where they should have been.
- `draw_label` scales its 8×8 glyph bitmap by the viewport scale. Pre-fix labels on retina rendered at half the intended size because each glyph pixel wrote one physical pixel regardless of scale.

### Changed

- `ui!` DSL recognises `border_width` as a separate attribute (previously only `border_color` was, which forced width to 1 px).

## [0.8.4] - 2026-05-13

### Perspective raster rewritten scanline-based

Quad fill and blit rasterizers used to be point-based: each pixel in the quad's bbox paid for a full `point_in_quad` check, and blit additionally did one `inverse.apply_point` per pixel. The new path finds the `[x_left, x_right]` span per scanline up front, then the inner loop writes pixels directly. ESP32-C3 cover_flow demo (5 perspective-tilted rounded cards + texture blit) goes from 46 ms/frame (22 fps) to 23 ms (43 fps), **2× speedup**.

### Added

- **`Fixed64`** — Q48.16 fixed-point built on `i64` raw. Sits next to `Fixed` (Q24.8) as the canonical higher-precision type for 3×3 homography matrix cells, pixel distance squared, and anywhere `Fixed` runs out of range or fractional resolution. `From<Fixed> for Fixed64` and `Fixed64::to_fixed()` handle lift/narrow.
- `Fixed64::mul_wide` / `div_wide` — i128-intermediate variants for callers that need ±2^47 headroom. The default `*` / `/` stay on i64 intermediates, matching what `Fixed` does, so they stay free on 32-bit targets.
- `draw::quad_perf` module — global counters for profiling the quad paths. `fill_ticks / blit_ticks` accumulate per-call timings, `fill_pixels_scanned / drawn` and `blit_pixels_scanned / drawn` track pixel-level work. Pointed at any monotonic clock via `quad_perf::CLOCK`. Off by default; enable with the `perf` crate feature.

### Changed

- `Transform3D` matrix cells are now `Fixed64` instead of raw `i64`. All constructors, `compose`, `apply_point`, `from_quad`, and `inverse` use `Fixed64` arithmetic. The previous file-local `q_mul` / `q_div` / `from_fixed` / `to_fixed` helpers are gone. No observable behaviour change, no size change on ESP32-C3.
- `fill_rect_quad` with `radius > 0` now scanlines: `quad_row_span` intersects the quad edges with each `y=py` horizontal, producing `[x_left, x_right]`; rows inside a corner's outward wedge then get clipped by the corner circle. Roughly 4× fewer cycles per drawn pixel versus the v0.8.3 `sdPolygon − r` point test.
- `blit_quad` uses scanline DDA: per-row setup precomputes the starting `(X, Y, W)`, then the inner loop does 3 Fixed64 adds to step along x and 1 divide + 2 multiplies to recover `(u, v)` (reciprocal-w trick). The old per-pixel `inverse.apply_point` (9 mul + 2 div) is gone.
- Fill fast path for opaque colour + RGB565 / ARGB8888 target skips `set_pixel`'s format match and writes the packed pixel bytes directly.

## [0.8.3] - 2026-05-12

### Rounded corners under 3D

`Fill` with `border_radius > 0` inside a 3D-transformed widget no longer panics; the quad path now renders the rounded-rect shape in screen space, so the corners stay round under perspective and the arcs line up with the straight edges even at steep tilts.

### Added

- Screen-space rounded-rect fill: `fill_rect_quad` insets the quad by `radius`, then checks each pixel against the inset polygon plus its four corner discs. Implementation follows Inigo Quilez's 2D SDF primitives (`sdPolygon` / `opRound`), reworked to short-circuit on inset membership so the per-pixel cost stays close to the v0.8.2 sharp-edge path.
- `examples/snapshot_cover_flow.rs` — headless renderer that sweeps `ScrollOffset` in 1/16-pixel steps and dumps per-step PPMs plus a pixel-level diff report. Used to chase sub-pixel flicker without asking a human to stare at the screen.

### Fixed

- `point_in_quad` now uses i64 cross products on raw `Fixed` values. The previous Q8.8 multiplication silently overflowed for widgets wider than ~180 px, flipping the cross-product sign and misclassifying points.

### Changed

- `cover_flow_demo` exercises a composite 3D transform (rotate_y + rotate_x + perspective) with rounded cards, so the demo actually stresses v0.8.3's rounded-corner quad path.

## [0.8.2] - 2026-05-12

### 🎠 Nested 2.5D

`WidgetTransform3D` now composes along the tree. A parent widget's 3D transform propagates into every descendant's render path, with 2D `WidgetTransform` descendants automatically lifted via `from_affine` so they inherit the parent's perspective. Covers cover-flow, card carousels, and any other "container tilts + children warp with it" effect in one go.

### Added

- `render_system::draw_tree` / `draw_tree_offset` / `collect_dirty_region` / `seed_prev_rects` thread a `parent_transform_3d` down the widget tree. The new `accumulate_3d` helper picks the right lift strategy at each level.
- `event::hit_test` walks a dedicated 3D chain via `compute_transforms_3d`, so rotated or perspective-warped nested widgets respond to touch in their transformed location.
- `examples/cover_flow_demo.rs` — horizontal carousel of five cards rendered with `rotate_y_perspective`, driven by `ScrollOffset` on the container (drag + inertia + elastic edges for free). Odd cards carry a nested `Image` widget to exercise the parent-child 3D path.

## [0.8.1] - 2026-05-12

### 🃏 2.5D Widget Warp

The `Transform` stub from v0.7.0 got filled in for 2D in v0.8.0 — now v0.8.1 adds the 3×3 homography path for 2.5D effects (card flip, iOS cover flow style tilt). The 2D path is unchanged; 3D widgets pay only for what they use.

### Added

- **`Transform3D`** (Q16.16 internal storage, 9 cells). Constructors: `IDENTITY`, `translate`, `scale`, `rotate_deg` (around the z-axis), `rotate_x_deg` / `rotate_y_deg` (parallel-projection variants), `perspective(d)` / `perspective_xy(dx, dy)`, and the combined `rotate_x_perspective` / `rotate_y_perspective` which produce the CSS-style "far edge shrinks into the distance" homography in one step (composing independent rotate + perspective doesn't match CSS, because the 2D matrix drops the z component — hence the combined constructor).
- **`WidgetTransform3D(Transform3D)`** component. Takes priority over `WidgetTransform` when both are attached.
- **`WidgetBuilder` chain methods**: `transform_3d`, `apply_transform_3d`, `rotate_x`, `rotate_y`, `rotate_x_perspective`, `rotate_y_perspective`, `perspective`.
- **`DrawCommand::Fill` / `DrawCommand::Blit`** gain `quad: Option<[Point; 4]>` — when `Some(q)`, the backend paints a quadrilateral instead of an axis-aligned rect. Direct-construction call sites (internal demos / tests) need to supply the field; the `None` path keeps existing behaviour.
- **`SwDrawBackend`** gains `fill_rect_quad` — iterates the quad's bbox, keeps pixels on one side of all four edges, writes the solid colour — plus `blit_quad` which solves a 4-point homography (Heckbert 1989) from the quad to the source rect and inverse-samples the texture per pixel. No divides in the hot inner loop for fill; blit only divides at the per-pixel `apply_point`.
- **`Transform3D::from_quad(src_rect, dst_quad)`** — recover a homography from four source-rect corners ↔ four destination-quad corners. Returns `None` on degenerate (collinear) quads.
- **`hit_test`** recognises `WidgetTransform3D` and tests the probe point against the projected quad.
- **`examples/flip_card_demo.rs`** — a solid-colour card rotating around the Y axis with perspective, swapping its bg colour when it crosses the 90°/270° plane so front and back stand out.
- **`examples/image_flip_demo.rs`** — same idea but with an `Image` widget, exercising the textured `blit_quad` path.

### Internal

- `types::transform_3d::point_in_quad` shared between the rasterizer and hit test.
- `render_system::quad_for` + `effective_transform_3d` emit quads as a one-shot per-entity computation; identity-only scenes don't call them.
- `render_system::seed_prev_rects` — called at the end of `App::render` so the first `render_dirty` frame knows which pixels the full render wrote; prevents residue stripes when a 3D widget shrinks (e.g. squash) between the initial full render and the first dirty pass.
- `collect_dirty_region` keeps a rolling union of current bbox + previous rect and stores the union back as the new `PrevRect`. When a widget shrinks, pixels it painted in previous frames are still in the next frame's dirty region and get overwritten by the root fill.
- `draw_tree` culls against the widget's projected quad bbox instead of its layout rect, so a rotated/translated 3D widget whose screen extent extends past the layout rect no longer gets early-skipped.

## [0.8.0] - 2026-05-12

### 🌀 Widget-level 2D Transforms

The `Transform` stub reserved in v0.7.0 is now live. Widgets can carry an arbitrary 2D affine — translate, rotate, scale, skew, or any composition — and the render tree accumulates them per-branch so ancestor transforms compose into descendant paint. Layout is untouched; the transform applies in the paint stage only, matching CSS and Flutter semantics.

Rotation pivots on the widget's centre by default (transform-origin = center), so `.rotate(30)` does what users expect without first translating.

### Added

- **`Transform::{translate, scale, rotate_deg, skew_deg, compose, apply_point, apply_rect_bbox, determinant, inverse, classify}`** — the full 2D affine API. `classify` returns a `TransformClass` (Identity / Translate / AxisAlignedScale / Rotate90 / General) so backends can fast-path common cases.
- **`WidgetTransform(pub Transform)`** component. Attach to any entity; absent means identity, pays zero cost.
- **`WidgetBuilder` chain API**: `.transform(t)`, `.apply_transform(t)`, `.rotate(deg)`, `.translate(tx, ty)`, `.scale_xy(sx, sy)`. `apply_transform` composes on top of the existing value so `.rotate(30).translate(10, 0)` reads left-to-right and applies right-first (CSS order).
- **`Viewport::as_transform`** — returns the scale-only `Transform` corresponding to the viewport's logical→physical mapping. Backends compose `viewport × widget_tf` once at entry and inverse-sample with the combined matrix.
- **`examples/transform_demo.rs`** — two spinning widgets (solid box + rotating icon) driven by a per-frame angle step.

### Changed

- `render_system`'s `draw_tree` / `draw_tree_offset` accumulate transforms top-down. Identity-only scenes (no `WidgetTransform` anywhere) hit the same fast paths as v0.7.1; the accumulation branch short-circuits on `is_identity`.
- `SwDrawBackend::draw` and `SdlGpuRenderer::draw` replace the previous `assert!(is_identity)` with a classify-and-dispatch step. Identity and Translate route through the existing raster paths with a pre-offset rect/point; anything else on SwDrawBackend lands on a general inverse-sampling rasterizer for `Fill` (radius=0) and `Blit`.
- `event::hit_test` walks the tree once to accumulate each entity's effective transform, then inverse-transforms the probe point before rect containment test. Rotated or scaled widgets hit correctly; singular matrices (scale 0) are unclickable.

### Performance

ESP32-C3 three-body, identity transform (no WidgetTransform attached): 5.0-5.7 ms / ~180 fps — matches v0.7.1's 5.1-5.3 ms within the noise band. Opt-in cost only: widgets without `WidgetTransform` don't pay the tree-accumulation math, and the classify step folds to a single equality against the IDENTITY constant.

## [0.7.1] - 2026-05-11

### ⚡ Faster `Blit` on CPU Backends

Addresses the HiDPI blit regression noted in the v0.7.0 CHANGELOG (scale = 2 costs ~3.6× more frame time because the dst pixel count is 4×). `SwDrawBackend::blit` gains three layered fast paths:

- **1× integer scale** (`dw == sw` and `dh == sh`) — dispatched per `(src.format, dst.format)` to a byte-level copy / format-convert that bypasses `get_pixel` / `set_pixel` bookkeeping. `RGB565Swapped → RGB565Swapped` is a `copy_from_slice` per row and is the main hot path for ESP32 builds that preload assets in framebuffer format.
- **2× integer scale** (`dw == 2*sw` and `dh == 2*sh`) — each src pixel splats its color to a 2×2 dst block; src is read once per block instead of four times. Same four format pairs specialized. Clip partially covering the dst rect on odd boundaries falls back to the DDA path.
- **Arbitrary non-integer scale** — replaces the per-pixel `(drow * sh) / dh` and `(dcol * sw) / dw` divides with a Q16.16 DDA accumulator. Two divides up front, zero in the inner loop. Dramatically helps any backend without hardware divide (RV32IMC).

Correctness is pinned down by three byte-for-byte comparison tests (`blit_dda_matches_generic_slow`, `blit_1to1_matches_generic_for_*`, `blit_2to2_565sw_matches_dda`) that run the new fast paths and the legacy slow path on identical inputs and compare dst buffers.

### Performance

ESP32-C3 three-body demo, 128×128 RGB565Swapped:

| config | v0.7.0 | v0.7.1 | speedup |
|---|---|---|---|
| scale=1 (3-body) | 5.9-6.5 ms | 4.9-5.3 ms | ~18% |
| scale=2 (3-body) | ~17 ms | 10.6-11.5 ms | ~35% |
| scale=0.5 (6-body) | 6.2-6.5 ms | 6.2-6.6 ms | flat |

scale=0.5 is dominated by SPI flush bandwidth, not blit sampling, so the DDA improvement there is invisible on this demo. scale=2 is the win that v0.7.0 regressed on.

### Added

- `mirui-examples` gains a `demo-hidpi-upscale` feature (commit `cc90089`) that runs the three-body demo at scale=2 — physical 128×128, logical 64×64 — for benchmarking the 2× fast path on hardware.

### Internal

- `SwDrawBackend::blit` now delegates to one of three free functions:
  - `blit_1to1_fast` — format-specialized 1× integer scale.
  - `blit_2to2_fast` — format-specialized 2× integer scale.
  - `blit_dda` — Q16.16 DDA for arbitrary non-integer scale.
- `blit_generic_slow` is kept as the `_` arm in the format-specialization matches (fallback for unsupported `(src, dst)` combinations like `RGB888 → RGB565`).
- No public API change. `DrawBackend::blit` signature identical to v0.7.0.

## [0.7.0] - 2026-05-11

### 🎯 The Logical-Coordinate Release

The render pipeline now flows end-to-end in **logical pixels**. Widgets, `Dimension::px`, layout trees, `DrawCommand`s and `DrawBackend` methods all speak logical. Each `DrawBackend` impl translates to physical on the way out. This lets HiDPI / subpixel downscale / any future `scale != 1` happen transparently — user code writes `Dimension::px(16)` and the backend rasters 16 logical × `scale` physical without the user knowing.

Every breaking change is trait-surface or struct-layout; end-user layout code (`WidgetBuilder`, `ui!`, `add_plugin`, `App::run`) is unchanged. `mirui-macros::compose_backend!` picks up the new `Blit` signature automatically.

### Breaking

- **`DrawCommand::Blit` gains `size: Point`** — the dst rect size is now explicit, and `SwDrawBackend::blit` / `SdlGpuRenderer::blit` scale the source texture to fit. Previously `Blit` always painted at the source's native dimensions, which meant a 16×16 icon widget at HiDPI scale=2 only occupied half its slot. Callers of `DrawBackend::blit` now receive a `dst_size: Point` argument.
- **Every `DrawCommand` variant carries a `transform: Transform`** — currently always `Transform::IDENTITY`, reserved for the upcoming widget-level 2D affine transform (rotate / scale / skew). Backend `Renderer::draw` entry-points `assert!(cmd.transform().is_identity())`; custom backends that match on `DrawCommand` exhaustively need to bind the new field.
- **`CoordTransform` → `Viewport`** — renamed to leave `Transform` free for the Layer-2 widget transform. Methods (`rect_to_physical`, `point_to_physical`, `logical_size`, `physical_size`, `scale`) keep the same shape.
- **`DisplayInfo.width` / `.height` now report logical pixels**, not physical. `Backend::physical_size() -> (u32, u32)` (new trait method, default impl computes from `display_info × scale`) gives drivers the physical buffer dims they need. Bundled backends override to return their internal dims directly.
- **`Backend::flush(area: &Rect)` is documented and enforced as physical-pixel coordinates.** `App::render` / `render_dirty` convert logical rects to physical via `Backend::physical_size()` and `Viewport::rect_to_physical` before calling `flush`, so drivers (ESP32 SPI, framebuffer) can treat `area` as raw device coordinates.
- `Backend::screen_rect()` default returns logical.
- `App::dirty_region()` returns logical pixels now (doc was incorrect before; implementation already was logical since v0.6.x).

### Added

- **`Transform` type** (currently identity-only stub) on `DrawCommand` — every draw op carries per-op transform metadata, backends assert identity at entry. Ready for Layer 2 widget transforms in a future release.
- **`Backend::physical_size() -> (u32, u32)`** trait method with a default derivation from `display_info()`. Bundled backends (`SdlBackend`, `SdlGpuBackend`, `FramebufBackend`) override to return their internal physical-size fields directly.
- **`FramebufBackend::with_scale(phys_w, phys_h, scale, cb)`** and `with_scale_and_format(..., scale, format, cb)` — opt-in HiDPI construction for embedded drivers. Lets the driver declare a `(physical, logical)` pair up front; user layout code continues to write `Dimension::px(logical_value)`.
- **`Viewport` is now a first-class public type**. `SwDrawBackend` and `SdlGpuRenderer` each hold one as a field; every method translates logical arguments internally.

### Fixed

- **Image widgets at HiDPI (scale > 1) now fill their slot.** Prior versions emitted `Blit` at the source texture's native dims, so a 16×16 icon at scale=2 painted 16 physical pixels (half of its 32-physical widget slot). Both SW and GPU paths now receive the widget size and nearest-sample / `canvas.copy`-stretch the source accordingly.
- **`border_width`, `radius`, scroll offsets, label padding scale once at HiDPI.** `widget::render_system::scale_rects` and the `scroll.x * scale` / `scroll.y * scale` workarounds in the scroll system are gone. Every `DrawCommand` is emitted in logical coordinates; the single `viewport.rect_to_physical` inside each backend method is the only scaling step.
- **HiDPI driver backends (`FramebufBackend`) now refresh the full physical surface each frame.** `App::render` / `render_dirty` previously flushed a logical-sized rect, which CPU framebuffer drivers interpreted as physical buffer offsets — at scale=2 only the top-left logical quadrant updated. Fixed by driving `Backend::flush` with physical coordinates throughout.

### Performance

- ESP32-C3 three-body demo: 5.7-6.5 ms/frame (~170 fps), matching v0.6.1 baseline within ~3% noise.
- HiDPI scale=2 on the same demo paints 4× more physical pixels per `Blit`; that translates to ~3.6× frame time, which is the expected HiDPI raster cost (same `fill_rect` speed via `copy_from_slice`; only `blit` gets expensive because of per-dst-pixel nearest sampling). The upcoming `sw-blit-fast-path` spec (v0.8 candidate) will recover most of this.
- Scale < 1 (e.g. scale=0.5 with logical 256×256 / physical 128×128 for a thumbnail-preview UI) is faster than scale=1 because `Blit` dst pixel count drops to 0.25×. No assumption of `scale ≥ 1` anywhere.

## [0.6.1] - 2026-05-11

### Fixed

- **`Dimension::Percent` overflow on windows wider than ~328 px**. `Fixed::div` now promotes through `i64`, so `parent_size * pct / 100` stays correct at any UI size. ESP32-C3 three-body baseline unchanged (~172 fps).
- **`SdlGpuBackend` + `App::run` black screen from the second frame.** `Backend` now reports its backbuffer behaviour via `persistence()`; `App::run` full-redraws every frame on `Transient` backends (swap-chain GPU) and keeps the `render_dirty` fast path on `Persistent` (CPU). Default is `Persistent`, so every existing CPU backend keeps working unchanged.
- **Labels and image blits no longer blur under HiDPI upscale.** `SdlGpuBackend` sets `SDL_RENDER_SCALE_QUALITY=0` at init — textures use nearest-neighbour filtering, which suits mirui's bitmap font and pixel-art assets.

### Added

- **`mirui::backend::BackbufferPersistence`** (`Persistent` / `Transient`) and a `Backend::persistence()` hook (default `Persistent`), used by `App::run` to pick render strategy per backend.
- `examples/sdl_gpu_demo.rs` gets a `DragPlugin` that moves an absolute-positioned box with the mouse — exercises the full `App::run` + plugin + `on_event` path on the GPU backend. The example also prints per-second wall-clock FPS with p50 / p99 / max frame time.

## [0.6.0] - 2026-05-11

### 🚀 The GPU Backend Release

mirui ships its first non-CPU-raster backend. `SdlGpuBackend` drives the SDL2 accelerated renderer (D3D / OpenGL / Metal, depending on platform) directly: solid fills go straight through `canvas.fill_rect`, complex paths are tessellated with `lyon` and submitted via `SDL_RenderGeometry`, text lives in a per-label `SdlTexture` cache, and textures are `canvas.copy`'d. Behind the new `sdl-gpu` feature; existing `sdl` / `no_std` / default builds are untouched.

On a standard 10 s benchmark scene (30 solid rects + 5 rounded rects with thick borders + 10 labels + 2 blits) on macOS, the GPU backend hits ~160 fps vs the CPU backend's ~119 fps — **~1.33× speedup** with substantially less work on the CPU.

### Added

- **`mirui::backend::sdl_gpu`** module (`sdl-gpu` feature):
  - `SdlGpuBackend` — GPU-backed window, no CPU framebuffer.
  - `SdlGpuFactory` — binds to `SdlGpuBackend` via `impl RendererFactory<SdlGpuBackend>`.
  - `SdlGpuRenderer` — DrawBackend impl covering `Fill` / `Border` / `Line` / `Blit` / `Label` / `Arc` / path fill + stroke through a hybrid fast-path + tessellation strategy.
- **`SdlGpuBackend::new` / `new_with_vsync`** — Vsync-off variant for benchmarking.
- **`SdlBackend::new_with_vsync`** — Same on the CPU backend for consistency.
- **Path tessellation via `lyon` 1.0** (feature-gated): `FillTessellator` / `StrokeTessellator` reuse inside a `TessellationCache`, so complex paths amortise to zero per-frame allocation after warm-up.
- **Per-label GPU texture cache** (keyed by text hash + colour, LRU-bounded to 128 entries) backs the `draw_label` hot path; miss triggers a CPU raster + streaming upload, hits are a single `canvas.copy`.
- **`examples/sdl_gpu_demo.rs`** — visual demo exercising every fast-path.
- **`examples/perf_bench.rs`** — standard scene + 10 s timed run, works against either backend depending on which feature is enabled.
- New optional dependencies (only under `sdl-gpu`): `lyon 1.0`, `lru 0.12`, `sdl2-sys 0.37`.

### Changed

- Lyon fill/stroke tolerance is 1.0 (previous SDL-GPU internal draft used 0.25). Sub-pixel accuracy isn't visible on UI elements; 1.0 buys ~40% tessellation time back.

## [0.5.2] - 2026-05-10

### 🧱 Trait Architecture Refactor (GPU-Ready Prep)

Groundwork for v0.6.0's SDL GPU backend — `Backend` no longer assumes a CPU framebuffer, and `RendererFactory` is parameterised over the backend type so GPU backends can reach into backend-specific resources (Canvas / device / context). CPU backends implement a new `FramebufferAccess` sub-trait. ESP32-C3 three-body holds at ~5.5-6.0 ms/frame — no regression.

### Added

- **`FramebufferAccess: Backend`** sub-trait (`mirui::backend`) — CPU-raster backends implement it and return `Texture<'_>` (metadata + buffer bundled). Consumed by `SwDrawBackendFactory` via blanket impl. GPU-only backends (future SDL GPU / wgpu / VG-Lite) skip it.

### Changed (⚠️ Breaking)

- **`Backend::framebuffer() -> &mut [u8]` removed**. CPU backends now implement `FramebufferAccess::framebuffer() -> Texture<'_>` instead. The bundled `SdlBackend` and `FramebufBackend` have migrated. Custom CPU backends need to split `impl Backend + impl FramebufferAccess`.
- **`RendererFactory` gains a `<B: Backend>` generic parameter**: `fn make(&mut self, backend: &mut B, transform: &CoordTransform)`. `SwDrawBackendFactory` now provides a `impl<B: FramebufferAccess> RendererFactory<B>` blanket, so any CPU backend "just works" with the default factory. GPU factories bind to their concrete `B` (e.g. `impl RendererFactory<SdlGpuBackend> for SdlGpuFactory`).
- **`Plugin<B, F>` bound tightened** to `F: RendererFactory<B>`. Custom plugin impls need one extra where clause: `where B: Backend, F: RendererFactory<B>`.
- **`App::new(backend)` requires `B: FramebufferAccess`** (only the default `SwDrawBackendFactory` needs CPU buffer access). `App::with_factory` remains open to any `B: Backend` + `F: RendererFactory<B>`.
- Generic demo fns on `mirui-examples` that used `App<impl Backend>` need to switch to `App<impl FramebufferAccess>` (same change any downstream CPU app will face).

## [0.5.1] - 2026-05-10

### 🧹 CoordTransform Follow-up

Finishes what 0.5.0 started — the `RendererFactory::make` signature still took a raw `Fixed` scale, and the event loop rebuilt a transform per event. Both gone. ESP32-C3 three-body demo holds at ~5.5-6.0 ms/frame (≈173 fps) on-device, matching the 0.5.0 baseline.

### Changed (⚠️ Breaking)

- **`RendererFactory::make(tex, scale: Fixed)` → `make(tex, transform: &CoordTransform)`**. Anyone implementing a custom `RendererFactory` (including `compose_backend!` factories) grabs `scale` via `transform.scale()`. Default `SwDrawBackendFactory` handled internally.

### Changed

- `CoordTransform` hot methods marked `#[inline]` (`scale`, `logical_size`, `physical_size`, `point_to_physical`, `rect_to_physical`, `rect_to_physical_pixel_bounds`, `point_to_logical`, `new`) plus `DisplayInfo::transform`. Release LTO already inlined most of them; the annotation pins the contract.
- `App::run` event-drain loop now snapshots `logical_size` once per iteration instead of reconstructing the transform per input event. Every event in a drain sees the same logical size — single source of truth.

## [0.5.0] - 2026-05-10

### 🗺️ The CoordTransform Release

Logical ↔ physical pixel conversion now lives in one type. The render pipeline used to carry `(screen_w, screen_h, scale)` triples around and inline `Fixed::from(w) / scale` divisions; every one of those collapses into a single `&CoordTransform` parameter.

### Added

- **`mirui::types::CoordTransform`** — holds physical size + scale, offers `logical_size()`, `point_to_physical()`, `rect_to_physical()`, `rect_to_physical_pixel_bounds()`, `point_to_logical()`. Constructor normalises `scale <= 0` to `1` so downstream consumers stop duplicating the guard.
- **`DisplayInfo::transform()`** — one-liner builder for the transform.

### Changed (⚠️ Breaking)

- `render_system::{render, render_region, update_layout, collect_dirty_region}` now take `transform: &CoordTransform` in place of `(screen_w: u16, screen_h: u16, scale: Fixed)`. Callers writing their own render loop need to switch to `CoordTransform::new(width, height, scale)` or `info.transform()`. `App::run` users are unaffected.

## [0.4.0] - 2026-05-10

### 🧩 The Plugin Release

`App` now accepts **plugins** — self-contained bundles of systems, resources, and lifecycle hooks. The previous contract of "subclass the run loop to get per-frame timing" is dead: `app.add_plugin(StdInstantClockPlugin).add_plugin(FpsSummaryPlugin).run()` is the whole story. The ESP32-C3 three-body demo still holds ~160 fps through the new path.

### Added

- **`Plugin<B, F>` trait** in `mirui::plugin`, with five methods:
  - `build(&mut self, app: &mut App<B, F>)` — one-shot registration
  - `pre_render(&mut self, world)` / `post_render(&mut self, world, render_nanos)` — per-frame hooks
  - `on_event(&mut self, world, event) -> bool` — intercept input, `true` consumes
  - `on_quit(&mut self, world)` — cleanup before `App::run` returns
  - blanket impl for `FnMut(&mut App<B, F>)` so a closure is already a plugin
- **`App::add_plugin<P>(p) -> &mut Self`** — registers a plugin, runs its `build`, stores the instance for later hooks. Chains with `add_system`.
- **`App::clock: ClockFn`** — monotonic clock providing the nanoseconds passed to `post_render`. Default `|| 0`; plugins swap it in `build`.
- **`mirui::plugins` module** with two built-ins:
  - `StdInstantClockPlugin` (gated on the new `std` feature) — `std::time::Instant`-backed clock
  - `FpsSummaryPlugin` — accumulates `render_nanos` over a frame bucket (default 60) and emits an "avg render" line; `with_sink` lets the sink be overridden for bare-metal targets
- **`std` feature flag** (implied by `sdl`). `no_std` + `alloc` remains the default build; anything in `mirui::plugins::std_clock` or other std-only items sits behind this feature.

### Changed

- **`App` gains a generic + field for plugin storage**: the run loop now dispatches `pre_render` / `post_render` / `on_event` / `on_quit` around the existing rendering and event code. Apps that never call `add_plugin` see empty vector iteration — identical to the previous behaviour in practice.
- `add_system` now returns `&mut Self` to chain with `add_plugin`.

## [0.3.1] - 2026-05-10

### Added

- **`compose_backend!` macro** (`mirui-macros`) — build a hybrid `DrawBackend` by routing each method to a chosen field:
  ```rust
  compose_backend! {
      pub struct Hybrid {
          sw: SwDrawBackend,
          gpu: GpuBackend,
      }
      route {
          default => sw,
          blit => gpu,
          clear => gpu,
      }
  }
  ```
  Generated struct is generic over one type parameter per field, implements both `DrawBackend` and `Renderer`, and routes through the chosen field at compile time (no runtime dispatch). Unrouted methods fall back to the `default` field; unrouted default-impl methods (`fill_rect` / `stroke_rect` / `draw_line` / `draw_arc`) fall through to the `DrawBackend` trait default.
- **`RendererFactory` trait** (`mirui::app`) + **`SwDrawBackendFactory`** — let `App` build a fresh `Renderer` each frame from the framebuffer instead of hard-coding `SwDrawBackend`. `App::with_factory(backend, factory)` is the new explicit constructor; `App::new(backend)` keeps the default behaviour.
- **`App` is now generic over the factory** (`App<B, F = SwDrawBackendFactory>`). Existing `App::new(backend)` call sites compile unchanged.
- Diagnostic Levenshtein "did you mean" hints for unknown method / field names in `compose_backend!` routes.
- Two SDL examples exercising `compose_backend!`:
  - `compose_backend_demo` — direct `DrawBackend` usage with a `Logging` wrapper
  - `compose_backend_dsl` — full `ui!` + ECS + `App::with_factory` pipeline, drifting images routed through the logging field

### Changed

- `App<B>` type signature becomes `App<B, F = SwDrawBackendFactory>`. Default value means `App::new(backend)` stays source-compatible; callers that spelled out the type (e.g. `fn use_app(app: &mut App<SdlBackend>)`) continue to work via the default too. Generic bounds that added `where` clauses on `App<B>` specifically are unaffected.
- Painter now forwards every DrawBackend primitive (`draw_text` / `fill_path` / `stroke_path` / `draw_line` / `draw_arc` in addition to the earlier four).

### Fixed

- `stroke_path`: reversed the outer ring winding so the even-odd fill rule correctly carves `outer_area ∖ inner_area`. Stroked triangles and rectangles now render as continuous outlines instead of the broken-up look the earlier winding produced.

## [0.3.0] - 2026-05-10

### 🎨 The DrawBackend Release

`DrawBackend` is now a real rendering surface. `fill_path` and `stroke_path` actually work (with scanline coverage anti-aliasing), `draw_line` / `draw_arc` exist, and `rounded_rect` corners are actually round. The ESP32-C3 three-body demo holds 160 fps with correct corner AA that used to silently skip; new shapes and butterfly demos render vector graphics at 30-35 fps.

### Added

- **`Path::bbox()`** — conservative bounding box including Bezier control points
- **`Path::arc(center, radius, start, end)`** — builds an arc path using cubic Bezier segments (≤90° each, `k = 4/3 · tan(θ/4)`). Angles in degrees, CCW
- **`Fixed::sin_deg` / `Fixed::cos_deg`** — Taylor 6-term approximation, error < 0.01
- **`Fixed::{MAX, MIN, PI}`** constants + **`Point::ZERO`** constant
- **`fill_path`** on `SwDrawBackend` — scanline rasterizer with 4 sub-scanlines per row, even-odd fill, Fixed-space coverage integration. Diagonal edges render cleanly without combing
- **`stroke_path`** on `SwDrawBackend` — offset polygon with miter join (miter_limit = 4, bevel fallback), butt caps for open paths. Outer ring winding is reversed relative to inner so even-odd carves `outer ∖ inner` correctly
- **`DrawBackend::draw_line`** / **`draw_arc`** — trait default implementations routing through `stroke_path`
- `DrawCommand::Line` / `Arc` are now handled by `Renderer::draw` (previously silently dropped)
- `rounded_rect` corners now use cubic Bezier (`k = 4/3 · tan(22.5°) ≈ 0.5523`), reducing arc approximation error from ~27% of radius to ~0.03%
- Visual snapshot tests under `tests/visual_fill_path.rs` (`#[ignore]`-gated, manual run via `cargo test -- --ignored`)

### Fixed

- **`Fixed::sqrt`** — previously returned `sqrt(raw)` instead of `sqrt(raw << 8)`, off by a factor of 16 in Fixed space. `rounded_rect_coverage` was accidentally masking it because the buggy `dist - r` value always exceeded 1 and took the "no AA" branch. Corner anti-aliasing now actually functions

### Changed (⚠️ Breaking)

- **`DrawBackend` trait** gained `draw_label(&mut self, pos, text, clip, color, opa)` as a required method. Previously `draw_label` was only defined on `SwDrawBackend` directly. External implementers of `DrawBackend` must now provide a `draw_label` implementation; there is no default
- **`DrawCommand::Line` / `Arc`** fields migrated from `u16` to `Fixed` (`width`, `radius`, `start_angle`, `end_angle`), aligning with the rest of the pipeline. No known external emitters

### Performance

- ESP32-C3 three-body demo: 170 → 160 fps (-6%). The regression is a direct consequence of the `Fixed::sqrt` fix: `rounded_rect_coverage` now actually performs the per-edge AA ramp it was designed to, instead of silently taking the short-circuit branch
- New scanline rasterizer is substantially faster than the previous "per-pixel distance + sqrt" approach: shapes demo 1 fps → 35 fps (small circle) after introducing Chebyshev AABB rejection + coverage integration

## [0.2.0] - 2026-05-09

### 🎉 The Subpixel Release

mirui now renders with **24.8 fixed-point precision** across the entire pipeline — from layout to rendering to event handling. Every coordinate, every rect, every scroll offset lives in subpixel space. Anti-aliased edges come for free. And somehow, the binary got **11% smaller**.

### Added

- **`Fixed` type** — 24.8 fixed-point arithmetic with `Add`/`Sub`/`Mul`/`Div`/`Neg`, `ceil()`/`floor()`/`round()`/`sqrt()`/`abs()`, `From<i32>`/`From<u16>`/`From<u32>`/`From<f32>`
- **`Dimension` enum** — `Px(Fixed)` / `Percent(Fixed)` / `Auto` / `Content` with `resolve(parent_size)` and arithmetic ops
- **Subpixel anti-aliased rendering** — rect edges and rounded corners use coverage-based alpha blending
- **`rounded_rect_coverage()`** — replaces boolean hit test with smooth 1px falloff
- **Fast path** — integer-aligned rects with no radius skip coverage calculation entirely (zero overhead)
- **`Rect::new(x, y, w, h)`** — accepts `impl Into<Fixed>`, write `Rect::new(0, 0, 480, 320)` directly
- **`Fixed::is_integer()`** / **`Rect::is_aligned()`** — query alignment without touching raw bits
- **`Dimension::px()`** / **`Dimension::percent()`** — const constructors
- **`set_position(world, entity, x, y)`** — now accepts `impl Into<Fixed>`, pass integers or Fixed seamlessly
- **xrune-fmt CI integration** — `cargo xtask ci` checks DSL formatting

### Changed (⚠️ Breaking)

- `Rect` fields: `i32`/`u16` → `Fixed`
- `Point` fields: `i32` → `Fixed`
- `LayoutStyle.width/height/left/top`: `Option<u16>`/`Option<i32>` → `Dimension`
- `LayoutStyle.grow`: `f32` → `Fixed`
- `LayoutStyle.padding`: `u16` → `Dimension`
- `InputEvent::Touch/TouchMove/Release` coordinates: `i32` → `Fixed`
- `ScrollOffset` fields: `i32` → `Fixed`
- `DisplayInfo.scale`: `u16` → `Fixed` (now supports fractional scales like 1.5x)
- `Style.border_width/border_radius`: `u16` → `Fixed`
- `ScrollConfig.content_height/content_width`: `u16` → `Fixed`
- `compute_layout()` signature: all params now `Fixed`
- `app.run()` now uses `render_dirty()` instead of full `render()` per frame

### Performance

- ESP32-C3 binary size: **42,740B → 37,976B (-11%)** — eliminated soft-float `__mulsf3`/`__divsf3`
- Zero-cost for integer-aligned widgets (fast path bypasses coverage math)
- RISC-V disassembly confirms: `Dimension::resolve()` fully inlined, Fixed mul = single `mul` instruction

## [0.1.6] - 2026-05-08

### Added

- Query API — `World::query::<T>().and::<U>().without::<V>().collect_into(&mut buf)`
- Enchants — DSL `[expr, ...]` syntax for attaching arbitrary components
- `WidgetBuilder::image()` + DSL `image:` attribute
- ScrollView — `ScrollOffset` + `ScrollConfig` components
- Scroll drag interaction with vsync
- Inertia scrolling (velocity decay)
- Elastic bounce (snap back to boundary)
- Scroll chaining (direction-aware, boundary check at resolve time)
- Elastic resistance (spring damping on overscroll drag)
- `ComputedRect` — persist layout results to entities
- `InputEvent::TouchMove`
- Nested scroll demo
- Full README rewrite

## [0.1.5] - 2026-05-08

### Added

- HiDPI support — scale factor for SDL backend, font + image scaling
- Dirty flag system — component-level partial refresh tracking
- `render_region` — only redraw widgets intersecting dirty rect
- Absolute positioning — `Position::Absolute` + `left`/`top`
- System scheduler — `add_system` + `add_fn` (closure support)
- World resource API — `insert_resource`/`resource`/`resource_mut`
- `set_position` — automatic old+new dirty tracking with PrevRect
- `Backend::flush(area)` — partial flush with region
- `App::render_dirty` — automatic dirty rect detection in run loop
- `DeltaTime`/`ElapsedTime` resources
- DSL: `position`/`left`/`top` attributes
- `Padding::all()` convenience constructor

### Performance

- ESP32-C3 partial refresh: 160fps (vs 60fps full-screen)

## [0.1.4] - 2026-05-07

### Added

- `walk` iteration support in DSL — dynamic widget generation
- `if` conditional rendering in DSL
- Compile-time error on unknown widget attributes
- Components: Button (pressed state), ProgressBar (click-to-set), Checkbox (toggle), Image (RGBA blit)
- Built-in asset: thumbs-up image (16x16 RGBA)
- `DrawCommand::Blit` — image rendering with alpha blending
- `button_system` — automatic interaction for Button/Checkbox/ProgressBar
- `Padding::all()` convenience constructor
- `ui!` macro now returns top-level widget Entity
- components_demo, walk_demo, image_demo examples

## [0.1.3] - 2026-05-07

### Added

- `mirui-macros` crate: declarative UI DSL powered by xrune
- `ui!` macro with `parent` + `world` context — zero-cost abstraction
- Post-order codegen: children created before parent, solves `&mut World` borrow
- Component-style architecture: each function = a UI component
- dsl_demo example showcasing composable UI functions

## [0.1.2] - 2026-05-07

### Added

- Rounded rectangle rendering (border_radius support)
- Border rendering with rounded corners
- 8x8 bitmap font (ASCII 32-126)
- Text rendering: DrawCommand::Label, .text(), .text_color()
- WidgetBuilder: .border(), .border_radius(), .text(), .text_color()
- Event system: hit test, dispatch, EventHandler callback
- click_demo and rounded_demo examples

## [0.1.1] - 2026-05-06

### Added

- Backend trait: pluggable platform abstraction (display + input)
- SdlBackend: SDL2 window + texture + event pump (feature `sdl`)
- FramebufBackend: memory buffer + flush callback (no_std friendly)
- App struct: unified entry point with render + run loop
- `cargo xtask release` now creates GitHub Release with changelog notes
- InputEvent enum: Touch / Release / Key / Quit

## [0.1.0] - 2026-05-06

### Added

- ECS core: Entity (generational index), SparseSet, World, System trait
- Draw module: Rect, Point, Color, DrawCommand enum, Renderer trait, SwRenderer (fill + alpha blending)
- Layout engine: Flexbox (direction, justify, align, grow, padding)
- Widget system: Style component, WidgetBuilder, RenderSystem (ECS → Layout → Draw pipeline)
- Release profile: opt-level z, LTO, strip, panic=abort (~5KB on RISC-V)
- xtask: ci/build/test/lint/size/bump/publish/release
- SDL2 examples: hello_sdl, layout_demo, widget_demo
