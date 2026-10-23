use crate::app::{App, RendererFactory, plugin::Plugin};
use crate::core::resource::{AssetCacheConfig, AssetError, AssetProvider, AssetSources};
use crate::surface::Surface;
use alloc::{boxed::Box, rc::Rc, vec::Vec};
use core::task::{Context, Waker};

type Registration = Box<dyn FnOnce(&mut AssetSources) -> Result<(), AssetError>>;

/// Installs named byte providers with shared bounded read storage.
///
/// **Inserts**
/// - resource: byte-source registry when providers are configured
/// - system: retires submitted reads on the owning thread, including without a UI root
pub struct ResourcePlugin {
    config: AssetCacheConfig,
    providers: Vec<Registration>,
}
impl Default for ResourcePlugin {
    fn default() -> Self {
        Self::new()
    }
}
impl ResourcePlugin {
    pub fn new() -> Self {
        Self {
            config: AssetCacheConfig::new(),
            providers: Vec::new(),
        }
    }
    pub fn with_cache(mut self, config: AssetCacheConfig) -> Self {
        self.config = config;
        self
    }
    pub fn with_source(mut self, name: &'static str, provider: impl AssetProvider) -> Self {
        self.providers
            .push(Box::new(move |sources| sources.register(name, provider)));
        self
    }
}
impl<B: Surface, F: RendererFactory<B>> Plugin<B, F> for ResourcePlugin {
    fn build(&mut self, app: &mut App<B, F>) {
        if self.providers.is_empty() {
            return;
        }
        assert!(
            app.world.resource::<Rc<AssetSources>>().is_none(),
            "byte providers are already installed"
        );
        let mut sources = AssetSources::new(self.config).expect("invalid byte-source capacity");
        for register in self.providers.drain(..) {
            register(&mut sources).expect("invalid byte-source registration");
        }
        app.world.insert_resource(Rc::new(sources));
        app.add_system(retire_sources::system());
    }
    fn name(&self) -> &'static str {
        "ResourcePlugin"
    }
}

#[crate::system]
fn retire_sources(sources: res!(Rc<AssetSources>)) {
    sources.poll_pending(&mut Context::from_waker(Waker::noop()));
}
