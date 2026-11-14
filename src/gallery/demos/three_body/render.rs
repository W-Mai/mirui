use super::state::{ARENA, BODY_RADIUS, Orbit, REST, TRAIL};
use crate::gallery::play::paint::PlayPainter;
use crate::prelude::*;
use crate::types::Transform;
use crate::ui::Theme;

/// The stage the orbit is painted on.
#[crate::component]
pub(super) struct Field {
    pub(super) orbit: model!(Orbit),
}

const AMBER: Color = Color::rgb(242, 184, 64);

//~focus-start
#[crate::view(component = Field, name = "ThreeBodyField", priority = 60)]
fn field_render(orbit: model!(component.orbit, watch(frame)), theme: res!(Theme), paint: paint!()) {
    let rect = paint.rect();
    let colors = [
        theme.resolve(ColorToken::Primary),
        AMBER,
        theme.resolve(ColorToken::Error),
    ];
    let guide = theme.resolve(ColorToken::Outline);
    let spring = theme.resolve(ColorToken::OnSurfaceVariant);
    let transform = paint.transform();
    let mut stage = PlayPainter::new(paint, transform);

    // Paint in world units, with the arena centred and fitted to the stage.
    let scale = rect.w.min(rect.h) / (2 * ARENA + 16);
    let mut world = stage.transformed(
        Transform::translate(rect.x + rect.w / 2, rect.y + rect.h / 2)
            .compose(&Transform::scale(scale, scale)),
    );
    let full_turn = Fixed::from_int(360);
    world.arc(
        Point::ZERO,
        Fixed::from_int(ARENA),
        Fixed::ZERO,
        full_turn,
        guide,
        Fixed::ONE,
    );
    world.arc(
        Point::ZERO,
        Fixed::from_int(ARENA / 2),
        Fixed::ZERO,
        full_turn,
        guide.scale_alpha(110),
        Fixed::ONE,
    );

    // A spring glows as it is stretched or squeezed away from its rest length.
    let at = orbit.positions();
    for (i, from) in at.iter().enumerate() {
        for to in &at[i + 1..] {
            let strain = ((*to - *from).length() - Fixed::from_int(REST)).abs();
            let glow = (40 + strain.to_int() * 4).min(190) as u8;
            world.line(
                (*from).into(),
                (*to).into(),
                spring.scale_alpha(glow),
                Fixed::ONE,
            );
        }
    }

    for (body, color) in orbit.bodies.iter().zip(colors) {
        // The tail thickens and brightens towards the body.
        for (age, past) in body.trail().enumerate() {
            let fade = (age * 90 / TRAIL) as u8;
            let size = Fixed::ONE + Fixed::from_ratio(age as i32 * 3, TRAIL as i32);
            world.disc(past.into(), size, color.scale_alpha(fade));
        }
        let centre: Point = body.at.into();
        world.disc(
            centre,
            Fixed::from_int(2 * BODY_RADIUS),
            color.scale_alpha(46),
        );
        world.disc(centre, Fixed::from_int(BODY_RADIUS), color);
        let shine = Point::new(
            centre.x - Fixed::from_ratio(3, 2),
            centre.y - Fixed::from_ratio(3, 2),
        );
        world.disc(
            shine,
            Fixed::from_ratio(3, 2),
            Color::rgba(255, 255, 255, 190),
        );
    }
}
//~focus-end

pub use field_render::view as field_view;
