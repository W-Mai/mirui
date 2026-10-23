use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::rc::Rc;
use core::task::{Context, Waker};

use crate::app::plugin::Plugin;
use crate::app::plugins::resources::ResourceHostWake;
use crate::app::{App, RendererFactory};
use crate::core::cache::MaxSize;
use crate::core::resource::{AssetSources, ResourceManager, SourceBinding};
use crate::ecs::World;
use crate::render::texture::{ColorFormat, MirxTextureOptions, Texture, TextureMeta};
use crate::surface::Surface;
use crate::ui::widgets::assets::IMG_THUMBS_UP;

/// Default values-cache byte budget. 256 KiB covers a few small icons + a
/// hero-size RGBA8888 texture without touching the LRU on a typical
/// embedded scene.
const DEFAULT_VALUES_BUDGET: MaxSize = MaxSize::Bytes(256 * 1024);

/// Default probes-cache count budget. One entry per registered token is
/// enough for the full screen of widgets even on a busy app.
const DEFAULT_PROBES_BUDGET: MaxSize = MaxSize::Count(1024);
const DEFAULT_DECODE_BUDGET: usize = 128 * 1024;

/// 16x16 checkerboard pixels (RGBA8888) used as the manager's fallback
/// texture. Magenta on black so missing assets stand out at a glance.
static CHECKERBOARD_PIXELS: [u8; 16 * 16 * 4] = checkerboard_pixels();

const fn checkerboard_pixels() -> [u8; 16 * 16 * 4] {
    let mut buf = [0u8; 16 * 16 * 4];
    let mut y = 0;
    while y < 16 {
        let mut x = 0;
        while x < 16 {
            let i = (y * 16 + x) * 4;
            let on = ((x / 4) ^ (y / 4)) & 1 == 0;
            if on {
                buf[i] = 0xFF; // R
                buf[i + 2] = 0xFF; // B = magenta
            }
            buf[i + 3] = 0xFF; // A
            x += 1;
        }
        y += 1;
    }
    buf
}

const FALLBACK_TEXTURE: Texture<'static> =
    Texture::expect_static(&CHECKERBOARD_PIXELS, 16, 16, ColorFormat::RGBA8888);

const FALLBACK_META: TextureMeta = TextureMeta {
    width: 16,
    height: 16,
    format: ColorFormat::RGBA8888,
};

type StaticEntry = (Cow<'static, str>, Texture<'static>);
type MirxEntry = (Cow<'static, str>, &'static [u8], MirxTextureOptions);
type LoaderEntry = Box<dyn FnOnce(&ResourceManager<Texture<'static>>) + 'static>;

/// Inserts a [`ResourceManager<Texture<'static>>`] into the world and seeds
/// it with built-in assets. Use `with_static` / `with_mirx_bytes` /
/// `with_loader` to extend before `app.add_plugin(...)`.
///
/// **Inserts**
/// - resource: `ResourceManager<Texture<'static>>`
/// - system: owner-thread loader polling when loaders or sources are configured
/// - hooks: `on_start` binds named byte providers
pub struct ImageResourcesPlugin {
    values_budget: MaxSize,
    probes_budget: MaxSize,
    decode_budget: usize,
    bundle_thumbs_up: bool,
    statics: alloc::vec::Vec<StaticEntry>,
    mirx_bytes: alloc::vec::Vec<MirxEntry>,
    loaders: alloc::vec::Vec<LoaderEntry>,
    sources: alloc::vec::Vec<SourceBinding>,
}

impl Default for ImageResourcesPlugin {
    fn default() -> Self {
        Self {
            values_budget: DEFAULT_VALUES_BUDGET,
            probes_budget: DEFAULT_PROBES_BUDGET,
            decode_budget: DEFAULT_DECODE_BUDGET,
            bundle_thumbs_up: true,
            statics: alloc::vec::Vec::new(),
            mirx_bytes: alloc::vec::Vec::new(),
            loaders: alloc::vec::Vec::new(),
            sources: alloc::vec::Vec::new(),
        }
    }
}

impl ImageResourcesPlugin {
    /// Resolves image tokens to keys in a named byte provider. Bindings are checked after all plugins are built.
    ///
    /// MIRX FLAT images support mapped storage and random-access readers with checked source boundaries. Mapped CHUNK RAW images retain their direct pixel plane. Other formats can be registered through [`Self::with_loader`].
    pub fn with_source(
        mut self,
        name: &'static str,
        resolve: impl Fn(&str) -> Option<Cow<'static, str>> + 'static,
    ) -> Self {
        self.sources.push(SourceBinding::new(name, resolve));
        self
    }
    /// Empty plugin — no built-in `thumbs_up`, no statics, no loaders.
    /// Caller is responsible for registering every token they reference.
    pub fn empty() -> Self {
        Self {
            bundle_thumbs_up: false,
            ..Self::default()
        }
    }

    pub fn with_values_budget(mut self, budget: MaxSize) -> Self {
        self.values_budget = budget;
        self
    }

    /// Limit the built-in source decoder's shared pixel storage, including pending reads, pinned leases, read state and cache metadata. The default is 128 KiB, allocated on demand; static and mapped RAW pixels do not consume this budget. Resource indexes, source byte blocks and custom loader allocations have separate budgets.
    pub fn with_decode_budget(mut self, bytes: usize) -> Self {
        self.decode_budget = bytes;
        self
    }

    pub fn with_probes_budget(mut self, budget: MaxSize) -> Self {
        self.probes_budget = budget;
        self
    }

    pub fn with_static(
        mut self,
        token: impl Into<Cow<'static, str>>,
        texture: Texture<'static>,
    ) -> Self {
        self.statics.push((token.into(), texture));
        self
    }

    pub fn with_mirx_bytes(
        self,
        token: impl Into<Cow<'static, str>>,
        bytes: &'static [u8],
    ) -> Self {
        self.with_mirx_bytes_options(token, bytes, MirxTextureOptions::new())
    }

    /// Registers MIRX bytes with explicit decode limits and output requirements.
    pub fn with_mirx_bytes_options(
        mut self,
        token: impl Into<Cow<'static, str>>,
        bytes: &'static [u8],
        options: MirxTextureOptions,
    ) -> Self {
        self.mirx_bytes.push((token.into(), bytes, options));
        self
    }

    pub fn with_loader<L>(mut self, configure: L) -> Self
    where
        L: FnOnce(&ResourceManager<Texture<'static>>) + 'static,
    {
        self.loaders.push(Box::new(configure));
        self
    }
}

impl<B, F> Plugin<B, F> for ImageResourcesPlugin
where
    B: Surface,
    F: RendererFactory<B>,
{
    fn build(&mut self, app: &mut App<B, F>) {
        let poll_loaders = !self.loaders.is_empty() || !self.sources.is_empty();
        let manager =
            ResourceManager::<Texture<'static>>::new(self.values_budget, FALLBACK_TEXTURE)
                .with_probes(self.probes_budget, FALLBACK_META);

        if self.bundle_thumbs_up {
            manager.add_static("thumbs_up", IMG_THUMBS_UP);
        }

        for (token, tex) in self.statics.drain(..) {
            manager.add_static(token, tex);
        }
        for (token, bytes, options) in self.mirx_bytes.drain(..) {
            // Errors here mean the user gave us bad bytes during plugin
            // construction; surfacing via panic is the right call —
            // there's no other reasonable place to report it.
            if let Err(e) = manager.add_mirx_bytes_with(token.clone(), bytes, options) {
                panic!("ImageResourcesPlugin: bad mirx bytes for {token:?}: {e:?}");
            }
        }
        for configure in self.loaders.drain(..) {
            configure(&manager);
        }

        app.world.insert_resource(manager);
        if poll_loaders {
            app.add_system(poll_images::system());
        }
    }

    fn name(&self) -> &'static str {
        "ImageResourcesPlugin"
    }

    fn on_start(&mut self, world: &mut World) {
        let images = world
            .resource::<ResourceManager<Texture<'static>>>()
            .expect("image resources installed");
        if self.sources.is_empty() && !images.has_source_bindings() {
            return;
        }
        let sources = world
            .resource::<Rc<AssetSources>>()
            .expect("ImageResourcesPlugin source requires ResourcePlugin")
            .clone();
        if !self.sources.is_empty() {
            images.add_probed_loader(crate::render::texture::SourceMirxLoader::new(
                self.decode_budget,
            ));
        }
        images
            .bind_sources(sources, core::mem::take(&mut self.sources))
            .expect("unknown image byte source");
    }
}

#[crate::system]
fn poll_images(
    images: res!(ResourceManager<Texture<'static>>),
    host: res!(optional ResourceHostWake),
) {
    let parent = host.as_ref().map_or(Waker::noop(), |host| &host.0);
    images.poll_pending(&mut Context::from_waker(parent));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::resource::HasProbe;

    fn rebuild_into_manager(
        plugin: &mut ImageResourcesPlugin,
    ) -> ResourceManager<Texture<'static>> {
        // Bypass App::add_plugin to test plugin construction in isolation.
        let manager =
            ResourceManager::<Texture<'static>>::new(plugin.values_budget, FALLBACK_TEXTURE)
                .with_probes(plugin.probes_budget, FALLBACK_META);
        if plugin.bundle_thumbs_up {
            manager.add_static("thumbs_up", IMG_THUMBS_UP);
        }
        for (token, tex) in plugin.statics.drain(..) {
            manager.add_static(token, tex);
        }
        for (token, bytes, options) in plugin.mirx_bytes.drain(..) {
            manager.add_mirx_bytes_with(token, bytes, options).unwrap();
        }
        for configure in plugin.loaders.drain(..) {
            configure(&manager);
        }
        manager
    }

    fn build_flat_rgb565_2x1() -> &'static [u8] {
        let bytes = mirx::Document::new_flat(mirx::image::ImageAsset::new(
            2,
            1,
            mirx::image::ColorFormat::RGB565,
            4,
            alloc::borrow::Cow::Borrowed(&[0xAA, 0xBB, 0xCC, 0xDD]),
        ))
        .unwrap()
        .finish()
        .unwrap()
        .into_owned();
        Box::leak(bytes.into_boxed_slice())
    }

    #[test]
    fn default_bundles_thumbs_up() {
        let mut plugin = ImageResourcesPlugin::default();
        let manager = rebuild_into_manager(&mut plugin);
        let tex = manager.resolve("thumbs_up");
        assert_eq!(tex.width, 16);
        assert_eq!(tex.format, ColorFormat::RGBA8888);
    }

    #[test]
    fn empty_skips_thumbs_up() {
        let mut plugin = ImageResourcesPlugin::empty();
        let manager = rebuild_into_manager(&mut plugin);
        assert_eq!(manager.probe("thumbs_up"), None);
        assert_eq!(manager.resolve("thumbs_up").extract_meta(), FALLBACK_META);
    }

    #[test]
    fn unknown_token_falls_back_to_checkerboard() {
        let mut plugin = ImageResourcesPlugin::default();
        let manager = rebuild_into_manager(&mut plugin);
        let tex = manager.resolve("nope");
        assert_eq!(tex.extract_meta(), FALLBACK_META);
    }

    #[test]
    fn with_static_adds_token() {
        let extra = IMG_THUMBS_UP;
        let mut plugin = ImageResourcesPlugin::default().with_static("alt_logo", extra);
        let manager = rebuild_into_manager(&mut plugin);
        let tex = manager.resolve("alt_logo");
        assert_eq!(tex.width, 16);
    }

    #[test]
    fn with_mirx_bytes_seeds_probe_and_value() {
        let bytes = build_flat_rgb565_2x1();
        let mut plugin = ImageResourcesPlugin::default().with_mirx_bytes("logo", bytes);
        let manager = rebuild_into_manager(&mut plugin);
        assert_eq!(
            manager.probe("logo"),
            Some(TextureMeta {
                width: 2,
                height: 1,
                format: ColorFormat::RGB565,
            })
        );
        assert_eq!(manager.resolve("logo").width, 2);
    }
}
