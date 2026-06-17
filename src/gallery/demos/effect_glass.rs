extern crate alloc;

use crate::anim::{PlayMode, Tween, ease};
use crate::prelude::*;
use crate::ui::widgets::{BackgroundBlur, MirrorOf, Text};

pub const DEFAULT_VIEW: (u16, u16) = (128, 128);

const GLASS_W: i32 = 60;
const GLASS_MARGIN: i32 = 8;
const GLASS_PERIOD_MS: u16 = 3000;

//~focus-start
mirui_macros::animate!(GlassX, |world, entity, value| {
    mirui::ui::set_position(world, entity, value, Fixed::from_int(50));
});

mirui_macros::animate!(GaussRadius, |world, entity, value| {
    if let Some(blur) = world.get_mut::<BackgroundBlur>(entity) {
        blur.radius = value;
    }
});
//~focus-end

const TILE_COLORS: [Color; 4] = [
    Color::rgb(220, 60, 60),
    Color::rgb(220, 160, 40),
    Color::rgb(60, 200, 80),
    Color::rgb(40, 140, 220),
];

fn tile_color(row: i32, col: i32) -> Color {
    TILE_COLORS[((row + col) as usize) % TILE_COLORS.len()]
}

pub fn build_widgets(world: &mut World, parent: Entity) {
    ui! {
        :(
            parent: parent
            world: world
        :)

        View (grow: 1.0) {
            walk 0..3i32 with row {
                walk 0..4i32 with col {
                    View (
                        bg_color: tile_color(row, col),
                        position: Position::Absolute,
                        left: col * 32,
                        top: row * 32,
                        width: 32,
                        height: 32
                    )
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
        )
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
        ]
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
            width: 112,
            height: 14
        ) [
            Text(b"BlurMeBlurMe".to_vec()),
        ]
    };

    //~focus-start
    ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            bg_color: Color::rgba(255, 255, 255, 50),
            border_radius: Fixed::from_int(6),
            position: Position::Absolute,
            left: GLASS_MARGIN,
            top: 50,
            width: GLASS_W,
            height: 30
        ) [
            BackgroundBlur::new(2),
            GlassX(
                Tween::new(
                        Fixed::from_int(GLASS_MARGIN),
                        Fixed::from_int(DEFAULT_VIEW.0 as i32 - GLASS_W - GLASS_MARGIN),
                        GLASS_PERIOD_MS,
                        ease::ease_in_out_cubic,
                        PlayMode::PingPong,
                    )
                    .into(),
            ),
            GaussRadius(
                Tween::new(
                        Fixed::from_int(0),
                        Fixed::from_int(3),
                        GLASS_PERIOD_MS,
                        ease::ease_in_out_cubic,
                        PlayMode::PingPong,
                    )
                    .into(),
            ),
        ]
    };
    //~focus-end
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::app::plugins::StdInstantClockPlugin;

    app.add_plugin(StdInstantClockPlugin);
    app.add_system(mirui::ecs::System::new(
        "glass_x",
        mirui::ecs::run_order::ANIMATION,
        GlassX::system(),
    ));
    app.add_system(crate::ecs::System::new(
        "gauss_radius",
        crate::ecs::run_order::ANIMATION,
        GaussRadius::system(),
    ));
    app.with_offscreen_pool_budget(8 * 1024);
    build_widgets(&mut app.world, parent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Children;
    use crate::ui::IdMap;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        build_widgets(&mut world, parent);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
    }
}
