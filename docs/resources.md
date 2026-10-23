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
