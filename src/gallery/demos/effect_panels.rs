extern crate alloc;

use crate::anim::{PlayMode, Tween, ease};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::ui::Theme;
use crate::ui::dirty::Dirty;
use crate::ui::widgets::{BackgroundBlur, DropGlow, DropShadow, MirrorOf, TemporalMix, Text};

pub const DEFAULT_VIEW: (u16, u16) = (360, 560);

pub struct ColorFlash {
    pub frame: u32,
}

#[system(order = ANIMATION)]
pub fn animate_color_flash(world: &mut World) {
    let mut entities = alloc::vec::Vec::new();
    world.query::<ColorFlash>().collect_into(&mut entities);
    for e in entities {
        let frame = match world.get_mut::<ColorFlash>(e) {
            Some(c) => {
                c.frame = c.frame.wrapping_add(1);
                c.frame
            }
            None => continue,
        };
        let color = match (frame / 60) % 3 {
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

animate!(BlurPan, |world, entity, value| {
    mirui::ui::set_position(world, entity, value, Fixed::from_int(259));
});

animate!(ShadowOffset, |world, entity, value| {
    if let Some(sh) = world.get_mut::<DropShadow>(entity) {
        sh.offset.0 = value;
        sh.offset.1 = value;
    }
    world.insert(entity, Dirty);
});

animate!(GlowPulse, |world, entity, value| {
    if let Some(gl) = world.get_mut::<DropGlow>(entity) {
        gl.blur_radius = value;
    }
    world.insert(entity, Dirty);
});

fn tile_color(i: i32) -> Color {
    [
        Color::rgb(220, 60, 60),
        Color::rgb(220, 160, 40),
        Color::rgb(60, 200, 80),
        Color::rgb(40, 140, 220),
        Color::rgb(180, 80, 220),
        Color::rgb(40, 200, 200),
    ][(i % 6) as usize]
}

#[compose]
pub fn build_widgets() {
    ui! {
        Column (
            direction: FlexDirection::Column,
            align: AlignItems::FlexStart,
            grow: 1.0,
            padding: Padding::all(10)
        ) {
            Text ("MirrorOf", text_color: ColorToken::OnSurface, width: 240, height: 20)
            Row (direction: FlexDirection::Row, justify: JustifyContent::SpaceAround, height: 80) {
                Text (
                    "source",
                    id: "mirror-src",
                    bg_color: ColorToken::Primary,
                    text_color: ColorToken::OnPrimary,
                    border_radius: 8,
                    width: 140,
                    height: 50
                )
                View (width: 140, height: 50) [
                    MirrorOf::new(id("mirror-src")).with_fade(160),
                ]
            }
            Text ("TemporalMix", text_color: ColorToken::OnSurface, width: 240, height: 20)
            Row (direction: FlexDirection::Row, justify: JustifyContent::SpaceAround, height: 80) {
                View (
                    id: "tm_src",
                    border_radius: 8,
                    width: 140,
                    height: 50
                ) [
                    ColorFlash { frame: 0 },
                ]
                View (
                    width: 140,
                    height: 50
                ) [
                    TemporalMix::new(id("tm_src")).with_mix(230),
                ]
            }
            Text (
                "BackgroundBlur",
                text_color: ColorToken::OnSurface,
                width: 240,
                height: 20
            )
            View (
                width: 300,
                height: 150,
                direction: FlexDirection::Column,
                justify: JustifyContent::Center
            ) {
                walk 0..3i32 with i {
                    Row (
                        direction: FlexDirection::Row,
                        justify: JustifyContent::Center,
                        height: 50
                    ) {
                        walk 0..4i32 with j {
                            View (
                                bg_color: tile_color(j + i * 4),
                                width: 75,
                                height: 50
                            )
                        }
                    }
                }
            }
            View (
                bg_color: Color::rgba(255, 255, 255, 50),
                border_radius: 10,
                position: Position::Absolute,
                left: 16,
                top: 259,
                width: 240,
                height: 80
            ) [
                BackgroundBlur::new(8),
                BlurPan(
                    Tween::new(
                            Fixed::from_int(16),
                            Fixed::from_int(100),
                            2200,
                            ease::ease_in_out_cubic,
                            PlayMode::PingPong,
                        )
                        .into(),
                ),
            ]
            Text (
                "DropShadow / DropGlow",
                text_color: ColorToken::OnSurface,
                width: 240,
                height: 20
            )
        }
    };

    // Effect entity spawned before source so it lands earlier in the
    // Children vec — render order = spawn order, shadow paints first
    // then source rect covers it. Reverse spawn = shadow on top of rect.
    let sh_fx = ui! {
        View (
            position: Position::Absolute,
            left: 40,
            top: 460,
            width: 100,
            height: 60
        )
    };
    let sh_src = ui! {
        View (
            bg_color: ColorToken::Primary,
            border_radius: 12,
            position: Position::Absolute,
            left: 40,
            top: 460,
            width: 100,
            height: 60
        )
    };
    cx.world_mut().insert(
        sh_fx,
        DropShadow::new(sh_src)
            .with_blur_radius(6)
            .with_offset(4, 4)
            .with_color(ColorToken::Shadow)
            .with_opacity(160),
    );
    cx.world_mut().insert(
        sh_fx,
        ShadowOffset(
            Tween::new(
                Fixed::from_int(2),
                Fixed::from_int(8),
                1500,
                ease::ease_in_out_cubic,
                PlayMode::PingPong,
            )
            .into(),
        ),
    );

    let gl_fx = ui! {
        View (
            position: Position::Absolute,
            left: 210,
            top: 460,
            width: 100,
            height: 60
        )
    };
    let gl_src = ui! {
        View (
            bg_color: ColorToken::Secondary,
            border_radius: 8,
            position: Position::Absolute,
            left: 210,
            top: 460,
            width: 100,
            height: 60
        )
    };
    cx.world_mut().insert(
        gl_fx,
        DropGlow::new(gl_src)
            .with_blur_radius(12)
            .with_color(ColorToken::Primary)
            .with_opacity(180),
    );
    cx.world_mut().insert(
        gl_fx,
        GlowPulse(
            Tween::new(
                Fixed::from_int(6),
                Fixed::from_int(18),
                1800,
                ease::ease_in_out_cubic,
                PlayMode::PingPong,
            )
            .into(),
        ),
    );
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::app::plugins::StdInstantClockPlugin;
    app.add_plugin(StdInstantClockPlugin)
        .with_theme(Theme::dark())
        .with_offscreen_pool_budget(1024 * 1024)
        .add_system(animate_color_flash::system())
        .add_system(BlurPan::system())
        .add_system(ShadowOffset::system())
        .add_system(GlowPulse::system());
    app.compose(parent, build_widgets);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Children;
    use crate::ui::IdMap;
    use crate::ui::UiScope;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
    }
}
