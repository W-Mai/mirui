extern crate alloc;

use crate::ecs::DeltaTimeMs;
use crate::prelude::*;
use crate::types::Transform3D;
use crate::ui::widgets::{ParagraphStyle, Text, TransformOrigin, WidgetTransform3D};

pub struct Page {
    pub angle_deg: Fixed,
    pub speed_deg_per_second: Fixed,
}

#[mirui_macros::system(order = ANIMATION)]
pub fn flip_system(world: &mut World) {
    let dt = world
        .resource::<DeltaTimeMs>()
        .map_or(16, |delta| delta.0)
        .min(50);
    world.for_each_stable::<Page>(|world, e| {
        let angle = if let Some(p) = world.get_mut::<Page>(e) {
            p.angle_deg += p.speed_deg_per_second * Fixed::from_ratio(i32::from(dt), 1_000);
            let limit = Fixed::from_int(120);
            if p.angle_deg > limit {
                p.angle_deg = limit * 2 - p.angle_deg;
                p.speed_deg_per_second = -p.speed_deg_per_second;
            } else if p.angle_deg < Fixed::ZERO {
                p.angle_deg = -p.angle_deg;
                p.speed_deg_per_second = -p.speed_deg_per_second;
            }
            p.angle_deg
        } else {
            return;
        };
        world.insert(
            e,
            WidgetTransform3D(Transform3D::rotate_y_perspective(
                Fixed::ZERO - angle,
                Fixed::from_int(500),
            )),
        );
        world.invalidate(e);
    });
}

#[compose]
pub fn build_widgets() {
    //~focus-start
    ui! {
        Column (
            id: "book_flip_shell",
            grow: 1.0,
            padding: Padding::all(16),
            row_gap: 12,
            bg_color: ColorToken::Surface
        ) {
            Text (
                "PROJECTIVE BOOK",
                width: Dimension::percent(100),
                max_width: 440,
                height: 28,
                font_size: 17,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label()
            )
            Row (
                id: "book_flip_spread",
                grow: 1.0,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) {
                Column (
                    id: "book_flip_left_page",
                    width: Dimension::percent(34),
                    min_width: 112,
                    max_width: 180,
                    height: Dimension::percent(78),
                    min_height: 160,
                    max_height: 240,
                    padding: Padding::all(16),
                    row_gap: 10,
                    bg_color: ColorToken::SurfaceVariant,
                    border_color: ColorToken::Outline,
                    border_width: 1,
                    border_radius: 8
                ) {
                    Text (
                        "MIRUI",
                        height: 34,
                        font_size: 22,
                        text_color: ColorToken::OnSurface,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "retained geometry\nfixed-point motion",
                        grow: 1.0,
                        font_size: 11,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "01",
                        height: 24,
                        font_size: 10,
                        text_color: ColorToken::Secondary,
                        paragraph: ParagraphStyle::label()
                    )
                }
                Column (
                    id: "book_flip_right_page",
                    width: Dimension::percent(34),
                    min_width: 112,
                    max_width: 180,
                    height: Dimension::percent(78),
                    min_height: 160,
                    max_height: 240,
                    padding: Padding::all(16),
                    row_gap: 10,
                    bg_color: ColorToken::Secondary,
                    border_color: ColorToken::Outline,
                    border_width: 1,
                    border_radius: 8
                ) [
                    TransformOrigin {
                        x: Fixed::ZERO,
                        y: Fixed::ONE / 2,
                    },
                    Page {
                        angle_deg: super::PROJECTIVE_SPIN_PHASE,
                        speed_deg_per_second: Fixed::from_int(30),
                    },
                    WidgetTransform3D(Transform3D::IDENTITY),
                ] {
                    Text (
                        "LIVE PAGE",
                        height: 34,
                        font_size: 18,
                        text_color: ColorToken::OnSecondary,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "one node\none transform",
                        grow: 1.0,
                        font_size: 11,
                        text_color: ColorToken::OnSecondary,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "02",
                        height: 24,
                        font_size: 10,
                        text_color: ColorToken::OnSecondary,
                        paragraph: ParagraphStyle::label()
                    )
                }
            }
        }
    };
    //~focus-end
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_system(flip_system::system());
    app.compose(parent, build_widgets);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::DeltaTimeMs;
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

    #[test]
    fn page_motion_uses_frame_delta_and_reflects_at_bounds() {
        let mut world = World::new();
        world.insert_resource(DeltaTimeMs(20));
        let page = world.spawn_empty();
        world.insert(
            page,
            Page {
                angle_deg: Fixed::ZERO,
                speed_deg_per_second: Fixed::from_int(30),
            },
        );

        flip_system(&mut world);
        assert_eq!(
            world.get::<Page>(page).unwrap().angle_deg,
            Fixed::from_int(30) * Fixed::from_ratio(20, 1_000)
        );

        let state = world.get_mut::<Page>(page).unwrap();
        state.angle_deg = Fixed::from_int(120);
        flip_system(&mut world);
        let state = world.get::<Page>(page).unwrap();
        assert!(state.angle_deg < Fixed::from_int(120));
        assert!(state.speed_deg_per_second < Fixed::ZERO);
    }
}
