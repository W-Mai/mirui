use crate::core::threading::{Shared, State};
use alloc::boxed::Box;
use core::task::{Context, Poll};

use super::access::CachedPixels;
use super::{ColorFormat, Region, TexBuf, Texture, TexturePixels};
use crate::core::cache::{Entry, Handle, LruLinearCache};
use crate::core::resource::{AssetError, AssetSource, LoadBudget, LoadError, SourceAccess};
use crate::render::budget::{Allocation, MemoryBudget};
use crate::render::{PlaneLayout, RenderError, RenderResource};

mod encoded;
pub(super) use encoded::{EncodedImage, EncodedPreparation};

pub(super) struct DecodeBuffer {
    bytes: alloc::vec::Vec<u8>,
    _allocation: Allocation,
}

fn capacity_error() -> RenderError {
    RenderError::ResourceLimit(RenderResource::Texture)
}

fn source_error(error: LoadError) -> RenderError {
    match error {
        LoadError::Source(AssetError::Capacity) => capacity_error(),
        _ => RenderError::InvalidTexture,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    source: u64,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl Key {
    fn new(source: u64, region: Region) -> Self {
        Self {
            source,
            x: region.x(),
            y: region.y(),
            width: region.width(),
            height: region.height(),
        }
    }

    fn contains(self, other: Self) -> bool {
        self.source == other.source
            && other.x >= self.x
            && other.y >= self.y
            && other.x + other.width <= self.x + self.width
            && other.y + other.height <= self.y + self.height
    }

    fn layout(self, format: ColorFormat) -> Result<PlaneLayout, RenderError> {
        PlaneLayout::packed(self.width as u16, self.height as u16, format)
            .map_err(|_| RenderError::InvalidTexture)
    }
}

type Index = LruLinearCache<Key, CachedPixels>;
pub(super) type Pool = Shared<State<DecodePool>>;

pub(super) struct DecodePool {
    cache: Index,
    budget: MemoryBudget,
    _index: Allocation,
    next_source: u64,
    limit: usize,
}

impl DecodePool {
    pub(super) fn new(limit: usize) -> Result<Pool, RenderError> {
        let budget = MemoryBudget::new(limit);
        let count = (limit / 4096).clamp(1, 32);
        let owner = core::mem::size_of::<State<Self>>()
            + 2 * core::mem::size_of::<usize>()
            + MemoryBudget::OWNER_BYTES;
        let reservation = budget
            .reserve(owner + count * Index::INDEX_BYTES_PER_ENTRY)
            .ok_or_else(capacity_error)?;
        let (cache, bytes) = Index::prepared(count).map_err(|_| capacity_error())?;
        let allocation = reservation
            .commit(owner + bytes)
            .map_err(|_| capacity_error())?;
        Ok(Shared::new(State::new(Self {
            cache,
            budget,
            _index: allocation,
            next_source: 1,
            limit,
        })))
    }

    fn make_room(&mut self, bytes: usize) -> Result<(), RenderError> {
        if bytes > self.limit {
            return Err(capacity_error());
        }
        while self.budget.available() < bytes {
            // Eviction releases only the index. Live leases retain their original charge.
            if self.cache.evict_one().is_none() {
                return Err(capacity_error());
            }
        }
        Ok(())
    }

    fn source(&mut self, bytes: usize) -> Result<(u64, Allocation), RenderError> {
        let id = self.next_source;
        let next = id.checked_add(1).ok_or_else(capacity_error)?;
        let allocation = self.workspace(bytes)?;
        self.next_source = next;
        Ok((id, allocation))
    }

    pub(super) fn workspace(&mut self, bytes: usize) -> Result<Allocation, RenderError> {
        self.make_room(bytes)?;
        self.budget
            .reserve(bytes)
            .ok_or_else(capacity_error)?
            .commit(bytes)
            .map_err(|_| capacity_error())
    }

    fn buffer(&mut self, size: usize) -> Result<DecodeBuffer, RenderError> {
        self.make_room(size)?;
        let reservation = self.budget.reserve(size).ok_or_else(capacity_error)?;
        let mut bytes = alloc::vec::Vec::new();
        bytes
            .try_reserve_exact(size)
            .map_err(|_| capacity_error())?;
        let allocation = reservation
            .commit(bytes.capacity())
            .map_err(|_| capacity_error())?;
        bytes.resize(size, 0);
        Ok(DecodeBuffer {
            bytes,
            _allocation: allocation,
        })
    }

    fn begin(
        &mut self,
        key: Key,
        format: ColorFormat,
    ) -> Result<Handle<CachedPixels>, RenderError> {
        let layout = key.layout(format)?;
        let bytes = layout
            .required_bytes()
            .checked_add(CachedPixels::OWNER_BYTES)
            .ok_or_else(capacity_error)?;
        if bytes > self.limit {
            return Err(capacity_error());
        }
        while self.budget.available() < bytes
            || self.cache.len()
                == match self.cache.max_size() {
                    crate::core::cache::MaxSize::Count(count) => *count,
                    _ => unreachable!("prepared count-bounded index"),
                }
        {
            let Some(mut old) = self.cache.evict_one() else {
                break;
            };
            if let Some(pixels) = old.get_mut()
                && pixels.bytes.capacity() >= layout.required_bytes()
            {
                pixels.bytes.resize(layout.required_bytes(), 0);
                return Ok(old);
            }
        }
        self.make_room(bytes)?;
        CachedPixels::shared(layout, &self.budget)
    }

    fn view(
        &mut self,
        key: Key,
        format: ColorFormat,
    ) -> Result<Option<TexturePixels<'static>>, RenderError> {
        let found = self
            .cache
            .iter()
            .find_map(|(entry, _)| entry.contains(key).then_some(*entry));
        let Some(found) = found else {
            return Ok(None);
        };
        let owner = self
            .cache
            .acquire(&found)
            .ok_or(RenderError::InvalidTexture)?;
        let region = Region::new(key.x - found.x, key.y - found.y, key.width, key.height)
            .map_err(|_| RenderError::InvalidTexture)?;
        TexturePixels::cached(owner, found.layout(format)?)
            .into_region(region)
            .map(Some)
    }

    fn publish(&mut self, key: Key, pixels: Handle<CachedPixels>) -> Result<(), RenderError> {
        match self.cache.entry(key) {
            Entry::Vacant(entry) => {
                entry
                    .reinsert(pixels)
                    .map_err(|_| RenderError::InvalidScope)?;
            }
            Entry::Occupied(_) => return Err(RenderError::InvalidScope),
        }
        Ok(())
    }
}

struct ReadRegion {
    key: Key,
    pixels: Handle<CachedPixels>,
    filled: usize,
    group: usize,
    ordinal: usize,
}

struct ReadState {
    source: AssetSource,
    pending: Option<ReadRegion>,
    encoded: Option<Box<EncodedImage>>,
}

pub(super) struct DecodedTexture {
    state: State<ReadState>,
    pool: Pool,
    source: u64,
    start: usize,
    layout: PlaneLayout,
    _storage: Allocation,
    #[cfg(any(feature = "sdl-gpu", feature = "wgpu", target_arch = "wasm32"))]
    identity: Shared<()>,
}

impl DecodedTexture {
    pub(super) fn into_texture(
        source: AssetSource,
        layout: PlaneLayout,
        start: usize,
        pool: Pool,
        storage: &mut DecodePool,
        encoded: Option<Box<EncodedImage>>,
    ) -> Result<Texture<'static>, LoadError> {
        let info = source.info().ok_or(AssetError::InvalidSource)?;
        if info.access != SourceAccess::Random {
            return Err(AssetError::Unsupported.into());
        }
        let bytes = core::mem::size_of::<Self>();
        #[cfg(any(feature = "sdl-gpu", feature = "wgpu", target_arch = "wasm32"))]
        let bytes = bytes + 2 * core::mem::size_of::<usize>();
        let (id, allocation) = storage.source(bytes).map_err(|_| AssetError::Capacity)?;
        Ok(Texture::from_layout(
            TexBuf::Decoded(Box::new(Self {
                state: State::new(ReadState {
                    source,
                    pending: None,
                    encoded,
                }),
                pool,
                source: id,
                start,
                layout,
                _storage: allocation,
                #[cfg(any(feature = "sdl-gpu", feature = "wgpu", target_arch = "wasm32"))]
                identity: Shared::new(()),
            })),
            layout,
            false,
        ))
    }

    pub(super) fn layout(&self) -> PlaneLayout {
        self.layout
    }

    pub(super) fn cache_size(&self) -> usize {
        self._storage.bytes()
    }

    #[cfg(any(feature = "sdl-gpu", feature = "wgpu", target_arch = "wasm32"))]
    pub(super) fn identity(&self) -> Shared<()> {
        self.identity.clone()
    }

    pub(super) fn pixels_in(
        &self,
        region: Region,
    ) -> Result<Option<TexturePixels<'static>>, RenderError> {
        self.pool
            .try_with(|pool| pool.view(Key::new(self.source, region), self.layout.format()))
            .map_err(super::state_error)?
    }

    pub(super) fn poll_pixels_in(
        &self,
        region: Region,
        cx: &mut Context<'_>,
    ) -> Poll<Result<TexturePixels<'static>, RenderError>> {
        {
            let mut pool =
                core::task::ready!(self.pool.poll_take(cx)).map_err(super::state_error)?;
            if let Some(pixels) = pool.view(Key::new(self.source, region), self.layout.format())? {
                return Poll::Ready(Ok(pixels));
            }
        }
        let mut reading =
            core::task::ready!(self.state.poll_take(cx)).map_err(super::state_error)?;
        let state = &mut *reading;
        let wanted = Key::new(self.source, region);
        if state.pending.is_none() {
            let mut pool =
                core::task::ready!(self.pool.poll_take(cx)).map_err(super::state_error)?;
            if let Some(pixels) = pool.view(wanted, self.layout.format())? {
                return Poll::Ready(Ok(pixels));
            }
            let pixels = pool.begin(wanted, self.layout.format())?;
            state.pending = Some(ReadRegion {
                key: wanted,
                pixels,
                filled: 0,
                group: 0,
                ordinal: 0,
            });
        }
        let result = self.poll_region(state, cx);
        match result {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(error)) => {
                state.pending = None;
                Poll::Ready(Err(error))
            }
            Poll::Ready(Ok(())) => {
                let mut pool =
                    core::task::ready!(self.pool.poll_take(cx)).map_err(super::state_error)?;
                let complete = state.pending.take().expect("completed region");
                pool.publish(complete.key, complete.pixels)?;
                if let Some(pixels) = pool.view(wanted, self.layout.format())? {
                    Poll::Ready(Ok(pixels))
                } else {
                    // Finish an already submitted read before admitting a changed region.
                    drop(pool);
                    drop(reading);
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            }
        }
    }

    fn poll_region(
        &self,
        state: &mut ReadState,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), RenderError>> {
        let pending = state.pending.as_mut().expect("admitted region");
        if let Some(encoded) = state.encoded.as_mut() {
            return encoded.poll_region(&mut state.source, pending, &self.pool, cx);
        }
        let bpp = self.layout.format().bytes_per_pixel();
        let row_bytes = pending.key.width as usize * bpp;
        let pixels = &mut pending
            .pixels
            .get_mut()
            .ok_or(RenderError::InvalidScope)?
            .bytes;
        let mut budget = LoadBudget::default();
        loop {
            if pending.filled == pixels.len() {
                return Poll::Ready(Ok(()));
            }
            let row = pending.filled / row_bytes;
            let column = pending.filled % row_bytes;
            let count = row_bytes - column;
            let offset = self.start
                + (pending.key.y as usize + row) * self.layout.stride_bytes()
                + pending.key.x as usize * bpp
                + column;
            let count = core::task::ready!(state.source.poll_read_budgeted(
                offset as u64,
                &mut pixels[pending.filled..pending.filled + count],
                cx,
                &mut budget
            ))
            .map_err(source_error)?;
            if count == 0 {
                return Poll::Ready(Err(RenderError::InvalidTexture));
            }
            pending.filled += count;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(all(feature = "std", feature = "sync-cache"))]
    #[test]
    fn shared_decoded_pixels_keep_one_address_and_charge_after_app_and_source_exit() {
        use crate::app::{
            App,
            plugins::{ImageResourcesPlugin, ResourcePlugin},
        };
        use crate::core::resource::{
            AssetBuffer, AssetCacheConfig, AssetReader, ReadComplete, ReadPosition, ReadProgress,
            ResourceManager, SourceConsistency, SourceInfo,
        };
        use crate::surface::framebuf::FramebufSurface;
        use crate::types::PhysicalRect;
        use core::task::Waker;

        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DecodedTexture>();
        assert_send_sync::<super::super::source::MappedTexture>();
        assert_send_sync::<TexturePixels<'_>>();
        assert_send_sync::<super::super::TextureRegion<'_>>();

        struct File(Shared<alloc::vec::Vec<u8>>);
        impl AssetReader for File {
            type Read = core::future::Ready<ReadComplete>;
            fn info(&self) -> SourceInfo {
                SourceInfo {
                    len: Some(self.0.len() as u64),
                    access: SourceAccess::Random,
                    consistency: SourceConsistency::Stable,
                }
            }
            fn read(&self, position: ReadPosition, mut buffer: AssetBuffer) -> Self::Read {
                let ReadPosition::At(offset) = position else {
                    panic!("random source")
                };
                let bytes = &self.0[offset as usize..];
                let filled = bytes.len().min(buffer.len());
                buffer.as_mut_slice()[..filled].copy_from_slice(&bytes[..filled]);
                core::future::ready(ReadComplete {
                    buffer,
                    result: Ok(ReadProgress {
                        filled,
                        eof: filled == bytes.len(),
                    }),
                })
            }
        }
        let pixels = alloc::vec![0x71; 16];
        let bytes = Shared::new(
            mirx::Document::new_flat(mirx::image::ImageAsset::new(
                2,
                2,
                mirx::image::ColorFormat::RGBA8888,
                8,
                pixels.into(),
            ))
            .unwrap()
            .finish()
            .unwrap()
            .into_owned(),
        );
        type TestApp = App<FramebufSurface<fn(&[u8], PhysicalRect)>>;
        let mut app: TestApp = App::headless(2, 2);
        app.add_plugin(
            ImageResourcesPlugin::empty()
                .with_decode_budget(4096)
                .with_source("file", |_| Some("pixels".into())),
        );
        app.add_plugin(
            ResourcePlugin::new()
                .with_cache(AssetCacheConfig::new().block_size(64).max_requests(1))
                .with_source("file", move |_| {
                    core::future::ready(Ok(File(bytes.clone())))
                }),
        );
        app.tick();
        let images = app
            .world
            .resource::<ResourceManager<Texture<'static>>>()
            .unwrap();
        let mut value = images.resolve("pixels");
        for _ in 0..32 {
            if value.width() == 2 {
                break;
            }
            images.poll_pending(&mut Context::from_waker(Waker::noop()));
            value = images.resolve("pixels");
        }
        assert_eq!(value.width(), 2);
        drop(app);
        let texture = match value.try_unwrap() {
            Ok(texture) => texture,
            Err(_) => panic!("resource value must be unique after App exit"),
        };
        let pool = match &texture.buf {
            TexBuf::Decoded(decoded) => decoded.pool.clone(),
            _ => panic!("decoded source"),
        };
        let acquired = std::sync::Barrier::new(2);
        let release = std::sync::Barrier::new(2);
        let wake = crate::core::resource::wake::TaskWake::new();
        wake.take_ready();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                let lease = pool.try_take().unwrap();
                acquired.wait();
                release.wait();
                drop(lease);
            });
            acquired.wait();
            let mut cx = Context::from_waker(wake.waker());
            assert!(texture.poll_read_pixels(&mut cx).is_pending());
            assert!(
                texture
                    .poll_pixels_in(Region::new(0, 0, 2, 2).unwrap(), &mut cx)
                    .is_pending()
            );
            release.wait();
        });
        assert!(wake.take_ready());
        drop(pool);
        let TexBuf::Decoded(decoded) = texture.buf else {
            panic!("nonmapped source must retain decoded storage")
        };
        let budget = decoded.pool.try_take().unwrap().budget.clone();
        let region = Region::new(0, 0, 2, 2).unwrap();
        let leases = std::thread::scope(|scope| {
            let threads: [_; 4] = core::array::from_fn(|_| {
                let decoded = &decoded;
                scope.spawn(move || {
                    let mut cx = Context::from_waker(Waker::noop());
                    for _ in 0..1024 {
                        if let Poll::Ready(result) = decoded.poll_pixels_in(region, &mut cx) {
                            return result.unwrap();
                        }
                        std::thread::yield_now();
                    }
                    panic!("bounded concurrent preparation did not complete");
                })
            });
            threads.map(|thread| thread.join().unwrap())
        });
        let address = leases[0].bytes().as_ptr();
        for pixels in &leases {
            assert_eq!(pixels.bytes(), &[0x71; 16]);
            assert_eq!(pixels.bytes().as_ptr(), address);
        }
        drop(decoded);
        assert_eq!(budget.allocated(), 16 + CachedPixels::OWNER_BYTES);
        drop(leases);
        assert_eq!(budget.allocated(), 0);
    }

    #[test]
    fn evicted_pixel_owners_keep_their_charge_until_the_last_lease_drops() {
        let pool = DecodePool::new(4096).unwrap();
        let budget = pool.try_take().unwrap().budget.clone();
        let key = Key::new(1, Region::new(0, 0, 8, 8).unwrap());
        let mut pixels = pool
            .try_take()
            .unwrap()
            .begin(key, ColorFormat::RGBA8888)
            .unwrap();
        pixels.get_mut().unwrap().bytes.fill(0x71);
        let charge = pixels._allocation.bytes();
        pool.try_take().unwrap().publish(key, pixels).unwrap();
        let lease = pool
            .try_take()
            .unwrap()
            .view(key, ColorFormat::RGBA8888)
            .unwrap()
            .unwrap();
        drop(pool.try_take().unwrap().cache.evict_one());
        assert!(budget.allocated() >= charge);
        drop(pool);
        assert_eq!(budget.allocated(), charge);
        assert_eq!(lease.bytes(), &[0x71; 256]);
        drop(lease);
        assert_eq!(budget.allocated(), 0);
    }

    #[test]
    fn bounded_index_and_exclusive_pixels_recycle_without_allocating() {
        let pool = DecodePool::new(4096).unwrap();
        let first = Key::new(1, Region::new(0, 0, 8, 8).unwrap());
        let pixels = pool
            .try_take()
            .unwrap()
            .begin(first, ColorFormat::RGBA8888)
            .unwrap();
        let address = pixels.bytes.as_ptr();
        pool.try_take().unwrap().publish(first, pixels).unwrap();
        let allocations = crate::test_support::tracked_allocations(|| {
            for source in 2..100 {
                let key = Key { source, ..first };
                let mut pixels = pool
                    .try_take()
                    .unwrap()
                    .begin(key, ColorFormat::RGBA8888)
                    .unwrap();
                assert_eq!(pixels.bytes.as_ptr(), address);
                pixels.get_mut().unwrap().bytes.fill(source as u8);
                pool.try_take().unwrap().publish(key, pixels).unwrap();
                assert!(
                    pool.try_take()
                        .unwrap()
                        .view(first, ColorFormat::RGBA8888)
                        .unwrap()
                        .is_none()
                );
                assert_eq!(
                    pool.try_take()
                        .unwrap()
                        .view(key, ColorFormat::RGBA8888)
                        .unwrap()
                        .unwrap()
                        .bytes()[0],
                    source as u8
                );
            }
        });
        assert_eq!(allocations, 0);
    }
}
