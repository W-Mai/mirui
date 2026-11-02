# Surface content sizing

`Surface::size_control()` optionally provides logical content-size requests. Fixed displays return `None`. Requesting size control must not change the current content dimensions.

`SurfaceSizeControl::request_size(width, height)` records the latest target. Successful recording does not mean the window has resized. `state()` observes actual dimensions, a pending target and any host rejection through the existing reactive signal mechanism; `state_untracked()` reads the same snapshot without subscribing. Zero dimensions are rejected. A retained control rejects new requests after its host is dropped.

Backends retain a `SurfaceSizeHost` and return its `control()`. Create the host lazily when size control is requested. At the existing owner-thread event boundary, take the latest request, invoke the native size operation outside device borrows, and complete it with actual logical dimensions or a `SurfaceSizeError`. Asynchronous implementations retain the opaque `SurfaceSizeRequest` until feedback arrives. Superseded completions cannot acknowledge a newer request. Publish unrelated native resizing with `observe_size`; this does not complete an outstanding operation.

```rust
use mirui::surface::{SurfaceSizeError, SurfaceSizeHost};

let host = SurfaceSizeHost::new(640, 480);
let control = host.control();
control.request_size(940, 480)?;

if let Some(request) = host.take_request() {
    let (width, height) = request.size();
    // Execute on the host thread and report the actual content size.
    host.complete(request, Ok((width, height)));
}
assert_eq!(control.state().actual, (940, 480));
# Ok::<(), SurfaceSizeError>(())
```

Actual dimensions that differ from the requested size report `Restricted`. Hosts can also reject unavailable or unsupported modes without changing the content. Native framebuffer and attachment changes continue through the backend's existing viewport and resize paths; size control does not create render targets or remap input coordinates.

SDL, SDL-GPU and desktop WGPU expose size control for windowed hosts. Native operations run at the existing event boundary; maximized or fullscreen modes can reject a request. `SlowSurface` forwards the same control through nested wrappers. Asynchronous hosts can `defer(request)` across a temporary interruption; only the current request can return to the pending slot.

Web canvas hosts opt in with `data-mirui-size-host="true"`. The surface writes the requested `width,height` to `data-mirui-size-request` and dispatches `mirui-size-request` on the canvas. The document host applies its CSS sizing policy after the current App call returns, then writes `data-mirui-size-result="ok"` or a rejection. The surface confirms actual canvas dimensions on subsequent polling. Do not synchronously call back into the App from that event handler. A host replacing its application can call `WebCanvasSurface::reset_size_control()` to retire old requests and handles before reusing the canvas.
