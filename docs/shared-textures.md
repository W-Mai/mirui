# Shared textures

The optional `sync` feature enables cross-thread ownership of `Texture`, its pixel and region guards, byte sources, and `cache::Handle<T>` when `T: Send + Sync`. It is disabled by default and does not enable `std`. Targets need pointer-width atomics; bare-metal applications provide their platform's `critical-section` implementation. Local builds retain `Rc`, local state and callbacks.

```toml
mirui = { version = "0.53", features = ["sync", "std"] }
```

## Worker handoff

Move an existing decoded buffer into the ordinary texture constructor. Moving the texture transfers the original pixel allocation. Sharing a completed resource uses its existing value handle.

```rust
use mirui::core::cache::Handle;
use mirui::render::texture::{ColorFormat, Texture};
use mirui::render::RenderError;

fn finish_decode(bytes: Vec<u8>, width: u16, height: u16)
    -> Result<Texture<'static>, RenderError>
{
    Texture::from_vec(bytes, width, height, ColorFormat::RGBA8888)
}

fn start_worker(bytes: Vec<u8>, width: u16, height: u16)
    -> std::thread::JoinHandle<Result<Texture<'static>, RenderError>>
{
    std::thread::spawn(move || finish_decode(bytes, width, height))
}

fn inspect(value: Handle<Texture<'static>>)
    -> std::thread::JoinHandle<Result<usize, RenderError>>
{
    std::thread::spawn(move || Ok(value.read_pixels()?.bytes().len()))
}
```

Existing loaders publish worker completion on their owning App thread through task wake and polling. `ResourceHandle::get()` returns the completed or fallback `Handle<Texture>`; send that value handle to a worker. App, World, Models, Signals, resource managers and demand handles retain their owning-thread contract. `sync` does not make a renderer or window event loop concurrently callable.

Textures remain non-`Clone`. Cloning a value handle retains the same immutable value; snapshots explicitly copy pixels. Borrowed textures and guards retain their Rust lifetimes. Scoped threads can borrow ready guards without copying pixels.

## Native sources

Compatible same-device draws sample native storage directly. Other consumers prepare the required CPU region through the source, then use their existing upload cache. Preparation pins the region before submission; pixel iteration does not access a device or lock per pixel.

SDL native textures stay on their creating thread. Their transferable representation contains metadata and an owner token. A missing CPU region read on another thread returns `RenderError::WrongThread`; it does not queue a device operation. Prepare pixels on the source thread before handing them to a worker:

```rust
use mirui::render::texture::Texture;
use mirui::render::RenderError;

fn inspect_on_worker(source: &Texture<'_>) -> Result<usize, RenderError> {
    let pixels = source.read_pixels()?;
    Ok(std::thread::scope(|scope| {
        scope.spawn(move || pixels.bytes().len()).join().unwrap()
    }))
}
```

Closing an SDL surface invalidates its native sources and releases raw targets on the source thread. Prepared CPU guards and resident regions remain readable; an uncovered region returns the source error. Worker token release only marks retirement; existing owner maintenance performs destruction. Local builds retain the existing device lifetime through source ownership.

Native WGPU resources share their portable device context and serialize reusable readback state. Textures can retain that context after their original surface is dropped. Device failures and source/target conflicts retain their normal errors. Web backends keep their existing CPU-output strategy; `sync` does not provide a JavaScript object transfer protocol or create a browser worker runtime. Source constraints follow the Rust auto traits supplied by each target's bindings; an accepted type alone does not establish an external JavaScript worker handoff.

## Sources, budgets and extensions

Under `sync`, `AssetProvider` and `AssetReader` require `Send + Sync`; their open and read futures require `Send`. Polling remains exclusive. Stable mappings retain their original reader, and pending or cancelled reads retain in-flight buffers until their physical owner releases them. Loader completion wakes existing UI demand rather than accessing UI state from a worker.

Source blocks, decoded pixels, output storage and backend upload caches keep their separate budgets. Evicting an index or closing an owner does not release charges for pixels or control storage that are still held. Preparation uses bounded waiters without an unbounded work queue. Shared metadata is charged to the corresponding existing budget.

`BackendTarget` payloads require `Send + Sync` with this feature. Portable backends retain their thread-safe device storage directly. Thread-bound backends use `ThreadOwner` and `ThreadResource<T>` without making raw device objects transferable. Native targets attach `TargetStorage` to their actual storage owner. Existing `compose_backend!` routing and target binding remain unchanged. See [backend target storage](backend-targets.md) for the extension contract.
