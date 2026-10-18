# Exact backend fallback

MIRUI classifies every checked draw before it changes the target. A backend
uses its native path only when that path preserves the complete request:
geometry, paint, clipping, compositing, filtering, and projection. Everything
else either enters an exact bounded fallback or returns a typed error.

![Backend fallback routing and bounded execution](backend-fallback.svg)

## Route contract

Software affine rounded fills use the existing quad raster path, preserving the local corner radius, opacity, viewport scale and clip mask in direct draws and region scopes. Plain affine fills retain their rectangle path.

SDL-GPU and native WGPU use bounded destination-region fallback for affine and projective lines and arcs. Planning includes stroke extent and clipping before device access. Affine requests retain the software affine raster path; projected strokes are rebased in physical coordinates to avoid scaling a fractional region offset twice.

`PaintCtx::with_clip_path` uses native clipping when supported and otherwise executes its complete body through `plan_scope` and `render_scope`. Planning failure skips the body; successful entry executes it once, preserves nested clip order and drawing context, and retains drawing failures before writeback failures. Region rebasing preserves affine commands as affine operations instead of selecting a projective raster path for a translation.

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

Logical bounds are scaled in `Fixed`; top and left are floored, bottom and right are ceiled; signed edges are clipped to the target; conversion to `u16` happens last. Empty logical bounds remain empty. Backends consume the resulting `PhysicalRect` and do not round it again. Web Canvas applies rectangular scissors in physical pixels; path clips retain their geometric coverage.

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

`Canvas::clear(&PhysicalRect, &Color)` replaces target pixels without blending, antialiasing, or logical scaling. The region is clipped to the current target; pixels outside it remain unchanged. Colors use straight RGBA at the API boundary and are encoded according to the target's alpha representation. Call it outside balanced `push_clip`/`pop_clip` scopes. Drawing state is preserved, and backend errors are reported by `flush()`. `Painter::clear` and composed backends use the same physical-coordinate contract.

`Surface::persistence()` describes whether partial drawing can reuse pixels; it does not request continuous drawing. `Surface::needs_redraw()` reports content loss without consuming the request. App redraws on damage or invalidation, expands a needed transient frame to full coverage, and acknowledges the request only after successful submission. Window exposure can re-present retained software-upload textures without rasterizing the scene again.

`FrameRateCapPlugin` limits logical ticks after platform maintenance, including idle ticks and failed drawing attempts. It requires `MonoClock`; without the plugin or clock there is no implicit frame cap. Mobile hosts use its deadline for event-loop waits and run systems independently of window redraw requests. A platform redraw does not advance systems a second time. Web uses browser animation frames for scheduling.

SDL and WGPU device, presentation, multisampling, and upload-cache options are described in the [quick start](quickstart.md#desktop-sdl). Software-upload hosts configure raster resolution through `SoftwareUploadConfig::render_scale` and bound CPU pixel storage with `framebuffer_budget_bytes`. These are separate from GPU presentation latency and upload caches.

Linux DRM uses `LinuxDrmConfig::mode` to select a connector mode and `buffer_count` to allocate scanout buffers; the default is 2 buffers. A single buffer writes into the scanned framebuffer and can tear. Additional buffers consume pixel storage and do not raise the panel refresh rate. Gallery exposes the existing configuration through `MIRUI_DRM_MODE=640x480` and `MIRUI_DRM_BUFFERS=2`. Framebuffer and embedded surfaces retain their platform transfer and synchronization behavior; a logical tick cap does not change a display bus clock.

Compare processing times using the same scene, physical dimensions, build profile, presentation settings, and cache warm-up. Report average and tail frame times separately from presentation cadence. Readback, tracing, and synchronous logging add work to the measured path; exclude diagnostic captures when measuring ordinary interaction.

An `App` draw or submission failure leaves the last accepted widget bounds unchanged and requires a full redraw before framebuffer scroll transfers resume. Layout or surface acquisition failures before drawing preserve the existing framebuffer. Full and partial frames both use `RendererFactory::mirror_buffers` to synchronize writable rotating buffers after `Surface::end_flush` succeeds.

`Renderer::flush()` explicitly submits pending drawing and returns `Result<(), RenderError>`. `Canvas::flush()` carries the same error contract through canvas adapters and composed backends. App calls it after drawing and any frame readback, before flushing the surface. A submission failure is returned without accepting the frame baseline. Dropping a renderer releases resources without submitting pending work.

Software Blit execution acquires one validated source pixel borrow before sampling, including affine, quad and projective draws. Direct Canvas calls retain source-read failures until `flush()`; Renderer submission returns them immediately. Software fallback preserves these errors through the outer backend instead of treating the draw as successful or replacing the error with an unsupported-geometry result.

Software target-region edits, readback destinations, framebuffer mirroring and scroll transfers use exclusive pixel borrows. Each write borrow advances the destination upload revision, including a region callback that modifies pixels and then fails. Read-only storage is rejected without allocating a writable copy. Region views preserve row stride, exclude trailing padding after their final pixel and remain transient upload sources.

SDL-GPU and native WGPU target readback write directly into the supplied destination region without an intermediate CPU texture. The shared region mapping clips to the destination extent and preserves uncovered pixels and row padding. WGPU retains its staging-buffer reuse and alpha conversion; explicit snapshots still own independent pixels. Driver transfers may synchronize and allocate internally.

SDL GPU region edits borrow the existing `SdlGpuFactory::with_projective_fallback` storage, including its alignment and hard capacity. Missing or insufficient storage rejects the edit before its callback runs. Projected draws and region edits share the same read/convert/edit/replace path; device access is released during the callback so it can read other sources on that device. Callback failure does not upload the partial edit. Explicit snapshots still allocate independent pixels, and GPU upload resources are not included in the CPU workspace reuse guarantee.

Native WGPU region edits and composite fallbacks share a CPU plane retained by the renderer factory. `with_target_edit_budget` bounds that plane; missing or insufficient capacity rejects the edit before the callback. Storage is prepared on first use or growth, reused for smaller edits, and released by `trim_memory`. Readback mapping ends before the callback, and callback failure does not upload partial edits. GPU readback staging and upload resources retain their separate lifecycles.

`Surface::begin_flush()` prepares the target before drawing. `flush(area)` transfers each physical region; `end_flush()` accepts the completed frame. These methods return `Result<(), RenderError>`. App calls `abort_flush()` on an unsuccessful frame and retains its pending damage. `SurfaceUnavailable` means acquisition can be retried; it is returned by explicit rendering and does not produce repeated tick warnings. `frame_end()` maintains platform completion and retained presentation retries, including idle and failed ticks. A successful submission does not generally mean the display has scanned the pixels. DRM waits for the flip event before exposing released buffers as mirror destinations; a timeout does not release them.

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

`App::snapshot()` renders and presents a complete frame while copying its target pixels before the renderer releases the target. It uses the normal frame preparation and completion path without advancing systems or polling input. The returned `Texture` owns its pixels and retains the renderer's format and stride; encoders must inspect these rather than assume RGBA bytes. No root returns `Ok(None)`. Unsupported synchronous readback returns `RenderError::Unsupported(RenderFeature::Readback)` without rendering through a substitute backend. A readback error still allows a successfully drawn frame to be presented and its painted baseline to be recorded.

Pixel constructors (`Texture::new`, `from_ref`, `from_static`, `owned`, and `from_vec`) return `Result<Texture, RenderError>`. Empty dimensions, layout overflow, and short buffers return `InvalidTexture`; `owned` reports allocation failure as `ResourceLimit(Texture)`. `from_vec` takes ownership without copying, and `from_static` supports checked constant construction. Renderers propagate construction failures instead of exposing incomplete readback textures.

`Texture` does not implement `Clone`. Clone a cache handle to share its existing resource, borrow an output for immediate use, or request a frame/widget snapshot when an independent owned copy is required.

MIRX loading and built-in static assets use the same checked texture layout as CPU constructors. Borrowed image planes preserve their row stride and storage; aligned decoded buffers transfer ownership without a pixel copy. A bound `AlignedPlane` exposes its validated layout through `texture()` without allocating.

Pixel storage variants are private to the texture module. Renderer, widget, snapshot and demo checks use the pixel borrow interface rather than inspecting the storage representation.

The widget output budget accounts for reservations and live allocations in one ledger. Current pixels, history slots and paint workspaces retain their charge while borrowed or detached for rendering; releasing storage returns the charge automatically. A zero budget allocates no ledger. Owned texture cache sizes include unused vector capacity and alignment storage, not only visible pixel bytes.

Widget snapshots borrow the completed source and copy its active rows into an independent CPU texture, preserving its format and alpha mode. Software output preparation clears a writable slot through checked pixel access; native production uses the renderer's rebuilding target scope. Failed writes return the slot without publishing it. History comparison propagates read failures instead of treating unavailable pixels as unchanged content. Raw texture storage is internal and has no copy-on-write mutation path.

Use `texture.width()`, `height()`, and `format()` for resource metadata. `texture.pixels()` returns `Result<Option<TexturePixels>, RenderError>` without performing device work; `read_pixels()` returns a readable pixel borrow. The borrow exposes `bytes()`, `stride()`, and `get_pixel()`. Writable CPU storage uses `pixels_mut()` and its `bytes_mut()` / `set_pixel()` methods. `pixels_mut()` rejects read-only storage without allocating a copy. Each writable borrow advances the upload revision once; revision exhaustion disables caching. Padded CPU sources use checked `with_stride(stride)` construction.

Software rasterization acquires a writable borrow before each raster batch, not for each pixel. The blur helper takes the same borrow: `iir_blur_inplace(&mut pixels, alpha, region)`. Direct `Canvas` calls report failed writes through `flush()`; `Renderer::submit` returns them immediately.

`mix_inplace(&mut destination, &source, amount)` also takes pixel borrows. Prepare source reads before borrowing the destination for writing. Target readback copies advance the destination revision, and software scopes validate writable storage before invoking their body and flush before returning success.

SDL, WGPU and Web Canvas uploads read through validated pixel borrows on cache misses. Packed RGBA uploads borrow the existing bytes; other formats and padded rows use the shared conversion. Upload failures remain available through submission and flush instead of being cleared by another draw. Web Canvas shares the texture identity and revision key used by the other GPU upload caches.

SDL and WGPU scalar glyph surfaces use a shared cache key containing the font face, revision, surface identity, dimensions, stride and sample layout. Backend upload formats and byte budgets remain independent; SDL uses RGBA coverage textures while WGPU uses single-channel coverage storage.

Raster glyph geometry shares representation scaling and bearing calculations for positioned and path-placed text. Positioned runs account for the first glyph origin, shaping offsets and font ascender; path placement supplies its own baseline frame. WGPU positioned glyphs use this geometry without changing their sampling or batching.

SDL GPU and WGPU share checked path tessellation. Invalid geometry, vertex limits and tessellator failures propagate through the existing render error path; neither backend submits a partial mesh after failure. Scratch buffers retain capacity for reuse.

SDL GPU tessellation scratch storage belongs to `SdlGpuFactory`. Factory configuration preserves the prepared buffers; `trim_memory()` releases them alongside other factory-owned drawing caches, and subsequent draws prepare them again.

SDL GPU text runs, glyph surfaces and text staging buffers also belong to `SdlGpuFactory`. They are reused only for the same device and generation, replaced after device reset or reassignment, and released by `trim_memory()`. Factory `InspectCaches` reports `sdl_gpu/label` and `sdl_gpu/glyph_surface`, each with a 4 MiB retention limit; staging capacity is separate.

Coverage text uses uploaded font surfaces and retained run geometry. Run entries verify the full shaped-glyph signature, preserve glyph order and reuse geometry across translation, color and opacity changes. Geometry storage and rasterized SDF run textures share the label budget; coverage surfaces use the separate surface budget. Unsupported representations and runs exceeding geometry or surface limits use rasterized run textures, subject to the same label and device limits. Ordinary glyph placement retains software raster rounding, while path-placed text uses continuous glyph frames. Both routes clamp sampling to each glyph region.

`RendererFactory` requires `InspectCaches`; factories without inspectable caches use its empty default implementation. App frame statistics collect both surface and factory caches into the existing `CacheRegistry`.

`SdlGpuFactory::with_geometry_cache_budget(bytes)` limits retained path signatures and tessellated meshes (default 1 MiB; zero disables retention). Physical path content, fill rule and stroke parameters determine reuse; translation, color and clipping do not. Cache hits verify complete signatures and reuse the mesh without tessellation or dashed-stroke expansion. Rounded rectangles, quads and lines provide stack-backed commands without temporary heap paths. Oversized entries draw through scratch storage without retention. `InspectCaches` exposes `sdl_gpu/geometry`; trimming releases retained meshes and scratch buffers while preserving the configured limit.

In a View, `paint.read_pixels(texture)` borrows the source pixels and retains read errors for the render walker. A `None` result means the paint has failed, not that an output is absent; check `outputs.get(entity)` separately. The pixel borrow does not borrow the paint context, so the callback can continue drawing while reading it. Temporal, reflection and drop effects use this access path with their existing workspace and history.

```rust
use mirui::render::renderer::RenderError;
use mirui::render::texture::{ColorFormat, Texture};
use mirui::types::Color;

fn marker() -> Result<Texture<'static>, RenderError> {
    let mut texture = Texture::owned(16, 16, ColorFormat::RGBA8888)?;
    {
        let mut pixels = texture.pixels_mut()?;
        for y in 0..16 {
            pixels.set_pixel(8, y, &Color::rgb(32, 160, 224));
        }
    }
    Ok(texture)
}
```

Software, SDL GPU, native WGPU, and Web Canvas provide target readback. Browser WGPU does not provide synchronous readback. Frame snapshots read the actual render target; they do not include host window decorations or desktop composition. Explicit readback can allocate and synchronize with the GPU, so snapshot timing is not ordinary rendering performance.

`OffscreenRender` renders a widget subtree into a texture and composites the completed result through the outer backend. Configure `app.with_offscreen_pool_budget(bytes)` before constructing output consumers; zero disables output storage and explicit offscreen nodes render inline. `OffscreenRender::with_scale(scale)` controls output resolution, and `with_opacity(alpha)` controls subtree composition. Production selects a native target when its storage and configured fallback workspace fit, otherwise it selects software pixels before drawing. Both paths use the same output graph and history; failed callbacks are not replayed in software. [Native target budgets](backend-targets.md#renderer-scopes) remain distinct from backend upload and fallback limits.

Typed callbacks can declare `textures: res!(Outputs<WidgetTexture>)`, importing both types from `mirui::ui`. `textures.get(source)` returns a borrowed result whose `texture()` method exposes the completed texture, which may reside on a device. An unprepared or failed output returns `None`. Reading does not produce a source, subscribe to its changes, or change the Component-only `query!` contract. A declared dependency, explicit `OffscreenRender` attachment or effect widget establishes the source request. `App::snapshot_widget(entity)` returns an owned CPU copy when the pixels must outlive the callback borrow.

### Composing an output without CPU access

Pass the source texture directly to a drawing command. The View does not request pixels, allocate a destination or select a device. Registering its component-filtered View activates the declared dependency; the source is produced before the consumer. Same-device native sampling and cross-device conversion follow the selected backend's existing texture path.

```rust
use mirui::prelude::*;
use mirui::render::RenderError;
use mirui::render::command::{CompositeMode, DrawCommand};
use mirui::ui::{Outputs, WidgetTexture};

#[component]
struct Preview {
    #[depends_on(WidgetTexture)]
    source: Entity,
}

#[view(component = Preview)]
fn preview(item: component!(), outputs: res!(Outputs<WidgetTexture>), paint: paint!()) {
    let Some(source) = outputs.get(item.source) else {
        return;
    };
    let rect = paint.rect();
    paint.draw(&DrawCommand::Blit {
        pos: Point::new(rect.x, rect.y),
        size: Point::new(rect.w, rect.h),
        transform: paint.transform(),
        quad: paint.quad(),
        texture: source.texture(),
        opa: 255,
        radius: Fixed::ZERO,
        composite: CompositeMode::SourceOver,
    });
}

#[compose]
fn panel() {
    ui! {
        Row (width: 32, height: 16) {
            View (
                id: "source",
                width: 16,
                height: 16,
                bg_color: Color::rgb(32, 160, 224)
            )
            View (width: 16, height: 16) [Preview { source: id("source") }]
        }
    };
}

fn main() -> Result<(), RenderError> {
    let mut app = App::headless(32, 16);
    app.with_default_widgets()
        .with_widget(preview::view())
        .with_offscreen_pool_budget(64 * 1024);
    let root = app.spawn_root().id();
    app.compose(root, panel());
    app.render()
}
```

This executable example uses a software framebuffer. The same component, View and composition work with an App constructed from SDL-GPU, WGPU or Web Canvas surfaces and factories; native output selection additionally checks the factory's fallback capacity. The `outputs` parameter is supplied by the existing `res!` role, not a global variable or an argument passed by the application.

For an effect that actually needs pixels, use `paint.read_pixels(source.texture())` inside its View and keep the returned borrow while reading. That call can synchronize and populate the source's reusable CPU copy. Merely composing the texture above does not make that request. `#[retain_previous(WidgetTexture)]` retains the consumer entity's own successful output; it does not retain history for its `source` field. Read that history with `outputs.previous(owner)`, obtaining `owner` through the existing `entity!()` role.

### Output geometry and history

Each published texture also exposes `layout_rect()` in root layout coordinates and `local_bounds()` in logical units relative to that origin. Local bounds include borders, shadow and glow extents, and unclipped children; explicit child clips and scrolling viewports still use the layout rectangle. Capture dimensions round outward after scaling, and composition uses the same footprint without moving the layout-based transform pivot. These values exclude ancestor scrolling and external transforms and belong to the same successful output as the pixels. Moving a source preserves local history; changing its logical size, local footprint, or output scale clears incompatible history even when its pixel dimensions stay the same.

Ordinary outputs reuse one pixel slot. `TemporalMix` requests a second slot for its own history and reads `textures.previous(owner)`, frozen at the start of the drawing batch. Multiple dirty regions share that history; failed production does not advance it. First use and owners without retained history return `None`. History tracks successful outputs rather than adjacent input generations.

`paint.request_frame()` schedules one more visual update after successful painting. Requests within a captured subtree commit with that output; ordinary drawing commits after the complete main-target batch, including all dirty regions. Failed drawing discards its pending requests without cancelling other successfully published outputs. State and declared output dependencies already invalidate their Views without this call.

`TemporalMix` schedules updates while its blend and final composited output are changing. Integer rounding cannot leave a permanent residual. First use, unavailable sources, completed convergence, `mix=0` (current source), and `mix=255` (frozen output) do not schedule another frame.

Output consumers declare their sources on components. Registering a View filtered by that component activates the declarations; editing a source field rebinds the request, and removing the component releases it. Several effects on one entity keep independent source declarations. Implicit requests do not insert `OffscreenRender` or replace the consumer's transform.

Source changes and new publications invalidate each consumer according to its View's `invalidation` setting: `Visual` requests repainting, while `Layout` also requests layout. An existing visual invalidation does not prevent a layout consumer from receiving the stronger invalidation.

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

`LayoutGeometry` publishes widget bounds without a texture. Declare `#[depends_on(LayoutGeometry)]` and read `res!(Outputs<LayoutGeometry>)`, importing both types from `mirui::ui`. `get(source)` returns a copyable snapshot: `border_rect()` and `content_rect()` use widget-local logical coordinates; `transform()` maps them to the screen, including ancestor scrolling and 2D/3D transforms; `clip()` is the screen-space intersection of ancestor clips and the viewport. Content bounds use resolved layout padding, including safe-area padding. Geometry is prepared before input and drawing, not while reading. Hidden, detached, deleted, or unprepared sources return `None`; an empty ancestor intersection retains geometry with an empty clip. Unchanged geometry retains its publication version. Only requested entities have records, with no pixel buffers or per-frame storage growth.

The output budget counts stored current and historical slots, in-progress writes, source CPU copies and readback reservations, and shared paint/binding workspace. Admission reserves this working set before paint callbacks and preserves independent outputs on rejection. On the software path, a 128×64 RGBA source and its reflection workspace need 64 KiB together, excluding any separately captured consumer output. Native storage and staging add their own charges. Sequential effects share the largest required paint workspace, including blur padding; it is released when no effect needs it. Offscreen captures also share retained software path and clip scratch, released when capture demand ends or on a memory warning. This raster scratch, the framebuffer, and metadata are outside the output byte budget. Factory-owned destination fallback buffers and upload caches keep their separate limits. Unchanged inputs reuse completed textures; changed outputs of the same size reuse compatible slots.

Widget textures contain local transparent RGBA pixels and never capture the parent framebuffer. Current dependencies and nested output boundaries determine production order, independently of sibling order or damage clipping. Composition applies the node's position, transform, scale, and group opacity; it does not produce textures. Cyclic output dependencies remain unavailable without executing the affected output callbacks. Completed sources remain valid if a dependent output or the final target fails.
