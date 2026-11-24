#[cfg(all(target_arch = "wasm32", feature = "web-canvas"))]
use super::web_runner;
use super::{App, plugins};
use crate::ecs::{FrameOperation, FramePhase, TimingOutcome};
use crate::input::event::gesture::GestureSystem;
use crate::render::factory::{RendererFactory, SwRendererFactory};
use crate::surface::{InputEvent, Surface};

impl<B: Surface, F: RendererFactory<B>> App<B, F> {
    /// Poll one event
    pub fn poll_event(&mut self) -> Option<InputEvent> {
        self.sync_platform_input();
        self.backend.poll_event()
    }

    pub(super) fn sync_platform_input(&mut self) {
        let unavailable = self.text_input_unavailable();
        let state = crate::ui::text_input::native_state(&mut self.world, unavailable);
        self.backend.set_text_input(state);
        let limit = crate::input::event::runtime::delivery_limits(&self.world)
            .commands
            .map_or(0, |capacity| capacity.record_count());
        for _ in 0..limit {
            use crate::ui::clipboard::ClipboardRequest;
            let Some(request) = crate::ui::clipboard::take_request(&mut self.world) else {
                break;
            };
            let dispatch = match request {
                ClipboardRequest::Write(request) => self.backend.write_clipboard(request),
                ClipboardRequest::Read(request) => self.backend.read_clipboard(request),
                #[cfg(any(test, all(feature = "web-canvas", target_arch = "wasm32")))]
                ClipboardRequest::DeferredRead(request, source) => source(request),
            };
            match dispatch {
                crate::ui::clipboard::ClipboardDispatch::Complete(response) => {
                    crate::ui::clipboard::complete(&mut self.world, &response);
                }
                crate::ui::clipboard::ClipboardDispatch::Pending => break,
            }
        }
    }

    /// Run input, systems, and demand-driven drawing until quit. Transient backends redraw the full frame whenever drawing is needed.
    pub fn run(&mut self) {
        self.ensure_started();
        if let Err(error) = self.render() {
            crate::warn!(target: "mirui::app", "frame render failed: {:?}", error);
        }
        loop {
            if self.tick() {
                return;
            }
        }
    }

    pub(super) fn dispatch_pending_input(&mut self) -> bool {
        if self.quitting {
            return true;
        }
        self.ensure_started();
        let _owner_scope = crate::core::reactive::OwnerGuard::enter(&mut self.world);
        if let Err(error) = self.prepare_text_layout() {
            crate::warn!(target: "mirui::app", "input layout preparation failed: {:?}", error);
        }
        self.sync_surface_geometry(self.backend.viewport());
        if self.suspended {
            // Fan-out plugin on_event while suspended so AutoSuspendOnFocus-style
            // plugins can write SuspendRequest::Resume in response to lifecycle events.
            // Widget dispatch is skipped — suspended means no UI interaction.
            loop {
                let event = self.poll_event();
                self.sync_surface_geometry(self.backend.viewport());
                match event {
                    Some(InputEvent::Quit) => {
                        self.finish_quit();
                        return true;
                    }
                    Some(InputEvent::Clipboard(response)) => {
                        crate::ui::clipboard::complete(&mut self.world, &response);
                        self.flush_event_notifications();
                        self.apply_suspend_request();
                    }
                    Some(event) => {
                        self.notify_raw_input(&event);
                        self.apply_suspend_request();
                        self.flush_event_notifications();
                        if !self.suspended {
                            break;
                        }
                    }
                    None => break,
                }
            }
            self.apply_suspend_request();
            self.flush_event_notifications();
            if self.suspended {
                return false;
            }
        }
        let mut quit = false;

        {
            crate::trace_span!("frame.input");
            loop {
                let event = self.poll_event();
                self.sync_surface_geometry(self.backend.viewport());
                match event {
                    Some(InputEvent::Quit) => {
                        quit = true;
                        break;
                    }
                    Some(InputEvent::Clipboard(response)) => {
                        crate::ui::clipboard::complete(&mut self.world, &response);
                        self.flush_event_notifications();
                        self.apply_suspend_request();
                    }
                    Some(event) => {
                        // Simulation owns interactive input, not host lifecycle requests.
                        let sim_running = self
                            .world
                            .resource::<crate::input::event::sim::SimTimeline>()
                            .is_some_and(|t| t.is_running());
                        if sim_running && event.is_user_input() {
                            continue;
                        }
                        let consumed = self.notify_raw_input(&event);
                        if !event.is_user_input()
                            && self
                                .world
                                .resource::<crate::app::lifecycle::SuspendRequest>()
                                .is_some()
                        {
                            self.apply_suspend_request();
                        } else if (!consumed || !event.is_user_input()) && !self.suspended {
                            let info = self.backend.display_info();
                            let (lw, lh) = (info.width, info.height);
                            let now_ms = (self.clock_ns() / 1_000_000) as u32;
                            crate::input::event::deliver_input(
                                &mut self.world,
                                self.root,
                                &event,
                                now_ms,
                                lw,
                                lh,
                            );
                        }
                        self.flush_event_notifications();
                        self.apply_suspend_request();
                    }
                    None => break,
                }
            }
            self.dispatch_gestures();

            if quit {
                self.finish_quit();
                return true;
            }
            self.flush_event_notifications();
        }
        self.sync_platform_input();
        false
    }

    /// One frame. Returns `true` on `Quit` (after `on_quit` hooks).
    pub fn tick(&mut self) -> bool {
        let quit = self.with_timing(FrameOperation::Tick, |app| {
            if app.update_tick_inner() {
                return true;
            }
            if !app.suspended {
                app.draw_tick_inner();
            }
            app.maintain_tick_inner();
            false
        });
        if quit {
            return true;
        }
        plugins::frame_rate::complete(&mut self.world, true);
        #[cfg(feature = "std")]
        if self.suspended {
            std::thread::sleep(core::time::Duration::from_millis(50));
        }
        false
    }

    #[cfg(any(
        test,
        all(feature = "wgpu", any(target_os = "android", target_os = "ios"))
    ))]
    pub(crate) fn update_tick(&mut self) -> bool {
        let quit = self.with_timing(FrameOperation::Update, |app| {
            if app.update_tick_inner() {
                return true;
            }
            app.maintain_tick_inner();
            false
        });
        if !quit {
            plugins::frame_rate::complete(&mut self.world, false);
        }
        quit
    }

    fn update_tick_inner(&mut self) -> bool {
        let frame_start = self.timing_clock_ns();
        if self.dispatch_pending_input() {
            self.record_phase(FramePhase::Input, frame_start, self.timing_clock_ns());
            return true;
        }

        if self.suspended {
            self.record_phase(FramePhase::Input, frame_start, self.timing_clock_ns());
            return false;
        }
        let _owner_scope = crate::core::reactive::OwnerGuard::enter(&mut self.world);
        let now_ms = (self.clock_ns() / 1_000_000) as u32;
        if let Some(gs) = self.world.resource_mut::<GestureSystem>() {
            gs.recognizer.check_long_press(now_ms, &mut gs.events);
        }
        if self
            .world
            .resource::<GestureSystem>()
            .is_some_and(|system| !system.events.buffer.is_empty())
        {
            self.dispatch_gestures();
        }
        self.flush_event_notifications();

        let input_end = self.timing_clock_ns();

        let mut systems_failed = false;
        {
            crate::trace_span!("frame.systems");
            if let Err(error) = self.systems.run_all(&mut self.world) {
                systems_failed = true;
                crate::warn!(target: "mirui::app", "scheduled system failed: {:?}", error);
            }
        }
        self.snapshot_system_perf();
        self.flush_event_notifications();
        let systems_end = self.timing_clock_ns();

        self.record_phase(FramePhase::Input, frame_start, input_end);
        self.record_phase(FramePhase::Systems, input_end, systems_end);
        self.timing_outcome(if systems_failed {
            TimingOutcome::Failed
        } else {
            TimingOutcome::Completed
        });
        false
    }

    #[cfg(any(
        test,
        all(feature = "wgpu", any(target_os = "android", target_os = "ios"))
    ))]
    pub(crate) fn draw_tick(&mut self) {
        self.with_timing(FrameOperation::Draw, |app| {
            app.draw_tick_inner();
            app.maintain_tick_inner();
        });
    }

    fn draw_tick_inner(&mut self) {
        if let Err(error) = self.render_dirty()
            && error != crate::render::RenderError::SurfaceUnavailable
            && error != crate::render::RenderError::ResourcePending
        {
            crate::warn!(target: "mirui::app", "dirty frame render failed: {:?}", error);
        }
    }

    fn maintain_tick_inner(&mut self) {
        self.flush_event_notifications();
        if let Err(error) = self.backend.frame_end()
            && error != crate::render::RenderError::SurfaceUnavailable
        {
            self.timing_outcome(TimingOutcome::Failed);
            crate::warn!(target: "mirui::app", "surface maintenance failed: {:?}", error);
        }

        self.apply_suspend_request();
    }

    #[cfg(any(
        test,
        all(feature = "wgpu", any(target_os = "android", target_os = "ios"))
    ))]
    pub(crate) fn tick_wait_ns(&self) -> Option<u64> {
        plugins::frame_rate::remaining(&self.world)
    }

    fn snapshot_system_perf(&mut self) {
        // PerfReportPlugin::build is the opt-in path for this resource.
        if self
            .world
            .resource::<crate::app::plugins::perf_report::SystemPerfSnapshot>()
            .is_none()
        {
            return;
        }

        let want_reset = self
            .world
            .resource::<crate::app::plugins::perf_report::PerfResetFlag>()
            .map(|f| f.0)
            .unwrap_or(false);
        if want_reset {
            self.systems.reset_perf();
            self.world
                .insert_resource(crate::app::plugins::perf_report::PerfResetFlag(false));
        }

        let snap = self
            .world
            .resource_mut::<crate::app::plugins::perf_report::SystemPerfSnapshot>()
            .expect("checked above");
        snap.entries.clear();
        for s in self.systems.iter() {
            let avg = if s.call_count == 0 {
                0
            } else {
                (s.total_us / s.call_count as u64) as u32
            };
            snap.entries
                .push(crate::app::plugins::perf_report::SystemStat {
                    name: s.name,
                    priority: s.priority(),
                    last_us: s.last_us,
                    avg_us: avg,
                    call_count: s.call_count,
                });
        }
    }

    /// Consume into a [`Runner`].
    pub fn into_runner(self) -> Runner<B, F> {
        Runner { app: self }
    }
}

/// Owns an [`App`] and drives [`tick`][App::tick] from a non-blocking
/// loop. Native: `Runner::run_blocking`. Wasm: `Runner::start_animation_frame`.
pub struct Runner<B: Surface, F: RendererFactory<B> = SwRendererFactory> {
    app: App<B, F>,
}

impl<B: Surface, F: RendererFactory<B>> Runner<B, F> {
    /// `App::run` with `-> !`.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn run_blocking(mut self) -> ! {
        self.app.ensure_started();
        if let Err(error) = self.app.render() {
            crate::warn!(target: "mirui::app", "frame render failed: {:?}", error);
        }
        loop {
            if self.app.tick() {
                break;
            }
        }
        #[cfg(feature = "std")]
        {
            std::process::exit(0)
        }
        // no_std MCUs never return from main; spin.
        #[cfg(not(feature = "std"))]
        loop {
            core::hint::spin_loop()
        }
    }

    /// Drive `App::tick` from `requestAnimationFrame` and return so the
    /// wasm-bindgen `start` function can yield to the browser.
    #[cfg(all(target_arch = "wasm32", feature = "web-canvas"))]
    pub fn start_animation_frame(self)
    where
        B: 'static,
        F: 'static,
    {
        use alloc::rc::Rc;
        use core::cell::RefCell;
        Self::drive_animation_frame(Rc::new(RefCell::new(Some(self.app))));
    }

    /// Drive a shared, swappable `App` from `requestAnimationFrame`. The
    /// cell may hold `None` while a host swaps the `App` (e.g. the web
    /// gallery rebuilding for a new demo); those frames are skipped and
    /// the loop keeps rescheduling. Browser input is also delivered during DOM event bubbling; scheduled systems and rendering remain on animation frames.
    #[cfg(all(target_arch = "wasm32", feature = "web-canvas"))]
    pub fn drive_animation_frame(app: alloc::rc::Rc<core::cell::RefCell<Option<App<B, F>>>>)
    where
        B: 'static,
        F: 'static,
    {
        use alloc::rc::Rc;
        use core::cell::RefCell;
        use wasm_bindgen::JsCast;
        use wasm_bindgen::closure::Closure;

        type FrameCallback = Closure<dyn FnMut()>;
        let holder: Rc<RefCell<Option<FrameCallback>>> = Rc::new(RefCell::new(None));
        let holder_inner = holder.clone();
        let mut input = Some(web_runner::InputBoundary::new(&app));

        *holder.borrow_mut() = Some(Closure::<dyn FnMut()>::new(move || {
            if let Some(app) = app.borrow_mut().as_mut() {
                if app.tick() {
                    input.take();
                    return;
                }
            }
            let window = web_sys::window().expect("no global `window`");
            window
                .request_animation_frame(
                    holder_inner
                        .borrow()
                        .as_ref()
                        .expect("RAF closure")
                        .as_ref()
                        .unchecked_ref(),
                )
                .expect("requestAnimationFrame");
        }));

        let window = web_sys::window().expect("no global `window`");
        window
            .request_animation_frame(
                holder
                    .borrow()
                    .as_ref()
                    .expect("RAF closure")
                    .as_ref()
                    .unchecked_ref(),
            )
            .expect("requestAnimationFrame");
    }

    #[cfg(all(target_arch = "wasm32", not(feature = "web-canvas")))]
    pub fn start_animation_frame(self) -> ! {
        let _ = self.app;
        unimplemented!("Runner::start_animation_frame requires the `web-canvas` feature on wasm32");
    }
}
