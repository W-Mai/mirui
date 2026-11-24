//! App lifecycle for native mobile windows.

use alloc::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use crate::render::factory::SwRendererFactory;
#[cfg(target_os = "ios")]
use crate::surface::wgpu_surface::prepare_window;
use crate::surface::wgpu_surface::{
    MobileEventFlow, MobileSurface, MobileSurfaceError, MobileWgpuSurface,
    window_event_is_deliverable,
};
use crate::surface::wgpu_upload::{SoftwareUploadConfig, SoftwareUploadSurface};

pub fn wgpu_mobile_host<F, Build>(
    title: impl Into<alloc::string::String>,
    build: Build,
) -> MobileHost<
    MobileWgpuSurface,
    F,
    impl FnOnce(Arc<Window>) -> Result<MobileWgpuSurface, MobileSurfaceError>,
    Build,
>
where
    F: crate::render::factory::RendererFactory<MobileWgpuSurface> + 'static,
    Build: FnOnce(MobileWgpuSurface) -> crate::app::App<MobileWgpuSurface, F> + 'static,
{
    MobileHost::new(title, MobileWgpuSurface::new, build)
}

/// Owns a mirui app inside the platform event loop.
pub struct MobileHost<B, F, Create, Build>
where
    B: MobileSurface,
    F: crate::render::factory::RendererFactory<B>,
    Create: FnOnce(Arc<Window>) -> Result<B, MobileSurfaceError>,
    Build: FnOnce(B) -> crate::app::App<B, F>,
{
    title: alloc::string::String,
    create: Option<Create>,
    build: Option<Build>,
    app: Option<crate::app::App<B, F>>,
    window: Option<Arc<Window>>,
    resource_wake: Option<core::task::Waker>,
}

struct ResourceReady;

struct ResourceLoopWake(winit::event_loop::EventLoopProxy<ResourceReady>);

impl alloc::task::Wake for ResourceLoopWake {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        // Readiness stays in the resource task; a closed host only discards its notification.
        let _ = self.0.send_event(ResourceReady);
    }
}

impl<B, F, Create, Build> MobileHost<B, F, Create, Build>
where
    B: MobileSurface + 'static,
    F: crate::render::factory::RendererFactory<B> + 'static,
    Create: FnOnce(Arc<Window>) -> Result<B, MobileSurfaceError> + 'static,
    Build: FnOnce(B) -> crate::app::App<B, F> + 'static,
{
    pub fn new(title: impl Into<alloc::string::String>, create: Create, build: Build) -> Self {
        Self {
            title: title.into(),
            create: Some(create),
            build: Some(build),
            app: None,
            window: None,
            resource_wake: None,
        }
    }

    #[cfg(target_os = "android")]
    pub fn run_android(mut self, android_app: winit::platform::android::activity::AndroidApp) -> ! {
        use winit::platform::android::EventLoopBuilderExtAndroid;

        let mut builder = EventLoop::<ResourceReady>::with_user_event();
        builder.with_android_app(android_app);
        let event_loop = builder.build().expect("winit Android event loop");
        self.resource_wake = Some(Arc::new(ResourceLoopWake(event_loop.create_proxy())).into());
        event_loop.set_control_flow(ControlFlow::Poll);
        event_loop
            .run_app(&mut self)
            .expect("winit Android application loop");
        unreachable!("mobile event loop returned")
    }

    /// Runs the iOS application on the main thread. The application manifest must declare a single application scene; the host binds each native window before creating or resuming its surface.
    #[cfg(target_os = "ios")]
    pub fn run_ios(mut self) -> ! {
        let event_loop = EventLoop::<ResourceReady>::with_user_event()
            .build()
            .expect("winit iOS event loop");
        self.resource_wake = Some(Arc::new(ResourceLoopWake(event_loop.create_proxy())).into());
        event_loop.set_control_flow(ControlFlow::Poll);
        event_loop
            .run_app(&mut self)
            .expect("winit iOS application loop");
        unreachable!("mobile event loop returned")
    }
}

impl<B, F, Create, Build> ApplicationHandler<ResourceReady> for MobileHost<B, F, Create, Build>
where
    B: MobileSurface + 'static,
    F: crate::render::factory::RendererFactory<B> + 'static,
    Create: FnOnce(Arc<Window>) -> Result<B, MobileSurfaceError> + 'static,
    Build: FnOnce(B) -> crate::app::App<B, F> + 'static,
{
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Poll);
        if let Some(window) = self.window.as_ref()
            && self.app.as_ref().is_some_and(|app| !app.is_suspended())
        {
            window.request_redraw();
            return;
        }
        let window = self.window.clone().unwrap_or_else(|| {
            Arc::new(
                event_loop
                    .create_window(Window::default_attributes().with_title(self.title.clone()))
                    .expect("winit mobile window"),
            )
        });

        #[cfg(target_os = "ios")]
        if let Err(error) = prepare_window(&window) {
            crate::warn!(target: "mirui::surface::wgpu_surface", "mobile window preparation failed: {:?}", error);
            self.suspended(event_loop);
            return;
        }

        if let Some(app) = self.app.as_mut() {
            if let Err(error) = app.backend.resume_window(window.clone()) {
                crate::warn!(target: "mirui::surface::wgpu_surface", "mobile surface resume failed: {:?}", error);
                event_loop.exit();
                return;
            }
            app.resume();
        } else {
            let create = self
                .create
                .take()
                .expect("mobile surface builder consumed once");
            let surface = match create(window.clone()) {
                Ok(surface) => surface,
                Err(error) => {
                    crate::warn!(target: "mirui::surface::wgpu_surface", "mobile surface creation failed: {:?}", error);
                    event_loop.exit();
                    return;
                }
            };
            let build = self.build.take().expect("mobile app builder consumed once");
            let mut app = build(surface);
            app.world
                .insert_resource(crate::app::plugins::resources::ResourceHostWake(
                    self.resource_wake
                        .as_ref()
                        .expect("mobile event loop started")
                        .clone(),
                ));
            self.app = Some(app);
        }
        self.window = Some(window.clone());
        window.request_redraw();
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(app) = self.app.as_mut() {
            app.suspend();
            app.backend.suspend_window();
        }
        #[cfg(not(target_os = "ios"))]
        {
            self.window = None;
        }
        event_loop.set_control_flow(ControlFlow::Wait);
    }

    fn memory_warning(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(app) = self.app.as_mut() {
            app.handle_memory_warning();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let Some(app) = self.app.as_mut() else {
            return;
        };
        if !window_event_is_deliverable(Some(&window.id()), &window_id, app.is_suspended(), &event)
        {
            return;
        }
        let redraw = matches!(event, WindowEvent::RedrawRequested);
        match app.backend.handle_window_event(event) {
            Ok(MobileEventFlow::Continue) => {}
            Ok(MobileEventFlow::Exit) => {
                event_loop.exit();
                return;
            }
            Err(error) => {
                crate::warn!(target: "mirui::surface::wgpu_surface", "mobile surface event failed: {:?}", error);
                event_loop.exit();
                return;
            }
        }
        if redraw {
            app.draw_tick();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(app) = self.app.as_mut() else {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        };
        app.backend.finish_input_batch();
        if app.is_suspended() {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }
        if app.tick_wait_ns().unwrap_or(0) == 0 && app.update_tick() {
            event_loop.exit();
            return;
        }
        if app.is_suspended() {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }
        match app.dirty_regions() {
            Ok(plan) if !plan.is_empty() => {
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            Err(error) => {
                crate::warn!(target: "mirui::surface::wgpu_surface", "mobile drawing preparation failed: {:?}", error)
            }
            _ => {}
        }
        event_loop.set_control_flow(match app.tick_wait_ns() {
            Some(wait) if wait > 0 => ControlFlow::WaitUntil(
                std::time::Instant::now() + core::time::Duration::from_nanos(wait),
            ),
            _ => ControlFlow::Poll,
        });
    }
}

pub fn software_mobile_host<Build>(
    title: impl Into<alloc::string::String>,
    config: SoftwareUploadConfig,
    build: Build,
) -> MobileHost<
    SoftwareUploadSurface,
    SwRendererFactory,
    impl FnOnce(Arc<Window>) -> Result<SoftwareUploadSurface, MobileSurfaceError>,
    Build,
>
where
    Build: FnOnce(SoftwareUploadSurface) -> crate::app::App<SoftwareUploadSurface> + 'static,
{
    MobileHost::new(
        title,
        move |window| SoftwareUploadSurface::new(window, config),
        build,
    )
}
