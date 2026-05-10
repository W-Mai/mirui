# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
