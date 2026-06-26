extern crate alloc;

use crate::anim::{BOUNCY, PlayMode, SMOOTH, Spring, Tween, ease};
use crate::prelude::*;
use crate::render::path::Path;
use crate::ui;
use crate::ui::icons::{
    ICON_ARROW_DOWN, ICON_ARROW_LEFT, ICON_ARROW_RIGHT, ICON_ARROW_UP, ICON_CHECK,
    ICON_CHEVRON_DOWN, ICON_CHEVRON_LEFT, ICON_CHEVRON_RIGHT, ICON_CHEVRON_UP, ICON_CIRCLE,
    ICON_CROSS, ICON_HEART, ICON_HOME, ICON_MINUS, ICON_PAUSE, ICON_PLAY, ICON_PLUS, ICON_SQUARE,
    ICON_STAR, ICON_STOP,
};
use crate::ui::theme::{ColorToken, ThemedColor};
use crate::ui::widgets::icon::Icon;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;

mirui_macros::animate!(IconScale, |world, entity, value| {
    if let Some(icon) = world.get_mut::<Icon>(entity) {
        icon.scale = value;
    }
    world.insert(entity, ui::dirty::Dirty);
});

fn icons() -> [(&'static Path, ColorToken); 20] {
    [
        (&ICON_HOME, ColorToken::Primary),
        (&ICON_CHECK, ColorToken::Success),
        (&ICON_CROSS, ColorToken::Error),
        (&ICON_PLUS, ColorToken::OnSurface),
        (&ICON_MINUS, ColorToken::OnSurface),
        (&ICON_ARROW_LEFT, ColorToken::OnSurfaceVariant),
        (&ICON_ARROW_RIGHT, ColorToken::OnSurfaceVariant),
        (&ICON_ARROW_UP, ColorToken::OnSurfaceVariant),
        (&ICON_ARROW_DOWN, ColorToken::OnSurfaceVariant),
        (&ICON_CHEVRON_LEFT, ColorToken::Primary),
        (&ICON_CHEVRON_RIGHT, ColorToken::Primary),
        (&ICON_CHEVRON_UP, ColorToken::Primary),
        (&ICON_CHEVRON_DOWN, ColorToken::Primary),
        (&ICON_STAR, ColorToken::Primary),
        (&ICON_HEART, ColorToken::Error),
        (&ICON_PLAY, ColorToken::Success),
        (&ICON_PAUSE, ColorToken::OnSurface),
        (&ICON_STOP, ColorToken::Error),
        (&ICON_CIRCLE, ColorToken::OnSurfaceVariant),
        (&ICON_SQUARE, ColorToken::OnSurfaceVariant),
    ]
}

fn beat() -> IconScale {
    IconScale(
        Tween::new(
            Fixed::ONE,
            Fixed::from_f32(1.4),
            600,
            ease::ease_in_out_cubic,
            PlayMode::PingPong,
        )
        .into(),
    )
}

fn breathe() -> IconScale {
    IconScale(
        Spring::preset(Fixed::ONE, Fixed::from_f32(1.25), SMOOTH)
            .repeat()
            .into(),
    )
}

fn bounce() -> IconScale {
    IconScale(
        Spring::preset(Fixed::from_f32(0.8), Fixed::from_f32(1.2), BOUNCY)
            .repeat()
            .into(),
    )
}

pub fn build_widgets(world: &mut World, parent: Entity) {
    let table = icons();
    ui! {
        :(
            parent: parent
            world: world
        :)

        Column (grow: 1.0, padding: Padding::all(16)) {
            walk table.chunks(5) with row {
                Row (height: 56) {
                    walk row.iter() with cell {
                        Icon (
                            path: cell.0.clone(),
                            color: ThemedColor::Token(cell.1),
                            size: Dimension::Px(Fixed::from_int(36)),
                            width: 100,
                            grow: 0.0
                        )
                    }
                }
            }
            Row (height: 96) {
                Icon (
                    path: ICON_HEART.clone(),
                    color: ThemedColor::Token(ColorToken::Error),
                    size: Dimension::Px(Fixed::from_int(48)),
                    width: 100,
                    grow: 0.0
                ) [
                    beat(),
                ]
                Icon (
                    path: ICON_CIRCLE.clone(),
                    color: ThemedColor::Token(ColorToken::Primary),
                    size: Dimension::Px(Fixed::from_int(48)),
                    width: 100,
                    grow: 0.0
                ) [
                    breathe(),
                ]
                Icon (
                    path: ICON_STAR.clone(),
                    color: ThemedColor::Token(ColorToken::Success),
                    size: Dimension::Px(Fixed::from_int(48)),
                    width: 100,
                    grow: 0.0
                ) [
                    bounce(),
                ]
                Icon (
                    path: ICON_PLAY.clone(),
                    color: ThemedColor::Token(ColorToken::Primary),
                    size: Dimension::Px(Fixed::from_int(48)),
                    width: 100,
                    grow: 0.0
                ) [
                    beat(),
                ]
                Icon (
                    path: ICON_PLUS.clone(),
                    color: ThemedColor::Token(ColorToken::OnSurface),
                    size: Dimension::Px(Fixed::from_int(48)),
                    width: 100,
                    grow: 0.0
                ) [
                    bounce(),
                ]
            }
        }
    };
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::ecs;
    app.add_system(ecs::System::new(
        "icon_scale",
        ecs::run_order::ANIMATION,
        IconScale::system(),
    ));
    app.add_plugin(StdInstantClockPlugin);
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

    #[test]
    fn icons_table_has_twenty_entries() {
        assert_eq!(icons().len(), 20);
    }
}
