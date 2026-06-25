use crate::ecs::{Entity, World};
use crate::render::command::DrawCommand;
use crate::render::path::Path;
use crate::render::renderer::Renderer;
use crate::types::{Dimension, Fixed, Rect, Transform};
use crate::ui::theme::{ColorToken, ThemedColor};
use crate::ui::view::{View, ViewCtx};

/// `scale` is a unitless multiplier orthogonal to `size`. Animate
/// `scale` (e.g. through `mirui_macros::animate!`) so hover bounces
/// don't replace the `Dimension::Percent` / `Auto` variant on `size`.
#[derive(Clone, Debug, crate::Component)]
#[non_exhaustive]
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
    pub fn new(path: Path) -> Self {
        Self {
            path,
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

fn icon_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(icon) = world.get::<Icon>(entity) else {
        return;
    };
    let theme = ctx.theme(world);
    let color = icon.color.resolve_in(theme, ctx.state);

    let size_px = icon.size.resolve_or(rect.w, rect.w);
    if icon.viewbox <= Fixed::ZERO || size_px <= Fixed::ZERO {
        return;
    }
    let effective = (size_px / icon.viewbox) * icon.scale;

    let scaled = ctx
        .transform
        .compose(&Transform::translate(rect.x, rect.y))
        .compose(&Transform::scale(effective, effective));

    renderer.draw(
        &DrawCommand::FillPath {
            path: &icon.path,
            transform: scaled,
            color,
            opa: 255,
        },
        ctx.clip,
    );
}

pub fn view() -> View {
    View::new("Icon", 70, icon_render).with_filter::<Icon>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::{Motion, MotionComponent, Spring, run_motion};
    use crate::ecs::{DeltaTimeMs, MonoClock};
    use crate::render::path::PathCmd;
    use crate::types::{Color, Point};
    use crate::ui::dirty::Dirty;
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

    // Stand-in for an `animate!`-generated component so this invariant
    // test doesn't need a public type just for itself.
    struct IconScale(Motion);

    impl crate::ecs::Component for IconScale {}

    impl MotionComponent for IconScale {
        fn motion(&self) -> &Motion {
            &self.0
        }
        fn motion_mut(&mut self) -> &mut Motion {
            &mut self.0
        }
    }

    fn tick(world: &mut World) {
        run_motion::<IconScale>(world, |world, entity, value| {
            if let Some(icon) = world.get_mut::<Icon>(entity) {
                icon.scale = value;
            }
            world.insert(entity, Dirty);
        });
    }

    #[test]
    fn animating_scale_leaves_size_dimension_untouched() {
        fn clock() -> u64 {
            0
        }

        let mut world = World::new();
        world.insert_resource(MonoClock::new(clock));
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

        for _ in 0..20 {
            tick(&mut world);
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
