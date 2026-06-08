extern crate alloc;

use super::attach_to_parent;
#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::components::{BackgroundBlur, MirrorOf, TemporalMix, WidgetTransform};
use crate::ecs::{Entity, World};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::types::Transform;
use crate::widget::Theme;
use crate::widget::dirty::Dirty;

pub const DEFAULT_VIEW: (u16, u16) = (480, 360);

pub struct AnimX {
    pub t: Fixed,
    pub span_px: i32,
}

#[mirui_macros::system(order = ANIMATION)]
pub fn animate_x(world: &mut World) {
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

pub struct ColorFlash {
    pub frame: u32,
}

#[mirui_macros::system(order = ANIMATION)]
pub fn animate_color_flash(world: &mut World) {
    use crate::widget::Style;
    let mut entities = alloc::vec::Vec::new();
    world.query::<ColorFlash>().collect_into(&mut entities);
    for e in entities {
        let frame = if let Some(c) = world.get_mut::<ColorFlash>(e) {
            c.frame = c.frame.wrapping_add(1);
            c.frame
        } else {
            continue;
        };
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

pub fn build_widgets(world: &mut World, parent: Entity, view_w: u16, view_h: u16) -> Entity {
    let win_w = view_w as i32;
    let win_h = view_h as i32;
    let half_w = win_w / 2;
    let half_h = win_h / 2;

    let root = WidgetBuilder::new(world)
        .bg_color(Color::rgb(20, 22, 28))
        .layout(LayoutStyle {
            width: Dimension::px(win_w),
            height: Dimension::px(win_h),
            ..Default::default()
        })
        .id();

    let m_source = ui! {
        :(
            parent: root
            world: world
        :)

        View (
            bg_color: ColorToken::Primary,
            border_radius: Fixed::from_int(8),
            position: Position::Absolute,
            left: 30,
            top: 30,
            width: 180,
            height: 50
        ) {
            View (
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
            world: world
        :)

        View (
            position: Position::Absolute,
            left: 30,
            top: 90,
            width: 180,
            height: 50
        ) [
            MirrorOf::new(m_source).with_fade(160),
        ] {}
    };

    ui! {
        :(
            parent: root
            world: world
        :)

        View (
            bg_color: Color::rgb(220, 60, 60),
            border_radius: Fixed::from_int(8),
            position: Position::Absolute,
            left: 50,
            top: half_h + 60,
            width: 50,
            height: 50
        ) [
            ColorFlash { frame: 0 },
        ] {}
    };
    let tm_source = ui! {
        :(
            parent: root
            world: world
        :)

        View (
            bg_color: Color::rgb(220, 60, 60),
            border_radius: Fixed::from_int(8),
            position: Position::Absolute,
            left: 160,
            top: half_h + 60,
            width: 50,
            height: 50
        ) [
            ColorFlash { frame: 0 },
        ] {}
    };
    ui! {
        :(
            parent: root
            world: world
        :)

        View (
            position: Position::Absolute,
            left: 160,
            top: half_h + 60,
            width: 50,
            height: 50
        ) [
            TemporalMix::new(tm_source).with_mix(230),
        ] {}
    };
    ui! {
        :(
            parent: root
            world: world
        :)

        View (
            text: "raw flash       TemporalMix",
            text_color: ColorToken::OnSurface,
            position: Position::Absolute,
            left: 30,
            top: half_h + 20,
            width: 220,
            height: 20
        ) {}
    };

    ui! {
        :(
            parent: root
            world: world
        :)

        View (
            position: Position::Absolute,
            left: half_w + 20,
            top: half_h + 20,
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
                ) {}
            }
        }
    };
    ui! {
        :(
            parent: root
            world: world
        :)

        View (
            bg_color: Color::rgba(255, 255, 255, 50),
            border_radius: Fixed::from_int(10),
            position: Position::Absolute,
            left: half_w + 50,
            top: half_h + 50,
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
            world: world
        :)

        View (
            text: "BackgroundBlur",
            text_color: ColorToken::OnSurface,
            position: Position::Absolute,
            left: half_w + 30,
            top: half_h + 165,
            width: 200,
            height: 20
        ) {}
    };

    attach_to_parent(world, parent, root);
    root
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity) -> Entity
where
    B: Surface,
    F: RendererFactory<B>,
{
    let info = app.backend.display_info();
    app.with_theme(Theme::dark())
        .with_offscreen_pool_budget(1024 * 1024)
        .add_system(animate_x::system())
        .add_system(animate_color_flash::system());
    build_widgets(&mut app.world, parent, info.width, info.height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::Children;
    use crate::widget::IdMap;
    use crate::widget::builder::WidgetBuilder;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let root = build_widgets(&mut world, parent, 480, 360);
        assert_ne!(root, parent);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| c.0.contains(&root)),
        );
    }
}
