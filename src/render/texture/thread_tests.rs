use super::*;

#[cfg(not(feature = "sync"))]
#[test]
fn default_texture_and_cache_values_keep_local_ownership() {
    macro_rules! assert_local {
        ($ty:ty, $bound:path) => {{
            trait Infer<A> {
                fn marker() {}
            }
            impl<T: ?Sized> Infer<()> for T {}
            impl<T: ?Sized + $bound> Infer<u8> for T {}
            let _ = <$ty as Infer<_>>::marker;
        }};
    }
    assert_local!(Texture<'static>, Send);
    assert_local!(Texture<'static>, Sync);
    assert_local!(crate::core::cache::Handle<Texture<'static>>, Send);
    assert_local!(crate::core::cache::Handle<Texture<'static>>, Sync);
}

#[cfg(feature = "sync")]
#[test]
fn complete_texture_and_guard_types_are_transferable() {
    fn assert_shared<T: Send + Sync>() {}
    assert_shared::<Texture<'_>>();
    assert_shared::<TexturePixels<'_>>();
    assert_shared::<TexturePixelsMut<'_>>();
    assert_shared::<TextureRegion<'_>>();
    assert_shared::<crate::core::cache::Handle<Texture<'_>>>();
    assert_shared::<crate::core::resource::AssetSource>();
    assert_shared::<alloc::boxed::Box<dyn crate::render::BackendTarget>>();
}

#[cfg(all(feature = "sync", feature = "std"))]
#[test]
fn owned_and_borrowed_pixels_cross_threads_without_copying() {
    use crate::core::cache::Handle;
    let mut bytes = alloc::vec![0u8; 24];
    bytes[8..12].copy_from_slice(&[20, 40, 60, 255]);
    let address = bytes.as_ptr() as usize;
    let value = std::thread::spawn(move || {
        Handle::new(
            Texture::from_vec(bytes, 2, 2, ColorFormat::RGBA8888)
                .unwrap()
                .with_stride(12)
                .unwrap(),
        )
    })
    .join()
    .unwrap();
    let shared = value.clone();
    std::thread::scope(|scope| {
        let pixels = value.read_pixels().unwrap();
        let region = shared
            .pixels_in(Region::new(0, 1, 1, 1).unwrap())
            .unwrap()
            .unwrap();
        scope.spawn(move || {
            assert_eq!(pixels.bytes().as_ptr() as usize, address);
            assert_eq!(pixels.stride(), 12);
            assert_eq!(region.pixels().unwrap().stride(), 12);
        });
    });
    let read = std::thread::spawn(move || {
        assert_eq!(
            shared.read_pixels().unwrap().bytes().as_ptr() as usize,
            address
        );
        shared
    })
    .join()
    .unwrap();
    assert!(Handle::ptr_eq(&value, &read));

    let mut buffer = [0u8; 4];
    let mut borrowed = Texture::new(&mut buffer, 1, 1, ColorFormat::RGBA8888).unwrap();
    std::thread::scope(|scope| {
        let mut pixels = borrowed.pixels_mut().unwrap();
        scope.spawn(move || pixels.set_pixel(0, 0, &Color::rgb(7, 8, 9)));
    });
    assert_eq!(
        borrowed.read_pixels().unwrap().get_pixel(0, 0),
        Color::rgb(7, 8, 9)
    );
    drop(borrowed);
    assert_eq!(buffer, [7, 8, 9, 255]);
}
