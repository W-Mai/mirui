# Backend target storage

`mirui::render::BackendTarget` describes native output storage and source-owned CPU readback. It is a backend extension interface; application components use `Texture` and `Outputs`.

## Storage and lifetime

`TargetDescriptor` combines a checked `PlaneLayout` with `TargetAlpha::Straight` or `Premultiplied`. The alpha encoding describes stored channels, independently of the destination blending policy in `AlphaMode`.

`TargetPlan` reports native storage bytes and drawing workspace bytes. Storage includes multisample and resolve attachments. CPU copies and readback staging need separate admission; a plan is not evidence that allocation succeeded. `allocated_bytes()` reports the actual target allocation.

A target owns the device resources required to sample, read and release its storage. It must not borrow a temporary renderer or require a window frame to remain alive. Validate native handles against device identity and resource generation before using them; matching concrete payload types alone is insufficient.

## Source reads

`read_pixels_into` receives a prepared `TexturePixelsMut` with matching dimensions and format. It writes straight-alpha pixels using the destination stride, including when native storage is premultiplied. The source implementation completes necessary pending writes and restores any changed device state on both success and error. Device restoration failures must prevent further submission.

The backend does not create a second persistent CPU cache or retain the destination borrow. Pixel caching and publication belong to the common resource layer. Newly allocated native storage is not a completed image and must not be published before initialization and successful production.

## Renderer scopes

Each `Renderer` declares `type TargetState`; renderers without native target support use `()`. Native backends implement `plan_target`, `create_target`, `bind_target` and `restore_target`. Planning returns `None` when native storage is unsupported. The binding state must not borrow the renderer or the supplied workspace.

The default `with_target` implementation flushes parent work, validates and binds the target, calls the drawing callback once, flushes successful child work and restores the parent. The callback receives the original renderer, including its composed engines. Callback and submission errors still restore the parent; the earliest error is returned. Unwinding attempts restoration, but abort does not run cleanup.

`restore_target(state, completed)` receives `true` only when the callback and child flush succeeded. It does not signal publication or window presentation. On failure, pending child commands must be discarded or safely completed on their original target before restoring the parent. Failed binding must restore its partial state or invalidate the device; failed restoration must prevent later submission.

`compose_backend!` forwards target planning, creation, binding and restoration to its `default` field. It uses the default scope executor, preserving every original engine and concrete renderer type, including nested compositions and borrowed engines. No additional DSL, projection type or callback adapter is required from the backend author.

The external implementation in [the composed-target contract tests](../tests/compose_backend_basic/native_targets.rs) exercises target planning, allocation, two nested concrete engines, independent target contents, failed production, foreign-device rejection and source reading after renderer destruction. The fixture models a native device in memory; it does not require GPU hardware.
