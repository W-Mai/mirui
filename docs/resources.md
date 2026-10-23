# Resources

`ResourceManager<T>` resolves tokens into shared values under its existing cache budget. `ResourceHandle::get()` returns a completed value or the manager's fallback. Static registrations and completed handle reads do not run loaders.

## Metadata

`Loader<T>` produces values. `ProbeLoader<T>` additionally provides metadata without producing a decoded value. Register the latter with `manager.add_probed_loader(loader)`; `add_loader` does not enable probing implicitly. Metadata can also come from a static value, a completed cached value, or `add_probed_factory`.

```rust,ignore
app.add_plugin(
    ImageResourcesPlugin::empty().with_loader(|images| {
        images.add_probed_loader(MirxLoader::new(|token| match token {
            "cover" => Some(include_bytes!("assets/cover.mirx").as_slice()),
            _ => None,
        }));
    }),
);
```

`MirxLoader` probes the MIRX image metadata without decoding pixels. Metadata cache budgets are enabled with `with_probes` or `enable_probes`; without that cache, available metadata is still returned. An ordinary loader without a probe stops the metadata chain: its token ownership cannot be inferred without decoding. Completed cached values can still provide metadata. Replacement, removal and completed loading invalidate cached metadata.

## Callback lifetime

Loader and probe callbacks run without a borrow of the resource manager. Resolving the same token from its own loader returns the fallback rather than recursively invoking that loader. Other tokens remain accessible. Replacing or removing the active token invalidates the earlier call's completion; it cannot overwrite the replacement. Unwinding releases the active-call guard.

Resource notifications are issued after publication and after releasing the manager borrow. A token's existing subscription remains attached when that token is removed and registered again. Cache hits and warmed handle reads do not allocate call guards or invoke external loaders.

## Image updates

`Image(src: "cover", ...)` observes its resource token through the registered View. Replacing or removing that token invalidates the image without changing `src`. Switching `src`, removing the `Image` component, or deleting its entity releases the old View subscription. Resource publication uses the existing reactive invalidation queues; rendering does not poll for changes.

## Byte providers

`AssetProvider` opens an object by key; its `AssetReader` reports length, positioning and content consistency. Both operations use associated standard futures. A reader can expose a complete immutable mapping through `mapped()`. The built-in `&'static [u8]` reader borrows its mapping and supports immediate random reads.

Each read takes ownership of an `AssetBuffer` and returns it in `ReadComplete`, including on failure. Moving the buffer does not move its bytes. A pending operation retains the buffer and its source session; cancelling a logical consumer is not permission to reuse memory still accessed by a device. Providers must stop physical access before releasing that storage or retain it in their physical completion record.

`ReadProgress::filled` and `eof` are independent: a short read may continue, while zero bytes without EOF on a nonempty request is invalid. `SourceConsistency::Stable` guarantees one immutable content version across reads, not merely an open file descriptor. `Unverified` must not be used to assemble a supposedly consistent media object.

These traits define the byte-provider boundary. They do not register a provider with `ResourceManager` or change the synchronous loader entry points documented above.
