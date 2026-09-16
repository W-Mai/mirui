extern crate alloc;

use crate::prelude::*;
use crate::types::Transform3D;
use crate::ui::widgets::{Image, ParagraphStyle, Text, TextAlign, WidgetTransform3D};

pub struct Spinner {
    pub angle: Fixed,
    pub speed: Fixed,
    pub bounce_phase: Fixed,
}

//~focus-start
#[mirui_macros::system(order = ANIMATION)]
pub fn spin_system(world: &mut World) {
    world.for_each_stable::<Spinner>(|world, e| {
        let (angle, bounce) = if let Some(s) = world.get_mut::<Spinner>(e) {
            s.angle += s.speed;
            if s.angle >= Fixed::from_int(360) {
                s.angle -= Fixed::from_int(360);
            }
            s.bounce_phase += s.speed;
            if s.bounce_phase >= Fixed::from_int(360) {
                s.bounce_phase -= Fixed::from_int(360);
            }
            (s.angle, s.bounce_phase)
        } else {
            return;
        };

        let t_num = bounce.to_int() % 180;
        let t = Fixed::from_int(t_num) / Fixed::from_int(180);
        let two_t_minus_1 = t * Fixed::from_int(2) - Fixed::ONE;
        let h = Fixed::ONE - two_t_minus_1 * two_t_minus_1;

        let bounce_y = Fixed::ZERO - h * Fixed::from_int(100);
        let squash = Fixed::ONE - (Fixed::ONE - h) / Fixed::from_int(4);
        let stretch = Fixed::ONE + h / Fixed::from_int(8);

        let rot = Transform3D::rotate_y_perspective(angle, Fixed::from_int(400));
        let scale = Transform3D::scale(squash, stretch);
        let translate = Transform3D::translate(Fixed::ZERO, bounce_y);
        world.insert(
            e,
            WidgetTransform3D(translate.compose(&rot).compose(&scale)),
        );
        world.invalidate(e);
    });
}
//~focus-end

#[compose]
pub fn build_widgets() {
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            padding: Padding::all(12),
            row_gap: 8
        ) {
            Text (
                "PROJECTIVE IMAGE",
                width: Dimension::percent(100),
                max_width: 440,
                height: 28,
                font_size: 17,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Column (
                width: Dimension::percent(100),
                max_width: 440,
                grow: 1.0,
                align: AlignItems::Center,
                justify: JustifyContent::FlexEnd,
                padding: Padding::all(24),
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 18,
                clip_children: true
            ) {
                Image (
                    width: 120,
                    height: 120,
                    src: "thumbs_up"
                ) [
                    Spinner {
                        angle: super::PROJECTIVE_SPIN_PHASE,
                        speed: Fixed::from_int(3),
                        bounce_phase: Fixed::ZERO,
                    },
                    WidgetTransform3D(Transform3D::IDENTITY),
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
    app.add_system(spin_system::system());
    app.add_plugin(crate::app::plugins::ImageResourcesPlugin::default());
    app.compose(parent, build_widgets);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{Children, IdMap, UiScope};

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
        drop(cx);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
    }
}
