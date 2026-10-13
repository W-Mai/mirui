use crate::render::path::Path;
use crate::types::{Dimension, Fixed, Rect, Transform};
use crate::ui::theme::{ColorToken, ThemedColor};

/// `scale` is a unitless multiplier orthogonal to `size`. Animate
/// `scale` (e.g. through `#[mirui::animate]`) so hover bounces
/// don't replace the `Dimension::Percent` / `Auto` variant on `size`.
#[crate::component]
#[derive(Clone, Debug)]
pub struct Icon {
    pub path: Path,
    pub color: ThemedColor,
    pub size: Dimension,
    pub viewbox: Fixed,
    pub scale: Fixed,
}

impl Default for Icon {
    fn default() -> Self {
        Self::new(Path::new())
    }
}

impl Icon {
    pub fn new(path: impl Into<Path>) -> Self {
        Self {
            path: path.into(),
            color: ThemedColor::Token(ColorToken::OnSurface),
            size: Dimension::Auto,
            viewbox: Fixed::from_int(24),
            scale: Fixed::ONE,
        }
    }

    pub fn with_color(mut self, color: ThemedColor) -> Self {
        self.color = color;
        self
    }

    pub fn with_size(mut self, size: Dimension) -> Self {
        self.size = size;
        self
    }

    pub fn with_viewbox(mut self, viewbox: Fixed) -> Self {
        self.viewbox = viewbox;
        self
    }

    pub fn with_scale(mut self, scale: Fixed) -> Self {
        self.scale = scale;
        self
    }
}

fn icon_size(icon: &Icon, rect: &Rect) -> Option<Fixed> {
    let available = rect.w.min(rect.h);
    let size_px = icon.size.resolve_or(available, available);
    if icon.viewbox <= Fixed::ZERO || size_px <= Fixed::ZERO || icon.scale <= Fixed::ZERO {
        return None;
    }
    let rendered_size = size_px * icon.scale;
    Some(rendered_size)
}

fn icon_transform(icon: &Icon, rect: &Rect, parent: Transform) -> Option<Transform> {
    super::image::vector_transform(rect, icon.viewbox, icon_size(icon, rect)?, parent)
}

#[crate::view(component = Icon, name = "Icon", priority = 70)]
fn icon_render(icon: component!(), theme: res!(crate::ui::Theme), paint: paint!()) {
    let color = icon.color.resolve_in(&theme, paint.state());
    let rect = paint.rect();

    let Some(transform) = icon_transform(icon, &rect, paint.transform()) else {
        return;
    };
    super::image::draw_vector_transformed(paint, &icon.path, color, transform);
}

pub use icon_render::view;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::Spring;
    use crate::ecs::DeltaTimeMs;
    use crate::ecs::World;
    use crate::render::path::PathCmd;
    use crate::types::{Color, Point};
    use alloc::borrow::Cow;

    static CLOSED_PATH: &[PathCmd] = &[PathCmd::Close];

    #[test]
    fn icon_new_defaults_match_design() {
        let icon = Icon::new(Path::from_static(CLOSED_PATH));
        assert_eq!(icon.size, Dimension::Auto);
        assert_eq!(icon.viewbox, Fixed::from_int(24));
        assert_eq!(icon.scale, Fixed::ONE);
        assert!(matches!(
            icon.color,
            ThemedColor::Token(ColorToken::OnSurface)
        ));
    }

    #[test]
    fn icon_builders_chain() {
        let icon = Icon::new(Path::from_static(CLOSED_PATH))
            .with_color(ThemedColor::Raw(Color::rgb(255, 0, 0)))
            .with_size(Dimension::Px(Fixed::from_int(32)))
            .with_viewbox(Fixed::from_int(16))
            .with_scale(Fixed::from_int(2));
        assert_eq!(icon.size, Dimension::Px(Fixed::from_int(32)));
        assert_eq!(icon.viewbox, Fixed::from_int(16));
        assert_eq!(icon.scale, Fixed::from_int(2));
        assert!(matches!(icon.color, ThemedColor::Raw(_)));
    }

    // Cloning an Icon whose Path borrows a 'static slice must stay
    // Borrowed — the whole point of the Path Cow is heap-constrained
    // MCUs. If a refactor ever promotes to Owned here, this test
    // catches it.
    #[test]
    fn icon_clone_keeps_path_borrowed() {
        static CMDS: &[PathCmd] = &[
            PathCmd::MoveTo(Point {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            }),
            PathCmd::Close,
        ];
        let icon = Icon::new(Path::from_static(CMDS));
        let cloned = icon.clone();
        assert!(matches!(cloned.path.cmds, Cow::Borrowed(_)));
    }

    #[test]
    fn explicit_icon_size_is_centered_inside_its_layout_rect() {
        let icon =
            Icon::new(Path::from_static(CLOSED_PATH)).with_size(Dimension::Px(Fixed::from_int(24)));
        let rect = Rect {
            x: Fixed::from_int(10),
            y: Fixed::from_int(20),
            w: Fixed::from_int(44),
            h: Fixed::from_int(60),
        };
        let transform = icon_transform(&icon, &rect, Transform::IDENTITY).unwrap();
        assert_eq!(transform.tx, Fixed::from_int(20));
        assert_eq!(transform.ty, Fixed::from_int(38));
        assert_eq!(transform.m00, Fixed::ONE);
        assert_eq!(transform.m11, Fixed::ONE);
    }

    #[test]
    fn animated_icon_scale_remains_centered() {
        let icon = Icon::new(Path::from_static(CLOSED_PATH))
            .with_size(Dimension::Px(Fixed::from_int(24)))
            .with_scale(Fixed::from_ratio(3, 2));
        let rect = Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: Fixed::from_int(60),
            h: Fixed::from_int(60),
        };
        let transform = icon_transform(&icon, &rect, Transform::IDENTITY).unwrap();
        assert_eq!(transform.tx, Fixed::from_int(12));
        assert_eq!(transform.ty, Fixed::from_int(12));
        assert_eq!(transform.m00, Fixed::from_ratio(3, 2));
        assert_eq!(transform.m11, Fixed::from_ratio(3, 2));
    }

    #[crate::animate(component = IconScale)]
    fn apply_icon_scale(value: motion!(), icon: component!(mut Icon)) {
        icon.scale = value;
    }

    #[test]
    fn animating_scale_leaves_size_dimension_untouched() {
        let mut world = World::new();
        world.insert_resource(DeltaTimeMs(16));

        let icon = Icon::new(Path::from_static(CLOSED_PATH))
            .with_size(Dimension::Percent(Fixed::from_int(50)))
            .with_scale(Fixed::ONE);
        let entity = world.spawn_empty();
        world.insert(entity, icon);
        world.insert(
            entity,
            IconScale(Spring::new(Fixed::ONE, Fixed::from_int(2), 100, Fixed::ZERO).into()),
        );

        let system = IconScale::system();
        for _ in 0..20 {
            system.run_once(&mut world).unwrap();
        }

        let after = world.get::<Icon>(entity).expect("icon survives");
        assert!(
            matches!(after.size, Dimension::Percent(_)),
            "size variant must stay Percent after animation, got {:?}",
            after.size,
        );
        assert!(
            after.scale > Fixed::ONE,
            "scale should have advanced toward 2.0, got {:?}",
            after.scale,
        );
    }
}
