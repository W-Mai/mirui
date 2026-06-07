//! Manual loop + PPM dump for the modal-slide animation; lets the
//! dirty-region path be inspected frame by frame without a window
//! server.
//!
//! ```text
//! cargo run -p gallery --example offscreen_modal_snapshot --release \
//!     -- /tmp/modal-snap
//! ```

use mirui::components::WidgetTransform;
use mirui::prelude::*;
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::{Color, Dimension, Fixed, Transform};
use mirui::widget::theme::ColorToken;
use mirui::widget::{Children, OffscreenRender, Theme};
use mirui_macros::ui;
use std::cell::RefCell;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;

extern crate alloc;

const WIN_W: i32 = 360;
const WIN_H: i32 = 360;
const MODAL_W: i32 = 200;
const MODAL_H: i32 = 280;
const MODAL_LEFT_FINAL: i32 = (WIN_W - MODAL_W) / 2;
const GRID_COLS: i32 = 4;
const GRID_ROWS: i32 = 9;
const TILE_W: i32 = 40;
const TILE_H: i32 = 24;
const TILE_GAP: i32 = 4;
const TILE_PAD: i32 = 8;

const FRAMES: i32 = 30;

struct Anim {
    t: Fixed,
}

#[mirui::system(order = ANIMATION)]
fn slide(world: &mut World) {
    let mut entities = alloc::vec::Vec::new();
    world.query::<Anim>().collect_into(&mut entities);
    for e in entities {
        let next_t = if let Some(a) = world.get_mut::<Anim>(e) {
            a.t += Fixed::ONE / Fixed::from_int(FRAMES);
            if a.t > Fixed::ONE {
                a.t = Fixed::ZERO;
            }
            a.t
        } else {
            continue;
        };
        let off_screen = Fixed::from_int(-MODAL_LEFT_FINAL - MODAL_W);
        let tx = off_screen * (Fixed::ONE - next_t);
        world.insert(e, WidgetTransform(Transform::translate(tx, Fixed::ZERO)));
        world.insert(e, mirui::widget::dirty::Dirty);
    }
}

fn write_ppm(path: &PathBuf, buf: &[u8], w: u32, h: u32) -> std::io::Result<()> {
    let mut f = File::create(path)?;
    writeln!(f, "P6\n{} {}\n255", w, h)?;
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    // RGBA8888 wire layout is [R, G, B, A].
    for px in buf.chunks_exact(4) {
        rgb.push(px[0]);
        rgb.push(px[1]);
        rgb.push(px[2]);
    }
    f.write_all(&rgb)?;
    Ok(())
}

fn tile_color(idx: i32) -> ColorToken {
    match idx % 3 {
        0 => ColorToken::Primary,
        1 => ColorToken::Secondary,
        _ => ColorToken::Tertiary,
    }
}

fn run(out_dir: &str, offscreen: bool) {
    std::fs::create_dir_all(out_dir).ok();

    let capture: Rc<RefCell<Vec<u8>>> =
        Rc::new(RefCell::new(vec![0u8; (WIN_W * WIN_H * 4) as usize]));
    let cap_cb = capture.clone();
    let backend = FramebufSurface::new(WIN_W as u16, WIN_H as u16, move |buf, _rect| {
        let mut dst = cap_cb.borrow_mut();
        if dst.len() == buf.len() {
            dst.copy_from_slice(buf);
        }
    });

    let mut app = App::new(backend);
    app.with_default_widgets()
        .with_default_systems()
        .with_theme(Theme::dark())
        .with_offscreen_pool_budget(256 * 1024);
    app.add_system(slide::system());

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(20, 22, 28))
        .layout(LayoutStyle {
            width: Dimension::px(WIN_W),
            height: Dimension::px(WIN_H),
            ..Default::default()
        })
        .id();

    let modal = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        View (
            bg_color: ColorToken::Surface,
            border_radius: Fixed::from_int(12),
            position: Position::Absolute,
            left: MODAL_LEFT_FINAL,
            top: 30,
            width: MODAL_W,
            height: MODAL_H
        ) [
            Anim { t: Fixed::ZERO },
        ] {
            walk 0..(GRID_COLS * GRID_ROWS) with i {
                View (
                    bg_color: tile_color(i),
                    border_color: ColorToken::OnSurface,
                    border_width: Fixed::ONE,
                    border_radius: Fixed::from_int(6),
                    position: Position::Absolute,
                    left: TILE_PAD + (i % GRID_COLS) * (TILE_W + TILE_GAP),
                    top: TILE_PAD + (i / GRID_COLS) * (TILE_H + TILE_GAP),
                    width: TILE_W,
                    height: TILE_H
                ) {}
            }
        }
    };

    if offscreen {
        app.world.insert(modal, OffscreenRender::default());
    }

    app.world.insert(root, Children(alloc::vec![modal]));
    app.set_root(root);

    let prefix = if offscreen { "off" } else { "inl" };

    app.render();

    for frame in 0..FRAMES {
        app.systems.run_all(&mut app.world);
        app.render_dirty();
        let path = PathBuf::from(out_dir).join(format!("{prefix}-{frame:03}.ppm"));
        write_ppm(&path, &capture.borrow(), WIN_W as u32, WIN_H as u32).expect("write ppm");
    }
}

fn main() {
    let out_dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/modal-snap".to_string());
    eprintln!("dumping inline frames to {out_dir}/inl-NNN.ppm");
    run(&out_dir, false);
    eprintln!("dumping offscreen frames to {out_dir}/off-NNN.ppm");
    run(&out_dir, true);
    eprintln!("done");
}
