use super::*;

#[cfg(all(feature = "std", feature = "sync"))]
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
fn indexed_region_pins_both_allocations_after_cache_and_source_exit() {
    let pool = DecodePool::new(4096).unwrap();
    let budget = pool.try_take().unwrap().budget.clone();
    let mut cache = pool.try_take().unwrap();
    let mut palette = cache.palette(256).unwrap();
    palette.get_mut().unwrap().bytes[12..16].copy_from_slice(&[0x16, 0x75, 0xe1, 127]);
    let indexed = IndexedPixels {
        format: SampleFormat::I8,
        stride: 4,
        palette,
    };
    let key = Key::new(1, Region::new(0, 0, 4, 1).unwrap());
    let mut pixels = cache.begin(key, indexed.format).unwrap();
    pixels.get_mut().unwrap().bytes.fill(3);
    let charge = pixels._allocation.bytes() + indexed.palette._allocation.bytes();
    cache.publish(key, pixels).unwrap();
    let lease = cache
        .view(key, ColorFormat::RGBA8888, Some(&indexed))
        .unwrap()
        .unwrap();
    drop(cache);
    drop(indexed);
    drop(pool);
    assert_eq!(budget.allocated(), charge);
    assert_eq!(lease.bytes(), &[3; 4]);
    assert_eq!(
        lease.get_pixel(3, 0),
        crate::types::Color::rgba(0x16, 0x75, 0xe1, 127)
    );
    drop(lease);
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
        .begin(key, SampleFormat::RGBA8888)
        .unwrap();
    pixels.get_mut().unwrap().bytes.fill(0x71);
    let charge = pixels._allocation.bytes();
    pool.try_take().unwrap().publish(key, pixels).unwrap();
    let lease = pool
        .try_take()
        .unwrap()
        .view(key, ColorFormat::RGBA8888, None)
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
        .begin(first, SampleFormat::RGBA8888)
        .unwrap();
    let address = pixels.bytes.as_ptr();
    pool.try_take().unwrap().publish(first, pixels).unwrap();
    let allocations = crate::test_support::tracked_allocations(|| {
        for source in 2..100 {
            let key = Key { source, ..first };
            let mut pixels = pool
                .try_take()
                .unwrap()
                .begin(key, SampleFormat::RGBA8888)
                .unwrap();
            assert_eq!(pixels.bytes.as_ptr(), address);
            pixels.get_mut().unwrap().bytes.fill(source as u8);
            pool.try_take().unwrap().publish(key, pixels).unwrap();
            assert!(
                pool.try_take()
                    .unwrap()
                    .view(first, ColorFormat::RGBA8888, None)
                    .unwrap()
                    .is_none()
            );
            assert_eq!(
                pool.try_take()
                    .unwrap()
                    .view(key, ColorFormat::RGBA8888, None)
                    .unwrap()
                    .unwrap()
                    .bytes()[0],
                source as u8
            );
        }
    });
    assert_eq!(allocations, 0);
}
