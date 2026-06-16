//! Headless snapshot of the cover_flow scene at a sweep of scroll
//! offsets. Writes /tmp/cover_flow_<off>.ppm for each and reports which
//! pixels change across consecutive offsets — used to chase the
//! sub-pixel flicker during drag.
//!
//!     cargo run --example snapshot_cover_flow --features sdl --release
use mirui::prelude::*;
use std::cell::RefCell;
use std::fs::File;
use std::io::Write;
use std::rc::Rc;

use mirui::components::{Image, WidgetTransform3D};
use mirui::event::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::{Color, Dimension, Fixed, Transform3D};
use mirui::widget::dirty::Dirty;

extern crate alloc;

const WINDOW_W: i32 = 640;
const WINDOW_H: i32 = 360;
const CARD_W: i32 = 140;
const CARD_H: i32 = 180;
const CARD_GAP: i32 = 80;
const PERSPECTIVE: i32 = 500;
const CARD_COUNT: i32 = 5;

struct CarouselCard {
    index: usize,
}

struct Carousel;

#[mirui::system]
fn layout_system(world: &mut World) {
    let mut carousels = alloc::vec::Vec::new();
    world.query::<Carousel>().collect_into(&mut carousels);
    let offset = match carousels
        .first()
        .and_then(|&e| world.get::<ScrollOffset>(e))
    {
        Some(s) => s.x,
        None => return,
    };

    let carousel = carousels[0];

    let mut cards = alloc::vec::Vec::new();
    world.query::<CarouselCard>().collect_into(&mut cards);
    if cards.is_empty() {
        return;
    }
    let slot_stride = Fixed::from_int(CARD_W + CARD_GAP);
    let container_center = Fixed::from_int(WINDOW_W / 2);

    for e in cards {
        let idx = match world.get::<CarouselCard>(e) {
            Some(c) => c.index as i32,
            None => continue,
        };
        let tx =
            container_center + Fixed::from_int(idx) * slot_stride - Fixed::from_int(CARD_W / 2);
        let ty = Fixed::from_int((WINDOW_H - CARD_H) / 2);
        mirui::widget::set_position(world, e, tx, ty);

        let relative = Fixed::from_int(idx) - offset / slot_stride;
        let tilt = Fixed::ZERO - relative * Fixed::from_int(22);
        world.insert(
            e,
            WidgetTransform3D(Transform3D::rotate_y_perspective(
                tilt,
                Fixed::from_int(PERSPECTIVE),
            )),
        );
        world.insert(e, Dirty);
    }
    world.insert(carousel, Dirty);
}

fn write_ppm(path: &str, buf: &[u8], w: u32, h: u32) -> std::io::Result<()> {
    // buf is RGBA8888 → byte order [R, G, B, A]
    let mut f = File::create(path)?;
    writeln!(f, "P6\n{} {}\n255", w, h)?;
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for px in buf.chunks_exact(4) {
        rgb.push(px[2]); // R
        rgb.push(px[1]); // G
        rgb.push(px[0]); // B
    }
    f.write_all(&rgb)?;
    Ok(())
}

fn render_at(scroll_x_raw: i32) -> Vec<u8> {
    let capture: Rc<RefCell<Vec<u8>>> = Rc::new(RefCell::new(Vec::new()));
    let cap_cb = capture.clone();

    let backend = FramebufSurface::new(WINDOW_W as u16, WINDOW_H as u16, move |buf, _rect| {
        *cap_cb.borrow_mut() = buf.to_vec();
    });
    let mut app = App::new(backend);
    app.with_default_widgets().with_default_systems();
    app.add_plugin(mirui::plugins::ImageResourcesPlugin::default());

    app.add_system(layout_system::system());

    let card_colors = [
        Color::rgb(255, 107, 107),
        Color::rgb(255, 206, 84),
        Color::rgb(136, 216, 176),
        Color::rgb(118, 209, 244),
        Color::rgb(178, 148, 255),
    ];

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(24, 26, 34))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(WINDOW_W),
            height: Dimension::px(WINDOW_H),
            ..Default::default()
        })
        .id();

    let world = &mut app.world;
    let card_colors_ref = &card_colors;
    mirui_macros::ui! {
        :(
            parent: root
            world: world
        :)

        View (
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: WINDOW_W,
            height: WINDOW_H
        ) [
            Carousel,
            ScrollOffset {
                x: Fixed::from_raw(scroll_x_raw),
                y: Fixed::ZERO,
            },
            ScrollConfig {
                direction: ScrollAxis::Horizontal,
                elastic: true,
                content_width: Fixed::from_int(
                    WINDOW_W + (CARD_W + CARD_GAP) * (CARD_COUNT - 1),
                ),
                content_height: Fixed::ZERO,
            },
        ] {
            walk card_colors_ref.iter().enumerate() with item {
                View (
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: CARD_W,
                    height: CARD_H,
                    bg_color: *item.1,
                    border_radius: 8,
                    border_color: Color::rgb(0, 0, 0),
                    border_width: 5
                ) [
                    CarouselCard { index: item.0 },
                ] {
                    if item.0 % 2 == 1 {
                        View (
                            position: Position::Absolute,
                            left: (CARD_W - 64) / 2,
                            top: (CARD_H - 64) / 2,
                            width: 64,
                            height: 64,
                            image: Image::new("thumbs_up")
                        )
                    }
                }
            }
        }
    };

    app.set_root(root);
    app.systems.run_all(&mut app.world);
    app.render();
    capture.borrow().clone()
}

fn main() {
    let dir = std::env::var("MIRUI_SNAPSHOT_DIR").unwrap_or_else(|_| ".".into());
    std::fs::create_dir_all(&dir).ok();
    let base_x = Fixed::from_int(440).raw();
    let mut prev: Option<Vec<u8>> = None;
    for step in 0..17 {
        let x_raw = base_x + step * 16;
        let buf = render_at(x_raw);
        let name = alloc::format!("{}/cover_flow_{:02}.ppm", dir, step);
        write_ppm(&name, &buf, WINDOW_W as u32, WINDOW_H as u32).expect("write ppm");

        if let Some(p) = &prev {
            let diffs = diff(p, &buf);
            let mut big = 0usize;
            let mut total_mag: u64 = 0;
            let mut max_mag: u32 = 0;
            for (_, _, a, b) in &diffs {
                let dr = (a.0 as i32 - b.0 as i32).unsigned_abs();
                let dg = (a.1 as i32 - b.1 as i32).unsigned_abs();
                let db = (a.2 as i32 - b.2 as i32).unsigned_abs();
                let m = dr.max(dg).max(db);
                total_mag += m as u64;
                if m > max_mag {
                    max_mag = m;
                }
                if m > 64 {
                    big += 1;
                }
            }
            let avg = if diffs.is_empty() {
                0
            } else {
                (total_mag / diffs.len() as u64) as u32
            };
            println!(
                "step {:02}: x_raw={} ({:.3}px)  changed={} px  avgΔ={} maxΔ={} big(>64)={}",
                step,
                x_raw,
                x_raw as f32 / 256.0,
                diffs.len(),
                avg,
                max_mag,
                big,
            );
            if step == 14 && big > 100 {
                let mut rows = std::collections::BTreeMap::<u32, u32>::new();
                for (_, y, a, b) in &diffs {
                    let m = (a.0 as i32 - b.0 as i32)
                        .unsigned_abs()
                        .max((a.1 as i32 - b.1 as i32).unsigned_abs())
                        .max((a.2 as i32 - b.2 as i32).unsigned_abs());
                    if m > 64 {
                        *rows.entry(*y).or_insert(0) += 1;
                    }
                }
                println!("   step 14 rows with big>64:");
                for (y, c) in rows.iter() {
                    if *c > 2 {
                        println!("     y={} × {}", y, c);
                    }
                }
            }
        }
        prev = Some(buf);
    }
}

fn diff(a: &[u8], b: &[u8]) -> Vec<(u32, u32, (u8, u8, u8), (u8, u8, u8))> {
    let w = WINDOW_W as u32;
    let h = WINDOW_H as u32;
    let mut out = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let pa = (a[i + 2], a[i + 1], a[i]);
            let pb = (b[i + 2], b[i + 1], b[i]);
            if pa != pb {
                out.push((x, y, pa, pb));
            }
        }
    }
    out
}
