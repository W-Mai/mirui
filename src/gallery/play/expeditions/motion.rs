//! Motion the expedition games share. Each game's director system compares
//! its model with what the board last showed and starts these motions; the
//! board views read [`BoardMotion`] and draw the pieces in between.

use super::{Direction4, ExpeditionUiModel};
use crate::anim::{BOUNCY, Motion, SNAPPY, Spring, Tween, TweenConfig, ease};
use crate::core::model::Model;
use crate::ecs::commands::DeferredCommands;
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::paint::PlayPainter;
use crate::prelude::*;
use crate::render::path::Path;
use crate::types::Transform;

/// A move lands on its new cells in this time.
const TRAVEL: TweenConfig = TweenConfig::new(150, &ease::OUT_CUBIC);
/// How fast, in board pixels per second, a refused move pushes off.
const BUMP_SPEED: i32 = 260;
/// How long the banner naming a new level stays up.
const BANNER: TweenConfig = TweenConfig::new(1400, &ease::LINEAR);
/// A result star sets off the next one once it has grown this far.
const STAR_HANDOFF: Fixed = Fixed::from_ratio(1, 2);
/// The logical size of the result star row.
const STAR_ROW: (i32, i32) = (108, 34);

/// A five-pointed star of radius one around the origin, point up.
static STAR: Path = crate::path!(
    "M 0 -1 L 0.2469 -0.3398 L 0.9511 -0.309 L 0.3995 0.1298 L 0.5878 0.809 L 0 0.42 L -0.5878 0.809 L -0.3995 0.1298 L -0.9511 -0.309 L -0.2469 -0.3398 Z"
);

/// Where a board's pieces are between two model states: `travel` runs from
/// 0 on the old cells to 1 on the new ones, `bump` is how far, in board
/// pixels, a refused move pushes them.
#[crate::component]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BoardMotion {
    pub(crate) travel: Fixed,
    pub(crate) bump: Fixed,
}

impl Default for BoardMotion {
    fn default() -> Self {
        Self {
            travel: Fixed::ONE,
            bump: Fixed::ZERO,
        }
    }
}

impl BoardMotion {
    /// `from` carried `travel` of the way to `to`.
    pub(crate) fn along(self, from: Point, to: Point) -> Point {
        Point::new(
            from.x + (to.x - from.x) * self.travel,
            from.y + (to.y - from.y) * self.travel,
        )
    }

    /// `point` pushed `bump` pixels toward `direction`.
    pub(crate) fn bumped(self, point: Point, direction: Direction4) -> Point {
        let (dx, dy) = direction.delta();
        Point::new(
            point.x + self.bump * Fixed::from_int(i32::from(dx)),
            point.y + self.bump * Fixed::from_int(i32::from(dy)),
        )
    }

    /// Starts the pieces on their way from the old cells.
    pub(crate) fn travel(commands: &mut DeferredCommands<'_>, board: Entity) {
        commands.insert(
            board,
            Self {
                travel: Fixed::ZERO,
                bump: Fixed::ZERO,
            },
        );
        commands.remove::<Bump>(board);
        commands.insert(
            board,
            Travel(Tween::preset(Fixed::ZERO, Fixed::ONE, TRAVEL).into()),
        );
    }

    /// Pushes the pieces toward a refused move; they spring back.
    pub(crate) fn bump(commands: &mut DeferredCommands<'_>, board: Entity) {
        commands.insert(
            board,
            Bump(
                Spring::preset(Fixed::ZERO, Fixed::ZERO, SNAPPY)
                    .with_velocity(Fixed::from_int(BUMP_SPEED))
                    .into(),
            ),
        );
    }

    /// Shows the board at rest on its current cells.
    pub(crate) fn settle(commands: &mut DeferredCommands<'_>, board: Entity) {
        commands.remove::<Travel>(board);
        commands.remove::<Bump>(board);
        commands.insert(board, Self::default());
    }

    /// Keeps the level banner up for a moment; it hides by itself.
    pub(crate) fn banner(commands: &mut DeferredCommands<'_>, board: Entity) {
        commands.insert(
            board,
            BannerLife(Tween::preset(Fixed::ZERO, Fixed::ONE, BANNER).into()),
        );
    }
}

#[animate(component = Travel)]
pub(crate) fn apply_travel(value: motion!(), board: component!(mut BoardMotion)) {
    board.travel = value;
}

#[animate(component = Bump)]
pub(crate) fn apply_bump(value: motion!(), board: component!(mut BoardMotion)) {
    board.bump = value;
}

#[animate(component = BannerLife)]
pub(crate) fn age_banner(value: motion!(), expedition: model!(ExpeditionUiModel)) {
    if value >= Fixed::ONE {
        expedition.hide_banner();
    }
}

/// The three stars of a result card: how many are earned and how far each
/// has popped in. Unearned stars pop in too, dim.
#[crate::component]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ResultStars {
    earned: u8,
    started: u8,
    scale: [Fixed; 3],
    lit: Color,
    unlit: Color,
}

impl ResultStars {
    pub(crate) const fn new(lit: Color, unlit: Color) -> Self {
        Self {
            earned: 0,
            started: 0,
            scale: [Fixed::ZERO; 3],
            lit,
            unlit,
        }
    }

    #[cfg(test)]
    pub(crate) const fn earned(&self) -> u8 {
        self.earned
    }

    #[cfg(test)]
    pub(crate) const fn scales(&self) -> [Fixed; 3] {
        self.scale
    }
}

fn star_pop() -> Motion {
    Spring::preset(Fixed::ZERO, Fixed::ONE, BOUNCY).into()
}

/// Pops the stars of a result card in, one after another, `earned` of them
/// lit.
pub(crate) fn start_result_stars(
    commands: &mut DeferredCommands<'_>,
    entity: Entity,
    stars: ResultStars,
    earned: u8,
) {
    commands.remove::<SecondStar>(entity);
    commands.remove::<ThirdStar>(entity);
    commands.insert(
        entity,
        ResultStars {
            earned,
            started: 1,
            scale: [Fixed::ZERO; 3],
            ..stars
        },
    );
    commands.insert(entity, FirstStar(star_pop()));
}

#[animate(component = FirstStar)]
pub(crate) fn pop_first_star(
    value: motion!(),
    stars: component!(mut ResultStars),
    entity: entity!(),
    commands: commands!(),
) {
    stars.scale[0] = value;
    if stars.started == 1 && value >= STAR_HANDOFF {
        stars.started = 2;
        commands.insert(entity, SecondStar(star_pop()));
    }
}

#[animate(component = SecondStar)]
pub(crate) fn pop_second_star(
    value: motion!(),
    stars: component!(mut ResultStars),
    entity: entity!(),
    commands: commands!(),
) {
    stars.scale[1] = value;
    if stars.started == 2 && value >= STAR_HANDOFF {
        stars.started = 3;
        commands.insert(entity, ThirdStar(star_pop()));
    }
}

#[animate(component = ThirdStar)]
pub(crate) fn pop_third_star(value: motion!(), stars: component!(mut ResultStars)) {
    stars.scale[2] = value;
}

#[crate::view(component = ResultStars, name = "ResultStars", priority = 60)]
fn result_stars_render(stars: component!(), paint: paint!()) {
    let (width, height) = STAR_ROW;
    let transform = fit_logical_canvas(paint.rect(), paint.transform(), width, height);
    let mut painter = PlayPainter::new(paint, transform);
    let pitch = width / 3;
    for (slot, scale) in stars.scale.iter().copied().enumerate() {
        if scale <= Fixed::ZERO {
            continue;
        }
        let color = if slot < usize::from(stars.earned) {
            stars.lit
        } else {
            stars.unlit
        };
        let radius = Fixed::from_int(height / 2 - 2) * scale;
        let center = Transform::translate(
            Fixed::from_int(pitch * slot as i32 + pitch / 2),
            Fixed::from_int(height / 2 + 1),
        );
        painter
            .transformed(center.compose(&Transform::scale(radius, radius)))
            .fill_path(&STAR, color);
    }
}

/// Registers the shared motion: board travel and bumps, the level banner
/// and the result stars.
pub(crate) fn install_expedition_motion<B, F>(
    app: &mut App<B, F>,
    expedition: &<ExpeditionUiModel as Model>::Handle,
) where
    B: Surface,
    F: RendererFactory<B>,
{
    app.with_widget(result_stars_render::view());
    app.add_system(Travel::system())
        .add_system(Bump::system())
        .add_system(BannerLife::system(expedition.clone()))
        .add_system(FirstStar::system())
        .add_system(SecondStar::system())
        .add_system(ThirdStar::system());
}
