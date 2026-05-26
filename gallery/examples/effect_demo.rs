//! Visual demo of the three built-in effect widgets.
//!
//! Layout (480 × 360):
//!
//! ```text
//! +--------------------+--------------------+
//! | MirrorOf           |                    |
//! | source above,      |                    |
//! | flipped + faded    |                    |
//! | mirror below       |                    |
//! +--------------------+--------------------+
//! | TemporalMix        | BackgroundBlur     |
//! | source flashes     | tiled colour grid  |
//! | colours; effect    | with frosted glass |
//! | smooths the flash  |                    |
//! +--------------------+--------------------+
//! ```
//!
//! ```text
//! cargo run -p gallery --example effect_demo --release
//! ```

use mirui::components::{BackgroundBlur, MirrorOf, TemporalMix};
use mirui::prelude::*;
use mirui::surface::sdl::SdlSurface;
use mirui::types::{Color, Dimension, Fixed, Transform};
use mirui::widget::Theme;
use mirui::widget::dirty::Dirty;
use mirui::widget::theme::ColorToken;

extern crate alloc;

const WIN_W: i32 = 480;
const WIN_H: i32 = 360;
const HALF_W: i32 = WIN_W / 2;
const HALF_H: i32 = WIN_H / 2;

struct AnimX {
    t: Fixed,
    span_px: i32,
}

#[mirui::system(order = ANIMATION)]
fn animate_x(world: &mut World) {
    use mirui::components::WidgetTransform;
    let mut entities = alloc::vec::Vec::new();
    world.query::<AnimX>().collect_into(&mut entities);
    for e in entities {
        let (next_t, span_px) = if let Some(a) = world.get_mut::<AnimX>(e) {
            a.t += Fixed::ONE / 90;
            if a.t > Fixed::ONE {
                a.t -= Fixed::ONE;
            }
            (a.t, a.span_px)
        } else {
            continue;
        };
        let bounce = if next_t < Fixed::ONE / 2 {
            next_t * Fixed::from_int(2)
        } else {
            (Fixed::ONE - next_t) * Fixed::from_int(2)
        };
        let span = Fixed::from_int(span_px);
        let tx = (bounce - Fixed::ONE / 2) * Fixed::from_int(2) * span;
        world.insert(e, WidgetTransform(Transform::translate(tx, Fixed::ZERO)));
        world.insert(e, Dirty);
    }
}

struct ColorFlash {
    frame: u32,
}

#[mirui::system(order = ANIMATION)]
fn animate_color_flash(world: &mut World) {
    use mirui::widget::Style;
    let mut entities = alloc::vec::Vec::new();
    world.query::<ColorFlash>().collect_into(&mut entities);
    for e in entities {
        let frame = if let Some(c) = world.get_mut::<ColorFlash>(e) {
            c.frame = c.frame.wrapping_add(1);
            c.frame
        } else {
            continue;
        };
        // Switch color every ~10 frames so the change is visually
        // jarring without TemporalMix and noticeably smoothed with it.
        let phase = (frame / 60) % 3;
        let color = match phase {
            0 => Color::rgb(220, 60, 60),
            1 => Color::rgb(60, 200, 80),
            _ => Color::rgb(40, 140, 220),
        };
        if let Some(style) = world.get_mut::<Style>(e) {
            style.bg_color = Some(color.into());
        }
        world.insert(e, Dirty);
    }
}

fn main() {
    let backend = SdlSurface::new("mirui — effect widget demo", WIN_W as u16, WIN_H as u16);
    let mut app = App::new(backend);
    app.with_default_widgets()
        .with_default_systems()
        .with_theme(Theme::dark())
        .with_offscreen_pool_budget(1024 * 1024);
    app.add_system(animate_x::system());
    app.add_system(animate_color_flash::system());

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(20, 22, 28))
        .layout(LayoutStyle {
            width: Dimension::px(WIN_W),
            height: Dimension::px(WIN_H),
            ..Default::default()
        })
        .id();

    // ---- Pane 1 (top-left): MirrorOf ----
    let m_source = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        m_source (
            bg_color: ColorToken::Primary,
            border_radius: Fixed::from_int(8),
            position: Position::Absolute,
            left: 30,
            top: 30,
            width: 180,
            height: 50
        ) {
            label (
                text: "MirrorOf",
                text_color: ColorToken::OnPrimary,
                position: Position::Absolute,
                left: 12,
                top: 16,
                width: 160,
                height: 20
            ) {}
        }
    };
    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        m_mirror (
            position: Position::Absolute,
            left: 30,
            top: 90,
            width: 180,
            height: 50
        ) [
            MirrorOf::new(m_source).with_fade(160),
        ] {}
    };

    // ---- Pane 3 (bottom-left): TemporalMix vs no fx ----
    // Both squares cycle red → green → blue every ~10 frames. The
    // left one shows the raw flash. The right one has TemporalMix
    // overlaid: the previous frame's colour bleeds into the current
    // frame, so the cycle looks smoother / muddier.
    let _bare = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        tm_bare (
            bg_color: Color::rgb(220, 60, 60),
            border_radius: Fixed::from_int(8),
            position: Position::Absolute,
            left: 50,
            top: HALF_H + 60,
            width: 50,
            height: 50
        ) [
            ColorFlash { frame: 0 },
        ] {}
    };
    let tm_source = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        tm_source (
            bg_color: Color::rgb(220, 60, 60),
            border_radius: Fixed::from_int(8),
            position: Position::Absolute,
            left: 160,
            top: HALF_H + 60,
            width: 50,
            height: 50
        ) [
            ColorFlash { frame: 0 },
        ] {}
    };
    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        tm_overlay (
            position: Position::Absolute,
            left: 160,
            top: HALF_H + 60,
            width: 50,
            height: 50
        ) [
            TemporalMix::new(tm_source).with_mix(230),
        ] {}
    };
    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        tm_label (
            text: "raw flash       TemporalMix",
            text_color: ColorToken::OnSurface,
            position: Position::Absolute,
            left: 30,
            top: HALF_H + 20,
            width: 220,
            height: 20
        ) {}
    };

    let _backdrop = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        bg_backdrop (
            position: Position::Absolute,
            left: HALF_W + 20,
            top: HALF_H + 20,
            width: 220,
            height: 140
        ) {
            walk 0..12 with i {
                tile (
                    bg_color: tile_color(i),
                    position: Position::Absolute,
                    left: (i % 4) * 55,
                    top: (i / 4) * 47,
                    width: 55,
                    height: 47
                ) {}
            }
        }
    };
    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        bg_glass (
            bg_color: Color::rgba(255, 255, 255, 50),
            border_radius: Fixed::from_int(10),
            position: Position::Absolute,
            left: HALF_W + 50,
            top: HALF_H + 50,
            width: 160,
            height: 80
        ) [
            BackgroundBlur::new(8),
            AnimX {
                t: Fixed::ZERO,
                span_px: 30,
            },
        ] {}
    };
    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        bg_label (
            text: "BackgroundBlur",
            text_color: ColorToken::OnSurface,
            position: Position::Absolute,
            left: HALF_W + 30,
            top: HALF_H + 165,
            width: 200,
            height: 20
        ) {}
    };

    app.set_root(root);
    app.run();
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
