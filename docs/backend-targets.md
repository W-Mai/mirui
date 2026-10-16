# Backend target storage

`mirui::render::BackendTarget` describes native output storage and source-owned CPU readback. It is a backend extension interface; application components use `Texture` and `Outputs`.

## Storage and lifetime

`TargetDescriptor` combines a checked `PlaneLayout` with `TargetAlpha::Straight` or `Premultiplied`. The alpha encoding describes stored channels, independently of the destination blending policy in `AlphaMode`.

`TargetPlan` reports native storage bytes and drawing workspace bytes. Storage includes multisample and resolve attachments. CPU copies and readback staging need separate admission; a plan is not evidence that allocation succeeded. `allocated_bytes()` reports the actual target allocation.

A target owns the device resources required to sample, read and release its storage. It must not borrow a temporary renderer or require a window frame to remain alive. Validate native handles against device identity and resource generation before using them; matching concrete payload types alone is insufficient.

SDL-GPU surfaces and upload caches share one renderer/window owner and resource generation. Temporary renderers borrow driver state only during upload, drawing or readback. Source pixel preparation, CPU glyph rasterization, software fallback and region-edit callbacks run outside that borrow. A conflicting driver borrow returns `RenderError::InvalidScope`; retained caches keep the underlying renderer alive until their textures are released.

SDL-GPU native targets use single-sample, packed RGBA8 storage with premultiplied channels. Window MSAA does not add multisample attachments to these textures. Targets retain their renderer owner and remain readable after the surface and factory are dropped. Rebuilding clears the complete target; incremental scopes preserve valid contents. Binding saves the parent target, viewport, clip, scale, logical size, draw color and blend mode. Failed, abandoned, out-of-order or stale restoration prevents further device submission.

SDL source reads and region-edit callbacks expose straight-alpha pixels. Software fallback converts the affected region before drawing and converts it back before replacement, reusing its existing workspace. Non-source-over composites on transparent native targets use this path. Source readback accepts padded destinations, including buffers without trailing padding after the final row. Prepared nested binding and source readback allocate no additional Rust storage; target creation and explicit snapshots allocate storage.

WGPU frame attachments determine pipeline format, sample count, clipping bounds and readback format. The resolved view retains its backing texture; a multisample view retains the attachment loaded by subsequent passes. Command encoding, uniform storage and pipeline caching remain shared by the renderer.

Native WGPU readback reuses a device-specific staging buffer and mapping completion state. Factory memory trimming releases that storage; growth replaces it, and failed mapping discards it before retrying. Source bytes are converted directly into checked pixel storage, preserving destination stride and padding. Explicit snapshots still allocate independent CPU pixels, and GPU command encoding and mapping retain their driver costs.

## Source reads

`read_pixels_into` receives a prepared `TexturePixelsMut` with matching dimensions and format. It writes straight-alpha pixels using the destination stride, including when native storage is premultiplied. The source implementation completes necessary pending writes and restores any changed device state on both success and error. Device restoration failures must prevent further submission.

The backend does not create a second persistent CPU cache or retain the destination borrow. Pixel caching and publication belong to the common resource layer. Newly allocated native storage is not a completed image and must not be published before initialization and successful production.

## Renderer scopes

Each `Renderer` declares `type TargetState`; renderers without native target support use `()`. Native backends implement `plan_target`, `create_target`, `bind_target` and `restore_target`. Planning returns `None` when native storage is unsupported. The binding state must not borrow the renderer or the supplied workspace.

The default `with_target` implementation flushes parent work, validates and binds the target, calls the drawing callback once, flushes successful child work and restores the parent. The callback receives the original renderer, including its composed engines. Callback and submission errors still restore the parent; the earliest error is returned. Unwinding attempts restoration, but abort does not run cleanup.

`with_target(target, viewport, update, workspace, body)` passes `TargetUpdate` to `bind_target`. `Rebuild` initializes the entire target to zero before drawing, independently of parent clipping, transforms or blending: alpha formats start transparent and opaque formats start black. Even an empty callback must complete this initialization. Intermediate flushes and returns from nested scopes preserve the current scope's contents. `Incremental` retains successfully completed contents and rejects newly allocated targets or targets with failed writes before invoking the callback. Background colors use ordinary drawing commands, not a separate initialization setting.

The backend checks content validity when binding and invalidates partially modified contents on failure. A failed write is not a rollback; the affected target must be rebuilt before incremental drawing or source reads. Preserving multisampled contents requires retaining the attachments actually loaded by subsequent drawing, not only their resolved image; include those attachments in the storage plan. Shared output textures and retained history are not writable target handles.

`restore_target(state, completed)` receives `true` only when the callback and child flush succeeded. Accept child contents only if restoration also succeeds; otherwise invalidate them. It does not signal publication or window presentation. On failure, pending child commands must be discarded or safely completed on their original target before restoring the parent. Failed binding must restore its partial state or invalidate the device; failed restoration must prevent later submission. Restoring a parent must not initialize it again.

`compose_backend!` forwards target planning, creation, binding and restoration to its `default` field. It uses the default scope executor, preserving every original engine and concrete renderer type, including nested compositions and borrowed engines. No additional DSL, projection type or callback adapter is required from the backend author.

An independent target uses its own viewport and local coordinates. Region offsets and projective transforms on the parent renderer do not transform the new image during production; they apply when drawing into the parent again, including when compositing the finished image. Coordinate wrappers forward target scopes without reapplying parent transforms. Software `render_scope` keeps its existing parent-region coordinate mapping.

The external implementation in [the composed-target contract tests](../tests/compose_backend_basic/native_targets.rs) exercises target planning, allocation, two nested concrete engines, independent target contents, failed production, foreign-device rejection and source reading after renderer destruction. Its `NativeFactory` uses the existing `RendererFactory` associated renderer lifetime to borrow both engines each frame; a target survives across those frames and remains readable after the surface and factory are dropped. The fixture models a native device in memory; it does not require GPU hardware.

The [update contracts](../tests/compose_backend_basic/native_targets/updates.rs) check invalidation after failed binding, submission, callbacks and nested unwinding; restoration failure also prevents later submissions. Nested incremental scopes preserve both targets across intermediate flushes without allocating after preparation. These checks exercise the shared executor and macro routing, not a GPU driver.
