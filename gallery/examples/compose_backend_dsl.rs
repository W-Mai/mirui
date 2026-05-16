//! End-to-end wiring of compose_backend!, App generics, and plugins:
//!
//! - the scene (banner + 8 drifting Images) is declared with `ui!`
//! - `HybridFactory` routes blit/clear through a Logging wrapper via
//!   `compose_backend!`, everything else through SwRenderer
//! - `drift_system` moves each Image along a sine path
//! - `StdInstantClockPlugin` + `FpsSummaryPlugin` print render timing

use std::cell::RefCell;
use std::rc::Rc;

use mirui::app::{App, RendererFactory};
use mirui::components::assets::*;
use mirui::components::image::Image;
use mirui::draw::canvas::Canvas;
use mirui::draw::path::Path;
use mirui::draw::sw::SwRenderer;
use mirui::draw::texture::Texture;
use mirui::ecs::World;
use mirui::layout::*;
use mirui::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};
use mirui::surface::sdl::SdlSurface;
use mirui::types::{Color, Dimension, Fixed, Point, Rect, Viewport};
use mirui::widget::builder::WidgetBuilder;
use mirui_macros::{compose_backend, ui};

const W: u16 = 480;
const H: u16 = 320;

/// Wraps any Canvas and counts every method call on a shared
/// Rc<RefCell<u32>>, so the counter stays readable after App takes
/// ownership of this instance.
struct Logging<B: Canvas> {
    inner: B,
    calls: Rc<RefCell<u32>>,
}

impl<B: Canvas> Canvas for Logging<B> {
    fn fill_path(&mut self, path: &Path, clip: &Rect, color: &Color, opa: u8) {
        self.inner.fill_path(path, clip, color, opa);
    }
    fn stroke_path(&mut self, path: &Path, clip: &Rect, width: Fixed, color: &Color, opa: u8) {
        self.inner.stroke_path(path, clip, width, color, opa);
    }
    fn blit(&mut self, src: &Texture, src_rect: &Rect, dst: Point, dst_size: Point, clip: &Rect) {
        *self.calls.borrow_mut() += 1;
        self.inner.blit(src, src_rect, dst, dst_size, clip);
    }
    fn clear(&mut self, area: &Rect, color: &Color) {
        *self.calls.borrow_mut() += 1;
        self.inner.clear(area, color);
    }
    fn draw_label(&mut self, pos: &Point, text: &[u8], clip: &Rect, color: &Color, opa: u8) {
        self.inner.draw_label(pos, text, clip, color, opa);
    }
    fn flush(&mut self) {
        self.inner.flush();
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
        clear => gpu,
    }
}

/// Factory that builds a fresh Hybrid each frame. Holds a Vec for the gpu
/// side's framebuffer + the shared counter Rc.
struct HybridFactory {
    gpu_fb: Vec<u8>,
    width: u16,
    height: u16,
    calls: Rc<RefCell<u32>>,
}

impl HybridFactory {
    fn new(width: u16, height: u16, calls: Rc<RefCell<u32>>) -> Self {
        Self {
            gpu_fb: vec![0u8; width as usize * height as usize * 4],
            width,
            height,
            calls,
        }
    }
}

impl<B: mirui::surface::FramebufferAccess> RendererFactory<B> for HybridFactory {
    type Renderer<'a>
        = Hybrid<SwRenderer<'a>, Logging<SwRenderer<'a>>>
    where
        Self: 'a,
        B: 'a;

    fn make<'a>(&'a mut self, backend: &'a mut B, transform: &Viewport) -> Self::Renderer<'a> {
        let tex = backend.framebuffer();
        let mut sw = SwRenderer::new(tex);
        sw.viewport = *transform;
        let gpu_tex = Texture::new(&mut self.gpu_fb, self.width, self.height, tex_format(&sw));
        let mut gpu_inner = SwRenderer::new(gpu_tex);
        gpu_inner.viewport = *transform;
        let gpu = Logging {
            inner: gpu_inner,
            calls: Rc::clone(&self.calls),
        };
        Hybrid { sw, gpu }
    }
}

/// Read the ColorFormat from an already-constructed SwRenderer so the gpu
/// side framebuffer matches the sw side byte layout without hard-coding.
fn tex_format(sw: &SwRenderer<'_>) -> mirui::draw::texture::ColorFormat {
    sw.target.format
}

struct Drift {
    t: f32,
    start_x: Fixed,
    start_y: Fixed,
    speed: f32,
    amplitude: Fixed,
}

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
        mirui::widget::set_position(world, e, new_x, new_y);
    }
}

fn main() {
    let backend = SdlSurface::new("mirui - compose_backend DSL demo", W, H);

    let calls = Rc::new(RefCell::new(0u32));
    let factory = HybridFactory::new(
        backend.scale_factor().to_int() as u16 * W,
        backend.scale_factor().to_int() as u16 * H,
        Rc::clone(&calls),
    );

    let mut app = App::with_factory(backend, factory);
    app.add_system(drift_system);

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

        scene (grow: 1.0) {
            banner (
                bg_color: Color::rgb(88, 166, 255),
                height: 40,
                text: "compose_backend DSL demo",
                border_radius: 6
            ) {}
            walk drifters.iter() with d {
                drifter (
                    position: Position::Absolute,
                    left: d.0,
                    top: d.1,
                    width: iw,
                    height: ih,
                    image: Image::new(&IMG_THUMBS_UP)
                ) [
                    Drift {
                        t: 0.0,
                        start_x: Fixed::from_int(d.0),
                        start_y: Fixed::from_int(d.1),
                        speed: d.2,
                        amplitude: Fixed::from_int(60),
                    },
                ] {}
            }
        }
    };

    app.set_root(root);

    // StdInstantClockPlugin feeds the post_render hook real ns via
    // std::Instant; FpsSummaryPlugin consumes those ns and prints avg
    // render cost every 60 frames.
    app.add_plugin(StdInstantClockPlugin::default())
        .add_plugin(FpsSummaryPlugin::default());

    app.run();

    eprintln!(
        "[final] Logging (blit+clear) routed over full session: {}",
        calls.borrow()
    );
}
