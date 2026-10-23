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

## Deferred loading

`Loader` provides `State`, `recognize`, `poll_load` and `reset`; `ProbeLoader` provides `poll_probe` using the same state. A pending invocation retains its progress and returns the fallback to consumers. The loader must register `LoadContext::waker()` with its completion mechanism. `LoadContext::consume_work()` bounds cooperative decoding work and schedules another poll when the allowance is exhausted. Simple `Fn(&str) -> Result<T, LoadError>` loaders complete immediately through this same protocol.

An owner-thread executor calls `manager.poll_pending(&mut context)`. This registers its standard `Waker`, advances ready invocations and publishes completed values through existing token subscriptions. I/O wake notifications never invoke a loader, access UI state or publish resources from another thread. Self-driven callers can poll on their normal tick with a no-op waker; an event-driven executor must supply a waker that schedules another turn.

Each loader registration prepares four reusable invocation slots. Full capacity returns the fallback and marks the attempted value load failed; `clear_failed(token)` permits an explicit retry. Pending invocations are not evicted. Completed probe state can be reused by value loading or retired to admit another token. Slot and wake storage is prepared at registration; token strings, completed values, metadata and third-party decoding allocations are separate from the value-cache budget. Static-only managers allocate no invocation slots or wake state.

Replacing or removing a pending token rejects its old completion. An invalidated invocation retires on the next owner-thread poll, including when the external producer has not sent a wake. Loaders that own asynchronous operations must ensure resetting their state cannot free memory still accessed by that operation. Metadata probing never falls through to full decoding.

Handles share demand for a token. Releasing the last handle retires a pending load initiated through those handles, without discarding successful cached values. Submitted source reads still finish and return their buffers before storage can be reused. Direct manager calls to `resolve`, `probe`, `prefetch`, `prefetch_probe` or `reload` keep their pending request independently of handles; merging one of these requests into an existing load preserves that work until completion or explicit invalidation.

`manager.reload(token)` schedules the registered loader chain for the next owner-thread poll. It does not run the resolver or provider at the call site and does not replay consumed one-shot factories. Requests for the same token coalesce into the newest generation; superseded completions cannot publish. No registered loaders returns `NotMine`, and exhausted prepared request capacity returns `Capacity`.

Refresh leaves the previous successful value in the value cache until replacement succeeds. Failed refresh does not discard it. The old value remains subject to the existing cache budget and eviction policy; independently held values stay immutable and valid. A source binding resolves and opens its key again, so changing a resolver's captured directory or alias takes effect after an explicit reload.

## Image updates

Image loaders advance in the existing owner-thread systems, including when an App has no UI root. Mobile hosts connect completion wakes to their event loop without running loaders on the notifying thread. Suspended Apps retain readiness until resume; active Apps respect the existing tick deadline. Desktop, Web and self-driven embedded hosts advance ready work on their next tick. Pending work does not schedule additional animation frames, and an unobserved resource completion does not request drawing.

`Image(src: "cover", ...)` observes its resource token through the registered View. Replacing or removing that token invalidates the image without changing `src`. Switching `src`, removing the `Image` component, or deleting its entity releases the old View subscription. Resource publication uses the existing reactive invalidation queues; rendering does not poll for changes.

The image owns its resource handle. Replacing or removing it releases its loading demand through normal component ownership; switching to another token or a vector releases the previous demand when the component's dependencies are reconciled, even before another visible paint. Other images requesting the same token continue to share its pending work.

## Byte providers

`AssetProvider` opens an object by key; its `AssetReader` reports length, positioning and content consistency. Both operations use associated standard futures. A reader can expose a complete immutable mapping through `mapped()`. The built-in `&'static [u8]` reader borrows its mapping and supports immediate random reads.

Each read takes ownership of an `AssetBuffer` and returns it in `ReadComplete`, including on failure. Moving the buffer does not move its bytes. A pending operation retains the buffer and its source session; cancelling a logical consumer is not permission to reuse memory still accessed by a device. Providers must stop physical access before releasing that storage or retain it in their physical completion record.

`ReadProgress::filled` and `eof` are independent: a short read may continue, while zero bytes without EOF on a nonempty request is invalid. `SourceConsistency::Stable` guarantees one immutable content version across reads, not merely an open file descriptor. `Unverified` must not be used to assemble a supposedly consistent media object.

`ResourcePlugin` registers named providers. `ImageResourcesPlugin::with_source` maps image tokens to provider keys; its existing `with_loader` configuration installs byte recognizers and decoders. The plugins bind during App startup, after all plugin resources are installed. Decoders recognize `LoadInput::Bytes`, request bounded prefixes with `FormatMatch::Need`, and read through `LoadContext::poll_read_exact`. Multiple recognizers claiming the same content fail rather than choosing by registration order.

Image source bindings include a MIRX byte decoder for mapped FLAT images. The texture retains the opened reader and borrows the checked pixel plane, preserving row stride without copying pixels. Opening can be asynchronous; completion updates existing Image subscriptions. The value cache charges the complete mapping length and its texture control storage. A mapping is read-only and must report its exact length; unsupported container layouts or nonmapped sources return `Unsupported`. Static `with_mirx_bytes` registrations retain their direct borrowing path and do not install source sessions.

`LoadContext::poll_read(offset, dst)` reads a bounded prefix with the same invocation allowance as `poll_read_exact`. Advance by the returned count; zero on a nonempty destination confirms EOF. This permits strict trailing-byte checks when the source length is unknown without treating I/O failures as EOF. Empty reads do not establish EOF. Finish any pending exact read before starting a prefix read; both methods reuse the opened source, block cache and cancellation storage.

```rust,ignore
app.add_plugin(
    ResourcePlugin::new()
        .with_source("assets", provider)
        .with_cache(AssetCacheConfig::new()
            .block_size(1024)
            .budget(MaxSize::Bytes(8 * 1024))
            .max_in_flight(2)),
);
app.add_plugin(
    ImageResourcesPlugin::empty()
        .with_source("assets", |token| {
            let id = token.strip_prefix("cover/")?;
            Some(format!("covers/{id}/thumbnail.r4").into())
        })
        .with_loader(|images| images.add_probed_loader(decoder)),
);
```

The resolver runs when starting a request, not on each frame. A borrowed key avoids a key allocation; an owned `Cow` transfers a dynamically constructed key. The provider owns path interpretation, permissions and content-stability guarantees. Metadata and decoded contents reuse the same opened object and decoder state. Static image registrations need neither a source plugin nor a byte cache.

Use `decoder.for_source(name, resolver)` to select a decoder explicitly, including formats without a recognizable header. Register the returned `BoundLoader` through the same `add_loader` or `add_probed_loader` method. Its state, reset and metadata behavior remain those of the wrapped decoder. Explicitly bound decoders do not participate in automatic content recognition; they still validate their input while decoding. Overlapping automatic and explicit source claims, or two explicit claims, fail before opening a provider. Unknown provider names fail during App startup.

```rust,ignore
app.add_plugin(
    ImageResourcesPlugin::empty().with_loader(|images| {
        images.add_probed_loader(decoder.for_source("assets", |token| {
            let id = token.strip_prefix("raw/")?;
            Some(format!("images/{id}.raw").into())
        }));
    }),
);
```

This binding does not require a separate `ImageResourcesPlugin::with_source`. The provider is still registered with `ResourcePlugin`. Wrapping a loader prepares one resolver allocation during registration, with no additional per-poll decoder state or task queue.

### Retaining an opened source

`LoadContext::take_source()` transfers the opened `AssetSource` into decoder state or a completed resource. It does not reopen the key, copy source bytes or allocate another session. Complete any exact read before transferring; a repeated transfer or a transfer during an incomplete exact read fails. The context no longer supplies that source after transfer.

`AssetSource::mapped()` borrows the complete resident mapping with the source's lifetime, returning `Ok(None)` when physical reads are required. It validates the mapping length and performs no I/O, copying or cache allocation. Retaining the source keeps the reader and provider alive; the borrowed slice cannot outlive that owner.

`AssetSource::poll_read(offset, destination, context)` reads a bounded prefix and returns its byte count. It retains no destination reference across `Pending`; resume the same offset until ready, then advance by the returned count. A nonempty request returning zero bytes indicates EOF. Each call uses the same 4 KiB copy and 32-operation limits as loader polling, so a ready short result is not necessarily EOF. A decoder looping over these calls must still charge its own execution allowance with `consume_work`. The original provider, read session and shared cache remain alive independently of the manager or App. The retained source continues to occupy its prepared provider slot until dropped, and pending physical reads retire before that slot is reused.

```rust,ignore
// Inside poll_load, after metadata reads have completed:
state.source = Some(cx.take_source()?);

// Later, using the retained owner and the current task's waker:
let source = state.source.as_mut().unwrap();
let mut task = core::task::Context::from_waker(cx.waker());
let count = core::task::ready!(source.poll_read(offset, destination, &mut task))?;
```

App systems advance ready image loads and retire submitted source operations on the owning thread, including when no UI root exists. An application driving `App::tick` delivers completions on a subsequent tick; an externally driven sleeping loop supplies its wake through the manager's polling context. Publication uses the same resource subscription that invalidates `Image`.

### Read storage

`AssetCacheConfig` defaults to 1 KiB blocks, a 4 KiB shared payload budget, two physical reads in flight, four prepared sessions per provider, 1 KiB keys and 256-byte recognition prefixes. `max_requests`, `max_key_bytes` and `probe_bytes` adjust the latter limits. `MaxSize::Count` limits payload blocks; `Disabled` disables residency but retains `max_in_flight` working blocks. Unbounded source storage is rejected. These settings do not limit third-party decoder output or driver-owned storage.

Cached blocks, evicted blocks held by readers and physical reads charge the same prepared payload capacity. Session objects, pinned open/read future slots, recognition prefixes and cache metadata are separate cold allocations. Short reads reuse the original block and future slot. Mapped reads bypass block storage. Polling and warmed mapped-session reuse do not allocate framework buffers; dynamic keys, resource values and decoder allocations remain separate.

Cancelling a request stops decoding and new reads. A submitted open or read retains its prepared session until completion; retirement returns the original buffer without starting another short read or publishing cancelled data. Exhausted session/read capacity returns an error rather than expanding storage. Recognition prefixes are retained separately so sequential decoders can reread their header after format selection.
