# Exact backend fallback

MIRUI classifies every checked draw before it changes the target. A backend
uses its native path only when that path preserves the complete request:
geometry, paint, clipping, compositing, filtering, and projection. Everything
else either enters an exact bounded fallback or returns a typed error.

![Backend fallback routing and bounded execution](backend-fallback.svg)

## Route contract

`RenderRoute::Native` performs no fallback allocation, readback, or upload.
`RenderRoute::ExactFallback(FallbackRegion)` carries the physical destination
and row stride required by a prepared software replay. Planning completes
before target access, so missing storage, invalid alignment, and insufficient
capacity cannot leave a partially updated scope.

The fallback path has three transfers at most:

1. Read the clipped target region when the operation depends on existing
   destination pixels.
2. Replay the unsupported scope into the same region-local RGBA plane.
3. Write or composite that region back once.

Opaque isolated content can skip the first step when its blend semantics do
not depend on the destination. Consecutive operations that share ordering and
clip state stay inside one fallback scope instead of crossing the backend
boundary per command.

## Geometry and memory types

The types describe different boundaries and intentionally compose rather than
alias one another:

```text
Rect<Fixed>                     logical geometry with subpixels
└─ Viewport::physical_rect()
   └─ PhysicalRect<u16>         clipped integer destination pixels
      └─ FallbackRegion         destination pixels + RGBA row stride
         └─ PlaneLayout         local format, stride, size, alignment
            └─ AlignedPlane     validated caller-owned bytes
```

`PhysicalRect` is shared by surfaces, dirty flushes, target readback, mirror
copies, scrolling, and fallback routes. `FallbackRegion` remains separate
because only a fallback route promises a row layout and byte budget. Embedding
the full `PlaneLayout` in every retained route would repeat format, required
size, and alignment fields; scene replay can retain many routes on a small
target.

Numeric conversion follows one direction:

| Value | Type | Lifetime |
|---|---|---|
| layout, glyph, path, clip, affine transform | `Fixed` | until raster or viewport conversion |
| homography and projection intermediates | `Fixed64` | until projection validation |
| unclipped pixel edges and signed motion | `i32` | before target clipping |
| clipped physical origin and extent | `u16` in `PhysicalRect` | after clipping |
| stride, capacity, required bytes, slice offsets | `usize` | runtime memory only |
| MIRX dimensions, offsets, and stride | `u32` | stable wire representation |
| SDL/WGPU/Web coordinates | backend-required `u32` or `f64` | final host call only |

Logical bounds are scaled in `Fixed`; top and left are floored, bottom and
right are ceiled; signed edges are clipped to the target; conversion to `u16`
happens last. Backends consume the resulting `PhysicalRect` and do not round it
again.

## Aligned caller storage

`PlaneRequirements` declares start-address and row-stride alignment. The
packed CPU default uses alignment 1. DMA, firmware, or GPU upload paths can
request aligned storage explicitly:

```rust
use mirui::render::{PlaneRequirements, ProjectiveFallback};

let requirements = PlaneRequirements::new(64, 64)?;
let fallback = ProjectiveFallback::borrowed_with(&mut bytes, requirements)?;
```

Planning rounds the minimum RGBA row size up to the requested stride alignment.
Binding checks the actual slice address, aligned stride, checked
`stride_bytes * height`, and available capacity. The same validated stride is
used for readback, software replay, and upload. The ordinary native route does
not inspect or carry this storage.

## Backend behavior

- Software writes directly into its framebuffer and normally needs no RGBA
  fallback plane.
- WGPU keeps supported projective geometry and glyph sampling native; exact
  destination-dependent operations use a clipped target edit on desktop.
- SDL GPU reads one clipped region into the configured aligned plane, performs
  local software replay, and uploads it with the same pitch.
- Web Canvas copies packed `ImageData` rows into the configured plane, replays
  locally, compacts only at the browser API boundary, and writes one
  `ImageData` region.

The public route result is semantic rather than predictive performance data.
A native route is preferred; fallback cost is proportional to the clipped
physical region and the number of backend crossings, not to the full surface
or glyph count.

## Retained widget outputs

`OffscreenRender` renders a widget subtree into a CPU texture and composites the completed result through the outer backend. Configure `app.with_offscreen_pool_budget(bytes)` before constructing output consumers; zero disables output storage and explicit offscreen nodes render inline. `OffscreenRender::with_scale(scale)` controls output resolution, and `with_opacity(alpha)` controls subtree composition. Subtree production uses the software renderer even when the outer backend uses a GPU.

Typed callbacks can declare `textures: res!(Outputs<WidgetTexture>)`, importing both types from `mirui::ui`. `textures.get(source)` returns a borrowed result whose `texture()` method exposes completed pixels. An unprepared or failed output returns `None`. Reading does not produce a source, subscribe to its changes, or change the Component-only `query!` contract. An explicit `OffscreenRender` attachment or an effect widget establishes the existing source request. `App::snapshot_widget(entity)` returns an owned copy when the pixels must outlive the callback borrow.

Ordinary outputs reuse one pixel slot. `TemporalMix` requests a second slot for its own history and reads `textures.previous(owner)`, frozen at the start of the drawing batch. Multiple dirty regions share that history; failed production does not advance it. First use and owners without retained history return `None`. History tracks successful outputs rather than adjacent input generations.

The output budget counts stored current and historical pixel slots together with in-progress writes. Admission rejects an oversized request before pixel allocation and preserves other outputs. For example, a 128×64 RGBA texture needs 32 KiB for one slot and 64 KiB for two retained slots. Source textures, the framebuffer, metadata, and filter scratch add to the application's total memory requirement. Unchanged inputs reuse completed pixels; changed outputs of the same size reuse their slots. `OffscreenAlphaMode` still selects transparent clearing or parent-target initialization, and nested offscreen nodes and projectively transformed sources remain unsupported.
