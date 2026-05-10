//! compose_backend! DSL variant. Widgets are built with the `ui!` macro,
//! a system moves an Image entity across the screen each frame, and the
//! Hybrid backend (sw for paths + Logging for blits/clears) feeds
//! render_system directly. Every Image visible ends up as a DrawCommand::Blit,
//! which routes through Logging and bumps the counter — stderr shows how
//! many blits per second actually took the 'gpu' path.

use std::cell::RefCell;

use mirui::backend::Backend;
use mirui::backend::sdl::SdlBackend;
use mirui::components::assets::*;
use mirui::components::image::Image;
use mirui::draw::backend::DrawBackend;
use mirui::draw::path::Path;
use mirui::draw::sw_backend::SwDrawBackend;
use mirui::draw::texture::Texture;
use mirui::ecs::World;
use mirui::layout::*;
use mirui::types::{Color, Dimension, Fixed, Point, Rect};
use mirui::widget::Style;
use mirui::widget::builder::WidgetBuilder;
use mirui::widget::render_system;
use mirui_macros::{compose_backend, ui};

const W: u16 = 480;
const H: u16 = 320;

struct Logging<B: DrawBackend> {
    inner: B,
    calls: RefCell<u32>,
}

impl<B: DrawBackend> Logging<B> {
    fn new(inner: B) -> Self {
        Self {
            inner,
            calls: RefCell::new(0),
        }
    }
}

impl<B: DrawBackend> DrawBackend for Logging<B> {
    fn fill_path(&mut self, path: &Path, clip: &Rect, color: &Color, opa: u8) {
        self.inner.fill_path(path, clip, color, opa);
    }
    fn stroke_path(&mut self, path: &Path, clip: &Rect, width: Fixed, color: &Color, opa: u8) {
        self.inner.stroke_path(path, clip, width, color, opa);
    }
    fn blit(&mut self, src: &Texture, src_rect: &Rect, dst: Point, clip: &Rect) {
        *self.calls.borrow_mut() += 1;
        self.inner.blit(src, src_rect, dst, clip);
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
        sw: SwDrawBackend,
        gpu: Logging,
    }
    route {
        default => sw,
        blit => gpu,
        clear => gpu,
    }
}

struct Drift {
    t: f32,
    start_x: Fixed,
    start_y: Fixed,
    speed: f32,
    amplitude: Fixed,
}

fn drift_system(world: &mut World) {
    let mut buf = alloc::vec::Vec::new();
    world.query::<Drift>().collect_into(&mut buf);
    for e in buf {
        let (start_x, start_y, offset_x, offset_y) = {
            let Some(d) = world.get_mut::<Drift>(e) else {
                continue;
            };
            d.t += 0.016;
            let offset_x = Fixed::from_f32((d.t * d.speed).sin()) * d.amplitude;
            let offset_y =
                Fixed::from_f32((d.t * d.speed * 0.7).cos()) * d.amplitude * Fixed::from_f32(0.5);
            (d.start_x, d.start_y, offset_x, offset_y)
        };
        if let Some(style) = world.get_mut::<Style>(e) {
            style.layout.left = Dimension::Px(start_x + offset_x);
            style.layout.top = Dimension::Px(start_y + offset_y);
        }
    }
}

extern crate alloc;

fn main() {
    let mut backend = SdlBackend::new("mirui - compose_backend DSL demo", W, H);

    let mut world = World::new();
    let root = WidgetBuilder::new(&mut world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(W as i32),
            height: Dimension::px(H as i32),
            ..Default::default()
        })
        .id();

    // 8 drifters in a 4×2 grid, each with its own sine speed. The whole tree
    // is built through ui! — banner widget plus a walk over drifters. Each
    // drifter attaches a Drift enchant so drift_system can move it.
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
            world: &mut world
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

    let info = backend.display_info();
    let mut gpu_fb = alloc::vec![0u8; (info.width as usize) * (info.height as usize) * 4];

    let mut last_summary = std::time::Instant::now();
    let mut frame: u64 = 0;

    loop {
        drift_system(&mut world);

        let info = backend.display_info();
        let fb_slice = backend.framebuffer();
        let sw_tex = Texture::new(fb_slice, info.width, info.height, info.format);
        let sw = SwDrawBackend::new(sw_tex);
        let gpu_tex = Texture::new(&mut gpu_fb, info.width, info.height, info.format);
        let gpu = Logging::new(SwDrawBackend::new(gpu_tex));
        let mut hybrid = Hybrid { sw, gpu };

        hybrid.clear(
            &Rect::new(0, 0, info.width, info.height),
            &Color::rgb(30, 30, 46),
        );

        render_system::update_layout(&mut world, root, info.width, info.height, info.scale);
        render_system::render(
            &world,
            root,
            info.width,
            info.height,
            info.scale,
            &mut hybrid,
        );

        let blits_this_frame = *hybrid.gpu.calls.borrow();
        drop(hybrid);

        backend.flush(&Rect::new(0, 0, info.width, info.height));

        if let Some(event) = poll_sdl_event(&mut backend) {
            match event {
                SdlPoll::Quit => break,
            }
        }

        frame += 1;
        if last_summary.elapsed().as_secs_f32() >= 1.0 {
            eprintln!(
                "[summary] frame {frame}, Logging (blit+clear) this frame: {blits_this_frame}"
            );
            last_summary = std::time::Instant::now();
        }
    }
}

enum SdlPoll {
    Quit,
}

fn poll_sdl_event(backend: &mut SdlBackend) -> Option<SdlPoll> {
    // SdlBackend::poll_event returns InputEvent; translate Quit to our tag.
    use mirui::backend::InputEvent;
    match backend.poll_event() {
        Some(InputEvent::Quit) => Some(SdlPoll::Quit),
        _ => None,
    }
}
