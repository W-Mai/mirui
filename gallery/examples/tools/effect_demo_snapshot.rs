//! Headless dump of the effect_demo scene to a PPM file. Uses the same widget tree as `effect_demo`, renders completed source outputs and retained temporal-mix history, then writes the framebuffer as a PPM.
//!
//! ```text
//! cargo run -p gallery --example effect_demo_snapshot --release \
//!     -- /tmp/effect-snap.ppm
//! ```

use mirui::gallery::demos::effect_panels::{ColorFlash, animate_color_flash};
use mirui::prelude::*;
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::{Color, Dimension, Fixed, Transform};
use mirui::ui::Theme;
use mirui::ui::dirty::Dirty;
use mirui::ui::theme::ColorToken;
use mirui::ui::widgets::{BackgroundBlur, MirrorOf, TemporalMix, Text};
use std::cell::RefCell;
use std::fs::File;
use std::io::Write;
use std::rc::Rc;

extern crate alloc;

const WIN_W: i32 = 480;
const WIN_H: i32 = 360;
const HALF_W: i32 = WIN_W / 2;
const HALF_H: i32 = WIN_H / 2;

#[mirui::component]
struct AnimX {
    t: Fixed,
    span_px: i32,
}

#[mirui::system(order = Animation)]
fn animate_x(animations: query!(Entity, mut AnimX), commands: commands!()) {
    use mirui::ui::widgets::WidgetTransform;
    for (entity, animation) in animations {
        animation.t += Fixed::ONE / 90;
        if animation.t > Fixed::ONE {
            animation.t -= Fixed::ONE;
        }
        let bounce = if animation.t < Fixed::ONE / 2 {
            animation.t * Fixed::from_int(2)
        } else {
            (Fixed::ONE - animation.t) * Fixed::from_int(2)
        };
        let span = Fixed::from_int(animation.span_px);
        let tx = (bounce - Fixed::ONE / 2) * Fixed::from_int(2) * span;
        commands.insert(
            entity,
            WidgetTransform(Transform::translate(tx, Fixed::ZERO)),
        );
        commands.insert(entity, Dirty);
    }
}

fn write_ppm(path: &str, buf: &[u8], w: u32, h: u32) -> std::io::Result<()> {
    let mut f = File::create(path)?;
    writeln!(f, "P6\n{} {}\n255", w, h)?;
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for px in buf.chunks_exact(4) {
        rgb.push(px[0]);
        rgb.push(px[1]);
        rgb.push(px[2]);
    }
    f.write_all(&rgb)
}

fn tile_color(idx: i32) -> Color {
    match idx % 6 {
        0 => Color::rgb(220, 60, 60),
        1 => Color::rgb(220, 160, 40),
        2 => Color::rgb(60, 200, 80),
        3 => Color::rgb(40, 140, 220),
        4 => Color::rgb(180, 80, 220),
        _ => Color::rgb(40, 200, 200),
    }
}

#[compose]
fn effects_panel() {
    let m_source = ui! {
        View (
            bg_color: ColorToken::Primary,
            border_radius: Fixed::from_int(8),
            position: Position::Absolute,
            left: 30,
            top: 30,
            width: 180,
            height: 50
        ) {
            Text (
                "MirrorOf",
                text_color: ColorToken::OnPrimary,
                position: Position::Absolute,
                left: 12,
                top: 16,
                width: 160,
                height: 20
            )
        }
    };
    ui! {
        View (
            position: Position::Absolute,
            left: 30,
            top: 90,
            width: 180,
            height: 50
        ) [
            MirrorOf::new(m_source).with_fade(160),
        ]
    };

    ui! {
        View (
            bg_color: Color::rgb(220, 60, 60),
            border_radius: Fixed::from_int(8),
            position: Position::Absolute,
            left: 50,
            top: HALF_H + 60,
            width: 50,
            height: 50
        ) [
            ColorFlash { frame: 0 },
        ]
    };
    let tm_source = ui! {
        View (
            bg_color: Color::rgb(220, 60, 60),
            border_radius: Fixed::from_int(8),
            position: Position::Absolute,
            left: 160,
            top: HALF_H + 60,
            width: 50,
            height: 50
        ) [
            ColorFlash { frame: 0 },
        ]
    };
    ui! {
        View (
            position: Position::Absolute,
            left: 160,
            top: HALF_H + 60,
            width: 50,
            height: 50
        ) [
            TemporalMix::new(tm_source).with_mix(230),
        ]
    };
    ui! {
        Text (
            "raw flash       TemporalMix",
            text_color: ColorToken::OnSurface,
            position: Position::Absolute,
            left: 30,
            top: HALF_H + 20,
            width: 220,
            height: 20
        )
    };

    ui! {
        View (
            position: Position::Absolute,
            left: HALF_W + 20,
            top: HALF_H + 20,
            width: 220,
            height: 140
        ) {
            walk 0..12 with i {
                View (
                    bg_color: tile_color(i),
                    position: Position::Absolute,
                    left: (i % 4) * 55,
                    top: (i / 4) * 47,
                    width: 55,
                    height: 47
                )
            }
        }
    };
    ui! {
        View (
            bg_color: Color::rgba(255, 255, 255, 50),
            border_radius: Fixed::from_int(10),
            position: Position::Absolute,
            left: HALF_W + 50,
            top: HALF_H + 50,
            width: 160,
            height: 80
        ) [
            BackgroundBlur::new(10),
        ]
    };
    ui! {
        Text (
            "BackgroundBlur",
            text_color: ColorToken::OnSurface,
            position: Position::Absolute,
            left: HALF_W + 30,
            top: HALF_H + 165,
            width: 200,
            height: 20
        )
    };
}

fn main() {
    let out_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/effect-snap.ppm".to_string());

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
        .with_offscreen_pool_budget(1024 * 1024);
    app.add_system(animate_x::system());
    app.add_system(animate_color_flash::system());

    let root = app
        .spawn_root()
        .ignore_safe_area()
        .bg_color(Color::rgb(20, 22, 28))
        .layout(LayoutStyle {
            width: Dimension::px(WIN_W),
            height: Dimension::px(WIN_H),
            ..Default::default()
        })
        .id();

    app.compose(root, effects_panel());

    // Prepare source outputs and publish the first TemporalMix result
    // before another render batch reads its retained history.
    app.render().unwrap();

    // If the second arg is "anim", dump 60 sequential frames so we
    // can inspect for residual / smearing as the source moves.
    if std::env::args().nth(2).as_deref() == Some("anim") {
        let dir = std::path::PathBuf::from(&out_path);
        std::fs::create_dir_all(&dir).ok();
        for frame in 0..60 {
            app.systems.run_all(&mut app.world).unwrap();
            app.render_dirty().unwrap();
            let path = dir.join(format!("frame-{frame:03}.ppm"));
            write_ppm(
                path.to_str().unwrap(),
                &capture.borrow(),
                WIN_W as u32,
                WIN_H as u32,
            )
            .expect("write ppm");
        }
        eprintln!("wrote 60 frames to {}", dir.display());
    } else {
        for _ in 0..8 {
            app.systems.run_all(&mut app.world).unwrap();
            app.render_dirty().unwrap();
        }
        write_ppm(&out_path, &capture.borrow(), WIN_W as u32, WIN_H as u32).expect("write ppm");
        eprintln!("wrote {out_path}");
    }
}
