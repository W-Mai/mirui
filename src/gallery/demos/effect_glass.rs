extern crate alloc;

use crate::anim::{PlayMode, Tween, ease};
#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::components::{BackgroundBlur, MirrorOf, Text};
use crate::ecs::{Entity, World};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::widget::Style;

pub const DEFAULT_VIEW: (u16, u16) = (128, 128);

mirui_macros::animate!(GlassX, |world, entity, value| {
    crate::widget::set_position(world, entity, value, Fixed::from_int(50));
});

mirui_macros::animate!(GaussRadius, |world, entity, value| {
    if let Some(blur) = world.get_mut::<BackgroundBlur>(entity) {
        blur.radius = value;
    }
});

const TILE_COLORS: [Color; 4] = [
    Color::rgb(220, 60, 60),
    Color::rgb(220, 160, 40),
    Color::rgb(60, 200, 80),
    Color::rgb(40, 140, 220),
];

fn tile_color(row: i32, col: i32) -> Color {
    TILE_COLORS[((row + col) as usize) % TILE_COLORS.len()]
}

pub fn build_widgets(world: &mut World, parent: Entity, view_w: u16, view_h: u16) {
    let win_w = view_w as i32;
    let win_h = view_h as i32;

    if let Some(style) = world.get_mut::<Style>(parent) {
        style.bg_color = Some(Color::rgb(20, 22, 28).into());
        style.layout = LayoutStyle {
            width: Dimension::px(win_w),
            height: Dimension::px(win_h),
            grow: Fixed::ONE,
            ..Default::default()
        };
    }

    ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: win_w,
            height: win_h
        ) {
            walk 0..3i32 with row {
                walk 0..4i32 with col {
                    View (
                        bg_color: tile_color(row, col),
                        position: Position::Absolute,
                        left: col * 32,
                        top: row * 32,
                        width: 32,
                        height: 32
                    ) {}
                }
            }
        }
    };

    let m_source = ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            bg_color: Color::rgb(80, 160, 255),
            position: Position::Absolute,
            left: 8,
            top: 8,
            width: 40,
            height: 14
        ) {}
    };

    ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            position: Position::Absolute,
            left: 8,
            top: 24,
            width: 40,
            height: 14
        ) [
            MirrorOf::new(m_source).with_fade(180),
        ] {}
    };

    ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            text: "BlurMeBlurMe",
            position: Position::Absolute,
            left: 8,
            top: 58,
            width: win_w - 16,
            height: 14
        ) [
            Text(b"BlurMeBlurMe".to_vec()),
        ] {}
    };

    ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            bg_color: Color::rgba(255, 255, 255, 50),
            border_radius: Fixed::from_int(6),
            position: Position::Absolute,
            left: 30,
            top: 50,
            width: 60,
            height: 30
        ) [
            BackgroundBlur::new(2),
            GlassX(
                Tween::new(
                    Fixed::from_int(8),
                    Fixed::from_int(win_w - 60 - 8),
                    3000,
                    ease::ease_in_out_cubic,
                    PlayMode::PingPong,
                )
                .into(),
            ),
            GaussRadius(
                Tween::new(
                    Fixed::from_int(0),
                    Fixed::from_int(3),
                    3000,
                    ease::ease_in_out_cubic,
                    PlayMode::PingPong,
                )
                .into(),
            )
        ] {}
    };
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::ecs;
    use crate::plugins::StdInstantClockPlugin;

    let info = app.backend.display_info();
    app.add_plugin(StdInstantClockPlugin);
    app.add_system(ecs::System::new(
        "glass_x",
        ecs::run_order::ANIMATION,
        GlassX::system(),
    ));
    app.add_system(ecs::System::new(
        "gauss_radius",
        ecs::run_order::ANIMATION,
        GaussRadius::system(),
    ));
    app.with_offscreen_pool_budget(8 * 1024);
    build_widgets(&mut app.world, parent, info.width, info.height);
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
        build_widgets(&mut world, parent, 128, 128);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
    }
}
