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

Each published texture also exposes `layout_rect()` in root layout coordinates and `local_bounds()` in logical units relative to that origin. These values exclude scrolling and external transforms, and belong to the same successful output as the pixels. Logical size is independent of pixel resolution. Moving a source preserves local history; changing its logical size clears incompatible history even when its rounded pixel dimensions stay the same.

Ordinary outputs reuse one pixel slot. `TemporalMix` requests a second slot for its own history and reads `textures.previous(owner)`, frozen at the start of the drawing batch. Multiple dirty regions share that history; failed production does not advance it. First use and owners without retained history return `None`. History tracks successful outputs rather than adjacent input generations.

`paint.request_frame()` schedules one more visual update after successful painting. Requests within a captured subtree commit with that output; ordinary drawing commits after the complete main-target batch, including all dirty regions. Failed drawing discards its pending requests without cancelling other successfully published outputs. State and declared output dependencies already invalidate their Views without this call.

`TemporalMix` schedules updates while its blend and final composited output are changing. Integer rounding cannot leave a permanent residual. First use, unavailable sources, completed convergence, `mix=0` (current source), and `mix=255` (frozen output) do not schedule another frame.

Output consumers declare their sources on components. Registering a View filtered by that component activates the declarations; editing a source field rebinds the request, and removing the component releases it. Several effects on one entity keep independent source declarations. Implicit requests do not insert `OffscreenRender` or replace the consumer's transform.

```rust
use mirui::{component, ecs::Entity, ui::WidgetTexture};

#[component]
struct Reflection {
    #[depends_on(WidgetTexture)]
    source: Entity,
}

#[component]
#[retain_previous(WidgetTexture)]
struct Feedback {
    #[depends_on(WidgetTexture)]
    source: Option<Entity>,
}
```

Field declarations accept `Entity` or `Option<Entity>`. A type-level `#[depends_on(T)]` requests the owner's current output; `#[retain_previous(T)]` requests owner history without creating a self-dependency. Each attribute accepts multiple output types. Conditional named fields retain their configuration gates; a tuple-field declaration cannot follow a conditionally removed field because its numeric position is not fixed.

The output budget counts stored current and historical pixel slots, in-progress writes, and the shared Mirror/Shadow/Glow/TemporalMix workspace. Admission reserves these pixels before paint callbacks and preserves independent outputs on rejection. A 128×64 RGBA source and its reflection workspace need 64 KiB together. Sequential effects share the largest required workspace, including blur padding; it is released when no effect needs it. The framebuffer, metadata, and other filters' scratch add to the application's total memory requirement. Unchanged inputs reuse completed pixels; changed outputs of the same size reuse their slots.

Widget textures contain local transparent RGBA pixels and never capture the parent framebuffer. Current dependencies and nested output boundaries determine production order, independently of sibling order or damage clipping. Composition applies the node's position, transform, scale, and group opacity; it does not produce textures. Cyclic output dependencies remain unavailable without executing the affected output callbacks. Completed sources remain valid if a dependent output or the final target fails.
