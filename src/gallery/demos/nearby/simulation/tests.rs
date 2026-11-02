use super::*;
use crate::render::texture::{ColorFormat, Texture};
use crate::surface::framebuf::FramebufSurface;
use crate::surface::{
    DisplayInfo, FramebufferAccess, SurfaceSizeControl, SurfaceSizeError, SurfaceSizeHost,
};
use crate::types::PhysicalRect;
use crate::ui::ComputedRect;

fn flush(_: &[u8], _: PhysicalRect) {}
type Pixels = FramebufSurface<fn(&[u8], PhysicalRect)>;

struct Resizable {
    size: SurfaceSizeHost,
    pixels: Pixels,
    redraw: bool,
    reject: bool,
}

impl Resizable {
    fn new(width: u16, height: u16) -> Self {
        Self {
            size: SurfaceSizeHost::new(width, height),
            pixels: FramebufSurface::with_format(width, height, ColorFormat::RGBA8888, flush),
            redraw: true,
            reject: false,
        }
    }
    fn resize(&mut self, width: u16, height: u16) {
        self.pixels = FramebufSurface::with_format(width, height, ColorFormat::RGBA8888, flush);
        self.redraw = true;
        self.size.observe_size(width, height);
    }
}

impl crate::core::cache::InspectCaches for Resizable {}
impl Surface for Resizable {
    fn display_info(&self) -> DisplayInfo {
        self.pixels.display_info()
    }
    fn size_control(&mut self) -> Option<SurfaceSizeControl> {
        Some(self.size.control())
    }
    fn flush(&mut self, area: PhysicalRect) -> Result<(), crate::render::RenderError> {
        self.pixels.flush(area)
    }
    fn needs_redraw(&self) -> bool {
        self.redraw
    }
    fn end_flush(&mut self) -> Result<(), crate::render::RenderError> {
        self.redraw = false;
        Ok(())
    }
    fn poll_event(&mut self) -> Option<crate::surface::InputEvent> {
        if let Some(request) = self.size.take_request() {
            if self.reject {
                self.size
                    .complete(request, Err(SurfaceSizeError::Restricted));
            } else {
                let (width, height) = request.size();
                self.resize(width, height);
                self.size.complete(request, Ok((width, height)));
            }
        }
        None
    }
}
impl FramebufferAccess for Resizable {
    fn framebuffer(&mut self) -> Texture<'_> {
        self.pixels.framebuffer()
    }
}

fn fixture(width: u16, height: u16) -> (App<Resizable>, super::super::model::DisplayStateHandle) {
    let mut app = App::new(Resizable::new(width, height));
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    let state = super::super::setup_app(&mut app, root);
    app.world.remove_resource::<crate::ecs::MonoClock>();
    app.add_resource(crate::ecs::DeltaTimeMs(0));
    for _ in 0..3 {
        app.tick();
    }
    app.prepare_text_layout().unwrap();
    app.render().unwrap();
    (app, state)
}

fn content(app: &App<Resizable>) -> Rect {
    let entity = app.world.find_by_id("nearby_demo_content").unwrap();
    app.world.get::<ComputedRect>(entity).unwrap().0
}

fn inspector_visible(app: &App<Resizable>) -> bool {
    app.world
        .find_by_id("nearby_simulation")
        .is_some_and(|entity| !crate::ui::branch::is_hidden_in_tree(&app.world, entity))
}

#[test]
fn inspector_docks_only_after_host_acknowledgement_and_preserves_demo_pixels_and_entities() {
    for width in [360_u16, 640] {
        let (mut app, state) = fixture(width, 480);
        let shelf = app.world.find_by_id("nearby_shelf").unwrap();
        let original = content(&app);
        let before = app
            .backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .bytes()
            .to_vec();
        state.toggle_controls();
        app.prepare_text_layout().unwrap();
        assert_eq!(content(&app), original);
        assert!(!inspector_visible(&app));
        app.backend.poll_event();
        app.prepare_text_layout().unwrap();
        app.render_dirty().unwrap();
        assert_eq!(app.backend.viewport().logical_size(), (width + 300, 480));
        assert_eq!(content(&app), Rect::new(300, 0, width, 480));
        assert_eq!(app.world.find_by_id("nearby_shelf"), Some(shelf));
        let pixels = app
            .backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .bytes()
            .to_vec();
        for row in 0..480_usize {
            let original_start = row * usize::from(width) * 4;
            let docked_start = (row * usize::from(width + 300) + 300) * 4;
            let expected = &before[original_start..original_start + usize::from(width) * 4];
            let actual = &pixels[docked_start..docked_start + usize::from(width) * 4];
            if let Some((x, (expected, actual))) = expected
                .chunks_exact(4)
                .zip(actual.chunks_exact(4))
                .enumerate()
                .find(|(_, (a, b))| a != b)
            {
                panic!("main pixel ({x}, {row}): {expected:?} != {actual:?}");
            }
        }
        state.toggle_controls();
        app.tick();
        assert_eq!(app.backend.viewport().logical_size(), (width, 480));
        assert_eq!(content(&app), original);
        assert_eq!(app.world.find_by_id("nearby_shelf"), Some(shelf));
    }
}

#[test]
fn inspector_coalesces_fast_reversals_and_closes_to_the_manually_resized_demo_size() {
    let (mut app, state) = fixture(640, 480);
    state.toggle_controls();
    app.backend.poll_event();
    state.toggle_controls();
    state.toggle_controls();
    app.tick();
    assert_eq!(content(&app), Rect::new(300, 0, 640, 480));
    app.backend.resize(1100, 600);
    app.tick();
    assert_eq!(content(&app), Rect::new(300, 0, 800, 600));
    state.toggle_controls();
    app.tick();
    assert_eq!(app.backend.viewport().logical_size(), (800, 600));
    assert_eq!(content(&app), Rect::new(0, 0, 800, 600));
}

#[test]
fn rejected_host_expansion_never_overlays_or_shrinks_the_demo() {
    let (mut app, state) = fixture(640, 480);
    app.backend.reject = true;
    state.toggle_controls();
    for _ in 0..3 {
        app.tick();
    }
    assert_eq!(app.backend.viewport().logical_size(), (640, 480));
    assert_eq!(content(&app), Rect::new(0, 0, 640, 480));
    assert!(!inspector_visible(&app));
}

#[test]
fn docked_disabled_replies_do_not_block_scrolling_or_network_simulation() {
    let (mut app, state) = fixture(640, 480);
    state.toggle_controls();
    app.tick();
    let options = app.world.find_by_id("nearby_simulation_options").unwrap();
    let area = app.world.get::<ComputedRect>(options).unwrap().0;
    for (dy, y) in [(-200, area.y + Fixed::ONE), (100, area.y + area.h / 2)] {
        let root = app.root;
        crate::input::event::dispatch_input(
            &mut app.world,
            root,
            &crate::surface::InputEvent::Wheel {
                dx: Fixed::ZERO,
                dy: Fixed::from_int(dy),
                x: area.x + area.w / 2,
                y,
            },
            1,
            940,
            480,
        );
        app.render_dirty().unwrap();
    }
    assert!(app.world.get::<ScrollOffset>(options).unwrap().y < Fixed::from_int(200));
    state.run_simulation(Route::NetworkJoin, Scenario::Normal);
    state.simulate(Action::Advance);
    state.simulate(Action::Reject);
    app.tick();
    assert_eq!(state.presentation().phase, Phase::Failed);
    let partial = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    app.render().unwrap();
    assert_eq!(
        partial,
        app.backend.framebuffer().read_pixels().unwrap().bytes()
    );
}
