use alloc::borrow::Cow;
use alloc::boxed::Box;

use crate::app::plugin::Plugin;
use crate::app::{App, RendererFactory};
use crate::core::cache::MaxSize;
use crate::core::resource::ResourceManager;
use crate::render::texture::{ColorFormat, Texture, TextureMeta};
use crate::surface::Surface;
use crate::ui::widgets::assets::IMG_THUMBS_UP;

/// Default values-cache byte budget. 256 KiB covers a few small icons + a
/// hero-size RGBA8888 texture without touching the LRU on a typical
/// embedded scene.
const DEFAULT_VALUES_BUDGET: MaxSize = MaxSize::Bytes(256 * 1024);

/// Default probes-cache count budget. One entry per registered token is
/// enough for the full screen of widgets even on a busy app.
const DEFAULT_PROBES_BUDGET: MaxSize = MaxSize::Count(1024);

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

static FALLBACK_TEXTURE: Texture<'static> = Texture {
    buf: crate::render::texture::TexBuf::Ref(&CHECKERBOARD_PIXELS),
    width: 16,
    height: 16,
    format: ColorFormat::RGBA8888,
    stride: 64,
    alpha_mode: crate::render::texture::AlphaMode::Opaque,
    transient: false,
};

const FALLBACK_META: TextureMeta = TextureMeta {
    width: 16,
    height: 16,
    format: ColorFormat::RGBA8888,
};

type StaticEntry = (Cow<'static, str>, Texture<'static>);
type MirxEntry = (Cow<'static, str>, &'static [u8]);
type LoaderEntry = Box<dyn FnOnce(&ResourceManager<Texture<'static>>) + 'static>;

/// Inserts a [`ResourceManager<Texture<'static>>`] into the world and seeds
/// it with built-in assets. Use `with_static` / `with_mirx_bytes` /
/// `with_loader` to extend before `app.add_plugin(...)`.
pub struct ImageResourcesPlugin {
    values_budget: MaxSize,
    probes_budget: MaxSize,
    bundle_thumbs_up: bool,
    statics: alloc::vec::Vec<StaticEntry>,
    mirx_bytes: alloc::vec::Vec<MirxEntry>,
    loaders: alloc::vec::Vec<LoaderEntry>,
}

impl Default for ImageResourcesPlugin {
    fn default() -> Self {
        Self {
            values_budget: DEFAULT_VALUES_BUDGET,
            probes_budget: DEFAULT_PROBES_BUDGET,
            bundle_thumbs_up: true,
            statics: alloc::vec::Vec::new(),
            mirx_bytes: alloc::vec::Vec::new(),
            loaders: alloc::vec::Vec::new(),
        }
    }
}

impl ImageResourcesPlugin {
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
        mut self,
        token: impl Into<Cow<'static, str>>,
        bytes: &'static [u8],
    ) -> Self {
        self.mirx_bytes.push((token.into(), bytes));
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
        let manager =
            ResourceManager::<Texture<'static>>::new(self.values_budget, FALLBACK_TEXTURE.clone())
                .with_probes(self.probes_budget, FALLBACK_META);

        if self.bundle_thumbs_up {
            manager.add_static("thumbs_up", IMG_THUMBS_UP.clone());
        }

        for (token, tex) in self.statics.drain(..) {
            manager.add_static(token, tex);
        }
        for (token, bytes) in self.mirx_bytes.drain(..) {
            // Errors here mean the user gave us bad bytes during plugin
            // construction; surfacing via panic is the right call —
            // there's no other reasonable place to report it.
            if let Err(e) = manager.add_mirx_bytes(token.clone(), bytes) {
                panic!("ImageResourcesPlugin: bad mirx bytes for {token:?}: {e:?}");
            }
        }
        for configure in self.loaders.drain(..) {
            configure(&manager);
        }

        app.world.insert_resource(manager);
    }

    fn name(&self) -> &'static str {
        "ImageResourcesPlugin"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::resource::HasProbe;

    fn rebuild_into_manager(
        plugin: &mut ImageResourcesPlugin,
    ) -> ResourceManager<Texture<'static>> {
        // Bypass App::add_plugin to test plugin construction in isolation.
        let manager = ResourceManager::<Texture<'static>>::new(
            plugin.values_budget,
            FALLBACK_TEXTURE.clone(),
        )
        .with_probes(plugin.probes_budget, FALLBACK_META);
        if plugin.bundle_thumbs_up {
            manager.add_static("thumbs_up", IMG_THUMBS_UP.clone());
        }
        for (token, tex) in plugin.statics.drain(..) {
            manager.add_static(token, tex);
        }
        for (token, bytes) in plugin.mirx_bytes.drain(..) {
            manager.add_mirx_bytes(token, bytes).unwrap();
        }
        for configure in plugin.loaders.drain(..) {
            configure(&manager);
        }
        manager
    }

    fn build_flat_rgb565_2x1() -> &'static [u8] {
        let input = mirx::FlatImageInput {
            width: 2,
            height: 1,
            stride: 4,
            format: mirx::ColorFormat::RGB565,
            main: &[0xAA, 0xBB, 0xCC, 0xDD],
            extra: None,
        };
        Box::leak(mirx::encode_flat(&input).into_boxed_slice())
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
        let extra = IMG_THUMBS_UP.clone();
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
