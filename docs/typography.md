# Typography

mirui shapes text into positioned glyphs, then optionally places those glyphs on a registered vector path. The same `Path` and `PathId` types drive drawing, clipping, hit testing, and text baselines.

<MiruiSnippet recipe="curve" label="TextPath" file="recipes.rs" height="230px">

<<< @/examples/first-app/src/recipes.rs#recipe-curve{rust}

<template #exercise>

Tap `Slide +18` to advance the leading offset along the Bézier curve, or tap `Flip wave` to switch the reactive `TextPath` between crest and valley paths.

</template>

</MiruiSnippet>

<div class="arch-diagram" role="img" aria-label="Typography pipeline from FontStack and shaping to linear or path glyph placement">
  <svg viewBox="0 0 940 168" xmlns="http://www.w3.org/2000/svg">
    <defs>
      <marker id="typo-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
        <path d="M0 0L8 4L0 8Z" fill="var(--site-blue-ink)" />
      </marker>
    </defs>
    <rect x="16" y="20" width="208" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="30" y="46" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="10.5" font-weight="600">01 · FONT STACK</text>
    <text x="30" y="70" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="13.2" font-weight="620">Primary + FALLBACK</text>
    <text x="30" y="94" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">FontToken::Default</text>
    <text x="30" y="114" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">FontToken::FALLBACK</text>
    <text x="30" y="134" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="10.5">MIRX A1 / A8 / SDF strikes</text>
    <path d="M224 84H244" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#typo-arrow)" />
    <rect x="250" y="20" width="208" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="264" y="46" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="10.5" font-weight="600">02 · SHAPING</text>
    <text x="264" y="70" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="13.2" font-weight="620">Bidi &amp; Line Breaking</text>
    <text x="264" y="94" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">TextWrap · Ellipsis</text>
    <text x="264" y="114" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">TextLayoutCapacity</text>
    <text x="264" y="134" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="10.5">Bounded workspace buffers</text>
    <path d="M458 84H478" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#typo-arrow)" />
    <rect x="484" y="20" width="208" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="498" y="46" fill="var(--site-blue-ink)" font-family="var(--font-mono)" font-size="10.5" font-weight="600">03 · PLACEMENT</text>
    <text x="498" y="70" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="13.2" font-weight="620">Box or Bézier Path</text>
    <text x="498" y="94" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">TextPath::with_offset</text>
    <text x="498" y="114" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">Outputs&lt;TextGeometry&gt;</text>
    <text x="498" y="134" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="10.5">Tangent frames &amp; carets</text>
    <path d="M692 84H712" stroke="var(--site-blue-ink)" stroke-width="1.5" marker-end="url(#typo-arrow)" />
    <rect x="718" y="20" width="206" height="128" rx="12" fill="var(--site-panel)" stroke="var(--site-rule-strong)" />
    <text x="732" y="46" fill="var(--logo-see)" font-family="var(--font-mono)" font-size="10.5" font-weight="600">04 · RASTER</text>
    <text x="732" y="70" fill="var(--site-ink)" font-family="var(--font-ui)" font-size="13.2" font-weight="620">Glyph &amp; Hit Test</text>
    <text x="732" y="94" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">Selection runs</text>
    <text x="732" y="114" fill="var(--site-muted)" font-family="var(--font-mono)" font-size="10.5">Oriented carets</text>
    <text x="732" y="134" fill="var(--site-faint)" font-family="var(--font-ui)" font-size="10.5">SW / GPU / Web</text>
  </svg>
</div>

## Default font and providers

`Font::default()` opens the embedded ASCII MIRX face at 8 px. Its A1 atlas is borrowed from program storage. `font_size` scales both metrics and drawing. Characters outside ASCII use the face's question-mark glyph unless a font stack supplies coverage. Applications that require more characters can register another MIRX font through `App::with_fonts`.

`font`, `font_stack`, `font_size`, and `text_color` are inherited along the UI parent chain. Each field uses the nearest declaration. The root uses the default font stack, the selected font's native size, and `ColorToken::OnSurface`. Specify `font_size: FontSize::Native` to use the selected font's native size within a subtree that inherits a different size. Paragraph layout is supplied on each text widget. Custom paint callbacks read the effective values through `paint.typography()`.

Every font uses `FontProvider`, including the embedded face. A custom provider is shared through the `provider: Rc<dyn FontProvider>` field of `Font`; cloning a font shares that provider. Software and accelerated renderers consume the same raster regions and coverage metadata. The software renderer reads exact-size A1 coverage directly and resamples scaled coverage.

## Text box overflow

LF, CR and CRLF separate paragraph lines without requesting font coverage or glyphs. Leading, consecutive and trailing separators retain empty lines and their caret positions. Text, selection and clipboard offsets refer to the original UTF-8 bytes; CRLF has no caret between its two bytes. `TextEditing` keeps its single-line appearance and crosses a CRLF pair together during horizontal navigation and deletion.

Assign a finite width when text should wrap or show an ellipsis. `Ellipsis` also requires a font in the stack that maps U+2026. Explicitly sized axes clip ordinary `Text` to the transformed text box for both overflow policies; `Auto` and `Content` preserve glyph ink overhang within the ancestor clip. Path text uses its placed glyph geometry and ancestor clip because its ink can extend beyond the layout box.

<MiruiSnippet recipe="typo-ellipsis" label="TextOverflow::Ellipsis" file="recipes.rs" height="190px">

<template #exercise>

Click `Toggle wrap` to switch between single-line `NoWrap` with `TextOverflow::Ellipsis` and two-line `TextWrap::Word`.

</template>

</MiruiSnippet>

## Layout failure reporting

`App::render()` and `App::render_dirty()` return `RenderError::TextLayout` when text preparation fails, including `TextInput` content and placeholders. Drawing, scrolling, and framebuffer flush are skipped for that frame. `App::last_text_layout_failure()` identifies the text entity, preparation stage, and available capacity details; a successful layout clears the diagnostic.

`TextEditing` prepares its single-line layout after its final size is known. Glyphs and the cursor read that layout during painting; input edits, placeholder changes, font replacement through `App::with_fonts`, and cache resets prepare it again. Empty inputs keep the cursor at the start, and long content scrolls horizontally to keep the cursor visible.

Low-level callers use `render_system::try_update_layout` to receive layout failures and stop before drawing. `collect_dirty_region` and `collect_dirty_regions` also return `Result` with `TextLayoutFailure`; `App::dirty_region()` maps failures to `RenderError::TextLayout`. An empty successful query is distinct from a failed layout. A failed pass also records a `TextLayoutFailure` World resource and invalidates cached geometry for retry, but does not restore text-cache entries already retired during preparation.

`App` records painted bounds after successful full and partial renders. Low-level render loops call `render_system::seed_prev_rects` after drawing successfully and before further state changes; collecting damage does not record a frame as painted.

## Bound dynamic content

`capacity` reserves UTF-8 content storage for a `Text` or `Button` label. A reactive expression can format directly into a stack buffer and copy a complete value into that storage without allocating during the content update:

```rust
#[compose]
fn reading_label(reading: Signal<u16>) {
    ui! {
        Text(text: ${ format_args!("{:03}", reading.get()) }, capacity: 4)
    };
}
```

The capacity counts UTF-8 bytes, not characters. A value that exceeds it or fails to format leaves the previous valid content intact; the `Text` component records a `TextContentError`. If the first value fails, `App::render()` and `App::render_dirty()` return `RenderError::TextContent` before drawing or flushing. `App::last_text_content_failure()` identifies the entity and error, prioritizing labels without a valid first value. A later valid update clears that diagnostic. Runtime overflow after a valid value does not block rendering of the rest of the UI.

The bounded write covers storage and formatting performed by `Text`; an expression such as `format!(...)`, `.to_string()`, or a cloned `Signal<String>` can allocate before the write. Use `format_args!` over borrowed or scalar values when the update itself must not allocate. Text shaping, layout, font caches, and backend resources have separate capacity requirements. Bounded content is a plain-text value; use the existing localized-text path when the label must follow locale changes.

## Reserve bounded layout storage

`App::with_text_layout_capacity` prepares text-cache storage before the first layout pass. Set `TextLayoutLimits` first when the default byte budget is unsuitable. The returned error leaves the previous cache installed; calls after text layout has started return `TextLayoutError::InUse`. `App::try_with_text_layout_limits` can also change limits after reservation and before layout while preserving the declared capacity; an incompatible limit returns an error without replacing the cache. The existing fluent `with_text_layout_limits` reports that error by panicking rather than silently removing the bound.

```rust
use mirui::text::{TextLayoutCapacity, WorkspaceCapacity};

app.with_text_layout_capacity(TextLayoutCapacity {
    layout_slots: 4,
    measurements: 4,
    lines: 16,
    runs: 32,
    glyphs: 64,
    carets: 64,
    workspace: WorkspaceCapacity {
        runs: 16,
        glyphs: 32,
        scratch_glyphs: 32,
        lines: 16,
    },
})?;
```

`layout_slots` covers retained layout handles and their generation storage. The line, run, glyph, and caret capacities cover all retained paragraphs plus one paragraph's temporary output; they are aggregate counts, unlike the per-paragraph limits in `TextLayoutLimits`. `workspace` bounds private shaping buffers. The reservation checks actual retained vector capacities and the workspace against `TextLayoutLimits::cache_bytes`. Later requests above the declared capacities fail explicitly instead of growing those buffers. Other renderer and font caches have separate storage.

A reusable UI module can declare its own `module_text_capacity: TextLayoutCapacity` with `App::require_text_layout_capacity` during setup. Requirements for layout slots, measurements, lines, runs, glyphs, and carets add together; shaping-buffer requirements take the maximum because one scratch area is reused. After all modules are composed, call `App::prepare_text_layout` before the first layout pass:

```rust
app.require_text_layout_capacity(module_text_capacity)?;
// Compose the remaining modules before preparing the shared cache.
app.prepare_text_layout()?;
```

Registration does not replace the cache. Preparation checks the combined request against the active byte budget and leaves the previous cache intact on failure. Calls after layout starts and mixing additive requirements with `with_text_layout_capacity` return an error rather than discarding another module's reservation. `App::render()` and `App::render_dirty()` also prepare a pending requirement before layout; a failure returns `RenderError::TextPreparation`, with its cause available through `App::last_text_layout_preparation_error()`. Applications that control startup should call `prepare_text_layout` explicitly to report failures before entering the frame loop. Without an opt-in requirement, text layout keeps its existing on-demand behavior.

On Web Canvas, a demo can provide an untransformed, 1× per-run ink-extent hint during setup:

```rust
app.prefer_text_raster_scratch(256, 8)?;
```

`App` takes the maximum width and height across hints and prepares raster slots before each frame using the current viewport scale. It provisions retained slots from the number of constructed bounded `Text` widgets; each slot retains its RGBA buffer, offscreen canvas, image data, and pixel views. A slot belongs to one text run: unchanged content reuses its raster, while changed content redraws into the same slot without displacing another run. The slots share one software raster scratch. One generation's requested RGBA bytes are capped at 8 MiB; a separate 8 MiB preflight includes the requested RGBA and slot metadata for both the retained generation and its replacement during resize. The glyph cache has its own independent 8 MiB nominal canvas-pixel limit; these are not a combined text-resource budget. The scratch checks bound managed Rust requests, not actual allocator capacity or browser heap usage. A resized viewport or a new bounded widget may require more storage. If preparation fails, the previous slots remain available and drawing continues through the glyph cache where needed; the error is available from `App::last_text_raster_scratch_preparation_error()`. Identical requests rejected by geometry or budget preflight are not rebuilt on every frame. Pool construction failures retry with at most 64 skipped preparation calls between attempts; a changed request or `trim_memory` clears the failure state. A memory trim releases the retained scratch and glyph/texture pools before the next preparation. Static text and runs that exceed the prepared extent keep the existing glyph-cache path; the hint is not a strict capacity or a zero-resource-growth guarantee. A bounded `Text` can produce multiple layout runs; if run identities exceed the prepared slot count, new runs replace the least recently used slot. `capacity` limits UTF-8 content bytes, not raster dimensions. Transformed text may also exceed the hint.

`WebCanvasRendererFactory::text_resource_stats()` distinguishes scratch preparation attempts and failures, retained-slot hits, draws, run evictions, and fallbacks from bounded runs that created a glyph-cache surface. Preparation counters count actual new-pool attempts, not cached rejections or skipped calls. `bounded_scratch_current` and `bounded_scratch_peak` are `WebScratchResourceUsage` snapshots. Their Rust capacity covers the RGBA slots and slot table; the peak includes old/new overlap and partial failed construction. Canvas and ImageData fields count retained objects and construction overlap. ImageData byte counts use its declared RGBA pixel length; canvas pixel bytes are only an RGBA-equivalent area, not measured backing allocation. The `js_pixels` view shares ImageData storage, and the retained `rust_view` refers to the Rust RGBA buffer. `bounded_scratch_upload_bytes` counts full-slot uploads after cache misses, not newly allocated bytes; `bounded_upload_bytes` counts new glyph-cache pixel uploads. Scratch and fallback counters cover native linear bounded text; path-laid-out and projective text use separate routes. These counters exclude the software raster scratch, glyph-cache metadata, browser object overhead, internal Canvas allocations, and delayed garbage collection, so they do not establish a browser-heap limit. Ordinary text and text inputs retain their existing rendering path.

An explicit memory warning retains the bounded text-layout cache and its live handles so that reservation still applies to the next frame. Web Canvas raster scratch and glyph/texture pools are separate reconstructible resources and are released. Unbounded text caches retain their existing trim behavior.

## Text on a static path

`path!` stores its commands in the program image. Registering those commands with `insert_static` keeps the path borrowed and gives it a stable `PathId`.

<MiruiSnippet recipe="typo-static" label="Static Path" file="recipes.rs" height="180px">

<template #exercise>

`insert_static` borrows the compile-time `ARC` commands in ROM/flash and returns a `PathId` that `Text(path: arc)` follows from distance `0`.

</template>

</MiruiSnippet>

Passing a `PathId` uses subpath `0`, starts at distance `0`, follows the path forward, and uses the full available length.

## Place multiple lines

Each paragraph line uses one consecutive subpath, beginning at `TextPath::subpath()`. The usable length of each subpath constrains wrapping, alignment, justification, and ellipsis before glyph placement.

<MiruiSnippet recipe="typo-multiline" label="Multi-Subpath" file="recipes.rs" height="190px">

<template #exercise>

`LINES` defines two subpaths (`M ... L ...` and `M ... Q ...`). Line 1 lays out along the straight segment and line 2 wraps onto the quadratic curve.

</template>

</MiruiSnippet>

No baseline array or copied command buffer is created. Line-only subpaths are sampled directly from `Path`; curved subpaths reuse the bounded measurement cache. A missing or invalid subpath rejects the path layout before placed glyph or caret frames are published.

## Select a range and direction

`TextPath` describes how text consumes a registered path without copying its commands.

<MiruiSnippet recipe="typo-range" label="Range & Direction" file="recipes.rs" height="200px">

<template #exercise>

Click `Offset +16` to advance the leading inset along subpath `1`, or click `Reverse` to flip `PathDirection::Forward` and `PathDirection::Reverse`.

</template>

</MiruiSnippet>

`with_seam` chooses the distance treated as the start of a closed subpath. The selected range remains measured in path-length units after the seam is applied.

`with_offset` leaves a fixed-point leading inset inside the selected range. Wrapping, alignment, ellipsis, glyphs, and carets use the remaining path length.

Pass the same placement value to a composition:

```rust
use mirui::prelude::*;
use mirui::ui::widgets::Text;

#[compose]
fn status_label(placement: TextPath) {
    ui! {
        Text("Status", font_size: 24, path: placement)
    };
}
```

## Switch paths reactively

The DSL accepts a `Signal<PathId>` as a reactive `path` attribute. Replacing the signal value moves the existing text entity to the new path and transfers its path-change subscription.

<MiruiSnippet recipe="typo-signal" label="Signal<PathId>" file="recipes.rs" height="200px">

<template #exercise>

Click `Switch path` to update `selected: Signal<PathId>`—the existing `Text` entity moves between the two registered paths without rebuilding the widget.

</template>

</MiruiSnippet>

## Derive path geometry from signals

Create one mutable path, reserve its command storage once, and bind its geometry to signals. `bind_path` installs an owner-bound effect: signal changes edit the existing path, advance its revision, and dirty every subscribed text or drawing entity.

<MiruiSnippet recipe="typo-elastic" label="bind_path" file="recipes.rs" height="200px">

<template #exercise>

Click `Invert amplitude` to flip `amplitude: Signal<Fixed>`—`cx.bind_path` updates the quadratic control point in place and re-lays out the text.

</template>

</MiruiSnippet>

The effect retains the path allocation and command capacity. Prefer `set_command` when the command topology is stable; use `clear` plus path-building methods when the topology itself changes.

## Edit a path from a handler

Event callbacks borrow the path store through `res!(mut PathStore)`. Editing by `PathId` preserves entity identity and updates all consumers of the path.

<MiruiSnippet recipe="typo-bend" label="res!(mut PathStore)" file="recipes.rs" height="190px">

<template #exercise>

Click directly on the curved text to run its `on Tap` handler, mutating the path in `res!(mut PathStore)` with `set_command`.

</template>

</MiruiSnippet>

## Query caret and selection geometry

`Outputs<TextGeometry>` borrows completed geometry for entities with a `Text` component. A component's `#[depends_on(TextGeometry)]` declaration requests the result when its View is registered. Layout, glyph placement, and caret placement share retained storage with text painting; reads do not produce or copy geometry.

<MiruiSnippet recipe="typo-carets" label="Outputs<TextGeometry>" file="recipes.rs" height="190px">

<template #exercise>

`CaretOverlay` declares `#[depends_on(TextGeometry)]` and draws each character's oriented normal caret segment along the curve.

</template>

</MiruiSnippet>

Register `caret_overlay::view()` and attach `CaretOverlay` to the path-text node. `carets_into` and `selection_into` return source-local geometry into caller-provided buffers. The example adds the current paint origin and applies its transform once. `hit_test` accepts screen coordinates; `transform()` provides the prepared source-local-to-screen mapping, including ancestor transforms and scrolling. Linear text exposes its retained layout, font metrics, and local content rectangle through `layout()`, `metrics()`, and `content_rect()`.

Mixed-direction selections are split at visual-run and line boundaries. Exhausted caret storage makes interaction geometry unavailable without discarding prepared glyphs. Selection output remains unchanged when capacity validation fails; hit testing through or behind the near plane returns `PathTextGeometryError::InvalidProjection`. Cache replacement, trimming, hidden sources, and removed text invalidate published geometry until preparation succeeds. TextEditing shares layout storage but does not replace the Text result on the same entity.

## Backend behavior

<ArchFlow kind="compose-backend-route" />

| Geometry | Software | WGPU | SDL GPU | Web Canvas |
|---|---|---|---|---|
| Linear and affine | native | native | native run cache | native canvas path |
| Posed coverage | inverse sampled | instanced atlas | batched scalar atlas | per-glyph transform |
| Posed SDF | derivative-aware | derivative-aware | exact coverage route | exact coverage route |
| Projective glyphs | inverse sampled | projective atlas | bounded software target | bounded software target |

SDL GPU run textures store straight RGB and glyph coverage separately. Transparent texels retain the text color so linear filtering does not introduce a dark fringe; text color alpha and draw opacity are applied during composition.

SDL GPU posed coverage batches split geometry at the region's outer texel centers, preserving the source sampling scale while clamping interpolation to each glyph. Empty clips do not submit text geometry.

`render::PosedGlyphs` pairs borrowed positioned glyphs and placement frames for render commands, Canvas calls, and scene replay. Construction rejects mismatched slice lengths without allocating.

SDL GPU and Web Canvas use a `ProjectiveFallback` supplied through their renderer factory for projective glyphs, fills, borders, and image blits with rounded corners, opacity, and composite modes. Its fixed RGBA storage is both the target and scratch space. `ProjectiveFallback::borrowed(&mut bytes)` uses a caller-owned fixed buffer; `ProjectiveFallback::new(bytes)` owns its buffer. Shape and image requests use projected visual bounds to limit the required storage; glyph requests use projected ink bounds. Preflight rejects missing or insufficient capacity before drawing. Unsupported projective commands remain errors. Gallery reserves 2 MiB for this target on Web Canvas and SDL GPU. The complete routing, numeric-domain, and aligned-memory contract is described in [Exact backend fallback](backend-fallback.md).

## Storage and invalidation

- `PathId` is a generational handle; removing a path invalidates stale handles.
- Static entries borrow command slices and reject edits.
- Mutable entries own or adopt their command storage and expose a monotonically advancing revision.
- Text entities subscribe to the path selected by their `TextPath` component.
- Baseline sampling, posed glyphs, caret frames, ink bounds, and hit testing are cached by path identity and revision.
- Path edits recompute affected line lengths and invalidate placement through the same path revision.
- Ordinary text without a `TextPath` stays on the linear shaping and rendering path.

Set the registry's live-path limit while it is empty. Changing this limit preserves retired ID generations and does not release slot metadata:

```rust
app.with_path_capacity(24)?;
```
