use super::state::Butterfly;
use super::wings::{
    ANTENNA_TIPS, ANTENNAE, BODY, DISC, EYE_SPOT, FOREWING, FOREWING_GLOW, FOREWING_SPOTS,
    FOREWING_VEINS, HINDWING, HINDWING_GLOW, HINDWING_VEINS,
};
use crate::gallery::play::paint::PlayPainter;
use crate::prelude::*;
use crate::types::Transform;
use crate::ui::Theme;

/// The colours of the butterfly, taken from the theme.
struct Palette {
    wing: Color,
    glow: Color,
    edge: Color,
    spot: Color,
    body: Color,
    feeler: Color,
}

impl Palette {
    fn of(theme: &Theme) -> Self {
        let wing = theme.resolve(ColorToken::Primary);
        Self {
            wing,
            glow: theme.resolve(ColorToken::Secondary),
            edge: wing.blend_with(Color::rgb(0, 0, 0), Fixed::from_ratio(45, 100)),
            spot: Color::rgba(255, 255, 255, 230),
            body: theme.resolve(ColorToken::OnSurface),
            feeler: theme.resolve(ColorToken::OnSurfaceVariant),
        }
    }
}

/// How far a pair of wings is open, from almost closed to fully open. The
/// easing makes the beat linger at both ends, as a real flap does.
fn openness(flap: Fixed) -> Fixed {
    let raised = (Fixed::ONE + Fixed::cos_deg(flap)) / 2;
    let eased = raised * raised * (Fixed::from_int(3) - raised * 2);
    Fixed::from_ratio(12, 100) + eased * Fixed::from_ratio(88, 100)
}

fn spot(painter: &mut PlayPainter<'_, '_>, (x, y): (i32, i32), radius: i32, color: Color) {
    painter.disc(Point::new(x, y), Fixed::from_int(radius), color);
}

fn forewing(painter: &mut PlayPainter<'_, '_>, colors: &Palette) {
    painter.fill_path(&FOREWING, colors.wing);
    painter.fill_path(&FOREWING_GLOW, colors.glow);
    painter.stroke_path(&FOREWING_VEINS, colors.edge.scale_alpha(150), Fixed::ONE);
    painter.stroke_path(&FOREWING, colors.edge, Fixed::from_ratio(3, 2));
    for (x, y, radius) in FOREWING_SPOTS {
        spot(painter, (x, y), radius, colors.spot);
    }
}

fn hindwing(painter: &mut PlayPainter<'_, '_>, colors: &Palette) {
    painter.fill_path(&HINDWING, colors.wing);
    painter.fill_path(&HINDWING_GLOW, colors.glow);
    painter.stroke_path(&HINDWING_VEINS, colors.edge.scale_alpha(150), Fixed::ONE);
    painter.stroke_path(&HINDWING, colors.edge, Fixed::from_ratio(3, 2));
    spot(painter, EYE_SPOT, 9, colors.edge);
    spot(painter, EYE_SPOT, 6, colors.spot);
    spot(painter, EYE_SPOT, 3, colors.edge);
}

//~focus-start
#[crate::view(component = Butterfly, name = "Butterfly", priority = 60)]
fn butterfly_render(state: component!(), theme: res!(Theme), paint: paint!()) {
    let rect = paint.rect();
    let colors = Palette::of(&theme);
    let transform = paint.transform();
    let mut painter = PlayPainter::new(paint, transform);

    // One drawing unit, so the wings fill the same share of any stage.
    let unit = rect.w.min(rect.h) / 230;
    let middle = Point {
        x: rect.x + rect.w / 2,
        y: rect.y + rect.h / 2,
    };

    // Drifting loop, with a small lift on every beat.
    let at = Point {
        x: middle.x + Fixed::sin_deg(state.drift_x) * (rect.w / 4),
        y: middle.y + Fixed::sin_deg(state.drift_y) * (rect.h / 6)
            - Fixed::cos_deg(state.flap) * unit * 3,
    };
    // Lean into the direction of travel, and favour one wing in the turns.
    let bank = Fixed::cos_deg(state.drift_x) * 10;
    let yaw = Fixed::sin_deg(state.drift_y) * Fixed::from_ratio(15, 100);
    let open = openness(state.flap);
    let sides = [
        (
            Fixed::from_int(-1),
            (open * (Fixed::ONE + yaw)).min(Fixed::ONE),
        ),
        (Fixed::ONE, (open * (Fixed::ONE - yaw)).min(Fixed::ONE)),
    ];

    // A soft shadow on the ground that narrows as the wings close.
    let ground = rect.y + rect.h * 4 / 5;
    painter
        .transformed(
            Transform::translate(at.x, ground)
                .compose(&Transform::scale(unit * 90 * open, unit * 9)),
        )
        .fill_path(&DISC, Color::rgba(0, 0, 0, 50));

    let mut butterfly = painter.transformed(
        Transform::translate(at.x, at.y)
            .compose(&Transform::rotate_deg(bank))
            .compose(&Transform::scale(unit, unit)),
    );
    for (side, open) in sides {
        hindwing(
            &mut butterfly.transformed(Transform::scale(side * open, Fixed::ONE)),
            &colors,
        );
    }
    for (side, open) in sides {
        forewing(
            &mut butterfly.transformed(Transform::scale(side * open, Fixed::ONE)),
            &colors,
        );
    }
    butterfly.fill_path(&BODY, colors.body);
    butterfly.stroke_path(&ANTENNAE, colors.feeler, Fixed::from_ratio(3, 2));
    for tip in ANTENNA_TIPS {
        spot(&mut butterfly, tip, 3, colors.feeler);
    }
}
//~focus-end

pub use butterfly_render::view as butterfly_view;
