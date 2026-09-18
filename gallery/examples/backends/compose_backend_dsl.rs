//! End-to-end wiring of compose_backend!, App generics, and plugins:
//!
//! - the scene (banner + 8 drifting Images) is declared with `ui!`
//! - `HybridFactory` routes blits through a logging engine on one target
//! - `drift_system` moves each Image along a sine path
//! - `StdInstantClockPlugin` + `FpsSummaryPlugin` print render timing

use mirui::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use mirui::app::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};
use mirui::app::{App, RendererFactory};
use mirui::render::engine::RenderEngine;
use mirui::render::renderer::{DrawRequest, RenderError, Renderer};
use mirui::render::sw::SwRenderer;
use mirui::surface::sdl::SdlSurface;
use mirui::types::{Color, Dimension, Fixed, Viewport};
use mirui::ui::widgets::assets::*;
use mirui::ui::widgets::{Image, ParagraphStyle, Text};
use mirui_macros::{compose_backend, ui};

const W: u16 = 480;
const H: u16 = 320;

struct Logging {
    calls: Rc<RefCell<u32>>,
}

impl<T: Renderer> RenderEngine<T> for Logging {
    fn submit(&mut self, target: &mut T, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        *self.calls.borrow_mut() += 1;
        target.submit(request)
    }
}

compose_backend! {
    pub struct Hybrid {
        sw: SwRenderer,
        gpu: Logging,
    }
    route {
        default => sw,
        blit => gpu,
    }
}

struct HybridFactory {
    calls: Rc<RefCell<u32>>,
}

impl HybridFactory {
    fn new(calls: Rc<RefCell<u32>>) -> Self {
        Self { calls }
    }
}

impl<B: mirui::surface::FramebufferAccess> RendererFactory<B> for HybridFactory {
    type Renderer<'a>
        = Hybrid<SwRenderer<'a>, Logging>
    where
        Self: 'a,
        B: 'a;

    fn make<'a>(&'a mut self, backend: &'a mut B, transform: &Viewport) -> Self::Renderer<'a> {
        let tex = backend.framebuffer();
        let mut sw = SwRenderer::new(tex);
        sw.viewport = *transform;
        let gpu = Logging {
            calls: Rc::clone(&self.calls),
        };
        Hybrid::new(sw, gpu)
    }
}

struct Drift {
    t: f32,
    start_x: Fixed,
    start_y: Fixed,
    speed: f32,
    amplitude: Fixed,
}

#[mirui::system]
fn drift_system(world: &mut World) {
    let mut buf = Vec::new();
    world.query::<Drift>().collect_into(&mut buf);
    for e in buf {
        let (new_x, new_y) = {
            let Some(d) = world.get_mut::<Drift>(e) else {
                continue;
            };
            d.t += 0.016;
            let ox = Fixed::from_f32((d.t * d.speed).sin()) * d.amplitude;
            let oy =
                Fixed::from_f32((d.t * d.speed * 0.7).cos()) * d.amplitude * Fixed::from_f32(0.5);
            (d.start_x + ox, d.start_y + oy)
        };
        mirui::ui::set_position(world, e, new_x, new_y);
    }
}

fn main() {
    let backend = SdlSurface::new("mirui - compose_backend DSL demo", W, H);

    let calls = Rc::new(RefCell::new(0u32));
    let factory = HybridFactory::new(Rc::clone(&calls));

    let mut app = App::with_factory(backend, factory);
    app.with_default_widgets();
    app.add_system(mirui::ecs::System::new(
        "drift",
        mirui::ecs::run_order::NORMAL,
        drift_system,
    ));

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(W as i32),
            height: Dimension::px(H as i32),
            ..Default::default()
        })
        .id();

    let iw = IMG_THUMBS_UP.width as i32;
    let ih = IMG_THUMBS_UP.height as i32;
    let drifters: [(i32, i32, f32); 8] = [
        (40, 80, 0.80),
        (140, 80, 1.05),
        (240, 80, 1.30),
        (340, 80, 1.55),
        (40, 180, 1.80),
        (140, 180, 2.05),
        (240, 180, 2.30),
        (340, 180, 2.55),
    ];

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        View (grow: 1.0) {
            View (
                bg_color: Color::rgb(88, 166, 255),
                height: 40,
                border_radius: 6
            ) {
                Text ("compose_backend DSL demo", paragraph: ParagraphStyle::label())
            }
            walk drifters.iter() with d {
                View (
                    position: Position::Absolute,
                    left: d.0,
                    top: d.1,
                    width: iw,
                    height: ih,
                    image: Image::new("thumbs_up")
                ) [
                    Drift {
                        t: 0.0,
                        start_x: Fixed::from_int(d.0),
                        start_y: Fixed::from_int(d.1),
                        speed: d.2,
                        amplitude: Fixed::from_int(60),
                    },
                ]
            }
        }
    };

    app.set_root(root);

    // StdInstantClockPlugin feeds the post_render hook real ns via
    // std::Instant; FpsSummaryPlugin consumes those ns and prints avg
    // render cost every 60 frames.
    app.add_plugin(StdInstantClockPlugin::default())
        .add_plugin(FpsSummaryPlugin::default())
        .add_plugin(mirui::app::plugins::ImageResourcesPlugin::default());

    app.run();

    eprintln!("[final] routed blits: {}", calls.borrow());
}
