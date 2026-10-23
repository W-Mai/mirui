use crate::app::{App, RendererFactory, plugin::Plugin};
use crate::core::resource::{AssetCacheConfig, AssetError, AssetProvider, AssetSources};
use crate::surface::Surface;
use alloc::{boxed::Box, rc::Rc, vec::Vec};
use core::task::{Context, Waker};

/// The owning event loop's notification. Completion threads never access the World.
pub(crate) struct ResourceHostWake(pub Waker);

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
fn retire_sources(sources: res!(Rc<AssetSources>), host: res!(optional ResourceHostWake)) {
    let parent = host.as_ref().map_or(Waker::noop(), |host| &host.0);
    sources.poll_pending(&mut Context::from_waker(parent));
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;
    use crate::app::plugins::ImageResourcesPlugin;
    use crate::core::resource::{
        FormatMatch, LoadContext, LoadError, LoadInput, Loader, ResourceManager,
    };
    use crate::render::texture::{ColorFormat, Texture};
    use alloc::{sync::Arc, task::Wake};
    use core::{
        sync::atomic::{AtomicUsize, Ordering},
        task::Poll,
    };
    use std::sync::Mutex;

    #[derive(Default)]
    struct Completion {
        ready: bool,
        wake: Option<Waker>,
    }

    struct PendingImage(Arc<Mutex<Completion>>);
    impl Loader<Texture<'static>> for PendingImage {
        type State = ();
        fn recognize(&self, _: LoadInput<'_>) -> FormatMatch {
            FormatMatch::Match
        }
        fn poll_load(
            &self,
            _: &mut (),
            cx: &mut LoadContext<'_>,
        ) -> Poll<Result<Texture<'static>, LoadError>> {
            let mut completion = self.0.lock().unwrap();
            if completion.ready {
                static PIXEL: [u8; 4] = [255, 0, 0, 255];
                Poll::Ready(Ok(Texture::expect_static(
                    &PIXEL,
                    1,
                    1,
                    ColorFormat::RGBA8888,
                )))
            } else {
                completion.wake = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }

    #[derive(Default)]
    struct Host(AtomicUsize);
    impl Wake for Host {
        fn wake(self: Arc<Self>) {
            self.wake_by_ref();
        }
        fn wake_by_ref(self: &Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn host_notification_does_not_publish_or_run_systems_while_suspended() {
        let host = Arc::new(Host::default());
        let completion = Arc::new(Mutex::new(Completion::default()));
        let mut app = App::headless(8, 8);
        app.world
            .insert_resource(ResourceHostWake(host.clone().into()));
        let loader = PendingImage(completion.clone());
        app.add_plugin(
            ImageResourcesPlugin::empty().with_loader(|images| images.add_loader(loader)),
        );
        let handle = app
            .world
            .resource::<ResourceManager<Texture<'static>>>()
            .unwrap()
            .load("remote");
        assert_eq!(handle.get().width, 16);
        app.update_tick();
        app.suspend();
        host.0.store(0, Ordering::SeqCst);
        let late = std::thread::spawn(move || {
            let wake = {
                let mut completion = completion.lock().unwrap();
                completion.ready = true;
                completion.wake.take().unwrap()
            };
            for _ in 0..16 {
                wake.wake_by_ref();
            }
            wake
        })
        .join()
        .unwrap();
        assert_eq!(host.0.load(Ordering::SeqCst), 1);
        assert_eq!(handle.get().width, 16, "worker cannot publish");
        app.update_tick();
        assert_eq!(handle.get().width, 16, "suspended app cannot poll loaders");
        app.resume();
        app.update_tick();
        assert_eq!(handle.get().width, 1);
        assert!(
            app.dirty_regions().unwrap().is_empty(),
            "unobserved completion does not paint"
        );
        drop(app);
        late.wake();
    }
}
