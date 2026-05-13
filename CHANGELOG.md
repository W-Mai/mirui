# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
