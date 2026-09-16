extern crate alloc;

use crate::prelude::*;
use crate::types::Transform3D;
use crate::ui::Style;
use crate::ui::root_viewport;
use crate::ui::widgets::{ParagraphStyle, Text, WidgetTransform3D};

pub const DEFAULT_VIEW: (u16, u16) = (480, 320);

pub struct FlipCard {
    pub angle_deg: Fixed,
    pub speed_deg: Fixed,
    pub front_color: Color,
    pub back_color: Color,
    pub root: Entity,
}

//~focus-start
#[mirui_macros::system(order = ANIMATION)]
pub fn flip_system(world: &mut World) {
    // 5/12 and 9/16 reproduce the original 200×180 card in a 480×320 window.
    let (vw, vh) = root_viewport(world)
        .map_or((DEFAULT_VIEW.0 as i32, DEFAULT_VIEW.1 as i32), |r| {
            (r.w.to_int(), r.h.to_int())
        });
    let card_w = vw * 5 / 12;
    let card_h = vh * 9 / 16;
    let card_left = (vw - card_w) / 2;
    let card_top = (vh - card_h) / 2;

    world.for_each_stable::<FlipCard>(|world, e| {
        let (angle, front, back, root) = if let Some(c) = world.get_mut::<FlipCard>(e) {
            c.angle_deg += c.speed_deg;
            if c.angle_deg >= Fixed::from_int(360) {
                c.angle_deg -= Fixed::from_int(360);
            }
            (c.angle_deg, c.front_color, c.back_color, c.root)
        } else {
            return;
        };

        let halfway = Fixed::from_int(90);
        let three_quarters = Fixed::from_int(270);
        let color = if angle < halfway || angle >= three_quarters {
            front
        } else {
            back
        };
        if let Some(style) = world.get_mut::<Style>(e) {
            style.set_bg_color(color);
            style.layout.left = Dimension::px(card_left);
            style.layout.top = Dimension::px(card_top);
            style.layout.width = Dimension::px(card_w);
            style.layout.height = Dimension::px(card_h);
        }

        world.insert(
            e,
            WidgetTransform3D(Transform3D::rotate_y_perspective(
                angle,
                Fixed::from_int(400),
            )),
        );
        world.invalidate(e);
        world.invalidate(root);
    });
}
//~focus-end

#[compose]
pub fn build_widgets() {
    let root = cx.parent();
    ui! {
        View (grow: 1.0) {
            Text (
                "PROJECTIVE FLIP · FRONT / BACK",
                position: Position::Absolute,
                left: 16,
                top: 12,
                width: Dimension::percent(100),
                max_width: 360,
                height: 26,
                font_size: 14,
                text_color: ColorToken::OnSurface
            )
            Column (
                position: Position::Absolute,
                bg_color: Color::rgb(88, 166, 255),
                border_radius: 18,
                align: AlignItems::Center,
                justify: JustifyContent::Center,
                row_gap: 8,
                clip_children: true
            ) [
                FlipCard {
                    angle_deg: super::PROJECTIVE_SPIN_PHASE,
                    speed_deg: Fixed::ONE,
                    front_color: Color::rgb(88, 166, 255),
                    back_color: Color::rgb(248, 81, 73),
                    root,
                },
                WidgetTransform3D(Transform3D::IDENTITY),
            ] {
                Text (
                    "MIRUI",
                    width: Dimension::percent(100),
                    height: 34,
                    font_size: 22,
                    text_color: Color::rgb(255, 255, 255),
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    "2.5D CARD",
                    width: Dimension::percent(100),
                    height: 22,
                    font_size: 10,
                    text_color: Color::rgba(255, 255, 255, 196),
                    paragraph: ParagraphStyle::label()
                )
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
    app.add_system(flip_system::system());
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
