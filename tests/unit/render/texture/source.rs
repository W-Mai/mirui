use super::*;
use crate::app::App;
use crate::app::plugins::{ImageResourcesPlugin, ResourcePlugin};
use crate::core::resource::ResourceManager;
use crate::render::texture::TextureKey;
use crate::surface::framebuf::FramebufSurface;
use crate::types::PhysicalRect;
use alloc::borrow::Cow;
use core::future::ready;

type TestApp = App<FramebufSurface<fn(&[u8], PhysicalRect)>>;

#[test]
fn mapped_opens_have_distinct_upload_identity_even_for_the_same_address() {
    let bytes = mirx::Document::new_flat(mirx::image::ImageAsset::new(
        1,
        1,
        mirx::image::ColorFormat::RGB565,
        2,
        Cow::Borrowed(&[0, 0]),
    ))
    .unwrap()
    .finish()
    .unwrap()
    .into_owned();
    use crate::core::threading::Shared;
    struct Mapping(Shared<alloc::vec::Vec<u8>>);
    impl crate::core::resource::AssetReader for Mapping {
        type Read = core::future::Ready<crate::core::resource::ReadComplete>;
        fn info(&self) -> crate::core::resource::SourceInfo {
            crate::core::resource::SourceInfo {
                len: Some(self.0.len() as u64),
                access: crate::core::resource::SourceAccess::Random,
                consistency: crate::core::resource::SourceConsistency::Stable,
            }
        }
        fn mapped(&self) -> Option<&[u8]> {
            Some(&self.0)
        }
        fn read(
            &self,
            _: crate::core::resource::ReadPosition,
            _: crate::core::resource::AssetBuffer,
        ) -> Self::Read {
            panic!("mapped source cannot require physical reads");
        }
    }
    let bytes = Shared::new(bytes);
    let mut app: TestApp = App::headless(1, 1);
    app.add_plugin(ImageResourcesPlugin::empty().with_source("mapped", |_| Some("same".into())));
    app.add_plugin(
        ResourcePlugin::new()
            .with_source("mapped", move |_| ready(Ok(Mapping(Shared::clone(&bytes))))),
    );
    app.tick();
    let images = app
        .world
        .resource::<ResourceManager<Texture<'static>>>()
        .unwrap();
    let first = images.resolve("first");
    let second = images.resolve("second");
    assert_eq!(
        first.read_pixels().unwrap().bytes().as_ptr(),
        second.read_pixels().unwrap().bytes().as_ptr()
    );
    let old = TextureKey::cacheable(&first).unwrap();
    assert_eq!(old, TextureKey::cacheable(&first).unwrap());
    assert_ne!(old, TextureKey::cacheable(&second).unwrap());
    images.remove_token("first");
    drop(first);
    let replacement = images.resolve("first");
    assert_ne!(old, TextureKey::cacheable(&replacement).unwrap());
}
