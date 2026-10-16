# Backend target storage

`mirui::render::BackendTarget` describes native output storage and source-owned CPU readback. It is a backend extension interface; application components use `Texture` and `Outputs`.

## Storage and lifetime

`TargetDescriptor` combines a checked `PlaneLayout` with `TargetAlpha::Straight` or `Premultiplied`. The alpha encoding describes stored channels, independently of the destination blending policy in `AlphaMode`.

`TargetPlan` reports native storage bytes and drawing workspace bytes. Storage includes multisample and resolve attachments. CPU copies and readback staging need separate admission; a plan is not evidence that allocation succeeded. `allocated_bytes()` reports the actual target allocation.

A target owns the device resources required to sample, read and release its storage. It must not borrow a temporary renderer or require a window frame to remain alive. Validate native handles against device identity and resource generation before using them; matching concrete payload types alone is insufficient.

## Source reads

`read_pixels_into` receives a prepared `TexturePixelsMut` with matching dimensions and format. It writes straight-alpha pixels using the destination stride, including when native storage is premultiplied. The source implementation completes necessary pending writes and restores any changed device state on both success and error. Device restoration failures must prevent further submission.

The backend does not create a second persistent CPU cache or retain the destination borrow. Pixel caching and publication belong to the common resource layer. Newly allocated native storage is not a completed image and must not be published before initialization and successful production.
