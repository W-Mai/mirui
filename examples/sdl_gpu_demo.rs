//! SDL GPU backend demo — hardware-accelerated rendering with a
//! drag-to-move widget. The flex-laid children (solid, rounded, blit,
//! label) stack vertically centred; the orange "DRAG ME" box is
//! `Position::Absolute` and follows the mouse while the left button is
//! held.
//!
//! Vsync is off so the fps summary reflects the real per-frame cost of
//! the GPU path.

use mirui::app::App;
use mirui::backend::sdl_gpu::{SdlGpuBackend, SdlGpuFactory};
use mirui::backend::{Backend, InputEvent};
use mirui::components::assets::*;
use mirui::components::image::Image;
use mirui::layout::*;
use mirui::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};
use mirui::types::{Color, Dimension, Fixed};
use mirui::widget::builder::WidgetBuilder;
use mirui::widget::{Children, Parent};

const DRAG_W: i32 = 160;
const DRAG_H: i32 = 60;

fn main() {
    let backend = SdlGpuBackend::new_with_vsync("mirui SDL GPU — drag me", 640, 480, false);
    let mut app = App::with_factory(backend, SdlGpuFactory::new());

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgba(30, 30, 46, 255))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::percent(100),
            height: Dimension::percent(100),
            justify: JustifyContent::Center,
            align: AlignItems::Center,
            ..Default::default()
        })
        .id();

    let solid = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgba(32, 160, 240, 255))
        .border(Color::rgba(240, 240, 255, 255), 1)
        .layout(LayoutStyle {
            width: Dimension::px(200),
            height: Dimension::px(80),
            ..Default::default()
        })
        .id();

    let translucent = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgba(80, 240, 160, 128))
        .layout(LayoutStyle {
            width: Dimension::px(200),
            height: Dimension::px(80),
            ..Default::default()
        })
        .id();

    let label = WidgetBuilder::new(&mut app.world)
        .text("SDL GPU BACKEND")
        .text_color(Color::rgba(255, 255, 255, 255))
        .layout(LayoutStyle {
            width: Dimension::px(200),
            height: Dimension::px(20),
            ..Default::default()
        })
        .id();

    let img = WidgetBuilder::new(&mut app.world)
        .layout(LayoutStyle {
            width: Dimension::px(IMG_THUMBS_UP.width as i32),
            height: Dimension::px(IMG_THUMBS_UP.height as i32),
            ..Default::default()
        })
        .id();
    app.world.insert(img, Image::new(&IMG_THUMBS_UP));

    let drag = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgba(240, 120, 60, 230))
        .border(Color::rgba(255, 255, 255, 255), 2)
        .border_radius(12)
        .text("DRAG ME")
        .text_color(Color::rgba(255, 255, 255, 255))
        .layout(LayoutStyle {
            position: Position::Absolute,
            width: Dimension::px(DRAG_W),
            height: Dimension::px(DRAG_H),
            left: Dimension::px(240),
            top: Dimension::px(30),
            ..Default::default()
        })
        .id();

    for child in [solid, translucent, label, img, drag] {
        app.world.insert(child, Parent(root));
        if let Some(children) = app.world.get_mut::<Children>(root) {
            children.0.push(child);
        }
    }

    app.set_root(root);
    app.add_plugin(StdInstantClockPlugin)
        .add_plugin(FpsSummaryPlugin::default());

    let mut drag_state: Option<(Fixed, Fixed)> = None;
    let mut drag_x = Fixed::from_int(240);
    let mut drag_y = Fixed::from_int(30);

    let mut wall_start = std::time::Instant::now();
    let mut frames_since_report: u32 = 0;

    loop {
        let mut quit = false;
        loop {
            match app.backend.poll_event() {
                Some(InputEvent::Quit) => {
                    quit = true;
                    break;
                }
                Some(InputEvent::Touch { x, y }) => {
                    if hit(x, y, drag_x, drag_y) {
                        drag_state = Some((x - drag_x, y - drag_y));
                    }
                }
                Some(InputEvent::TouchMove { x, y }) => {
                    if let Some((ox, oy)) = drag_state {
                        drag_x = x - ox;
                        drag_y = y - oy;
                        mirui::widget::set_position(&mut app.world, drag, drag_x, drag_y);
                    }
                }
                Some(InputEvent::Release { .. }) => {
                    drag_state = None;
                }
                Some(_) => {}
                None => break,
            }
        }
        if quit {
            break;
        }
        app.render();
        frames_since_report += 1;
        let elapsed = wall_start.elapsed();
        if elapsed.as_secs_f64() >= 1.0 {
            eprintln!(
                "[wall] {} frames in {:.3}s → {:.0} fps",
                frames_since_report,
                elapsed.as_secs_f64(),
                frames_since_report as f64 / elapsed.as_secs_f64()
            );
            frames_since_report = 0;
            wall_start = std::time::Instant::now();
        }
    }
}

fn hit(x: Fixed, y: Fixed, dx: Fixed, dy: Fixed) -> bool {
    let w = Fixed::from_int(DRAG_W);
    let h = Fixed::from_int(DRAG_H);
    x >= dx && x < dx + w && y >= dy && y < dy + h
}
