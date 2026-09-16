extern crate alloc;

use crate::prelude::*;
use crate::ui;
use crate::ui::root_viewport;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

const BAR_W: i32 = 50;
const RIGHT_MARGIN: i32 = 10;

pub const DEFAULT_VIEW: (u16, u16) = (480, 320);

pub struct BarState {
    pub y: Fixed,
    pub speed: Fixed,
    pub snap: bool,
    pub x: Fixed,
    pub right_anchored: bool,
}

pub struct BarBounds {
    pub w: i32,
    pub h: i32,
}

//~focus-start
#[mirui_macros::system(order = ANIMATION)]
pub fn bar_move_system(world: &mut World) {
    if let Some(rect) = root_viewport(world) {
        world.insert_resource(BarBounds {
            w: rect.w.to_int(),
            h: rect.h.to_int(),
        });
    }
    let (bound_w, bound_h) = world
        .resource::<BarBounds>()
        .map(|b| (b.w, b.h))
        .unwrap_or((DEFAULT_VIEW.0 as i32, DEFAULT_VIEW.1 as i32));
    world.for_each_stable::<BarState>(|world, e| {
        let (new_x, new_y, changed) = {
            let Some(bar) = world.get_mut::<BarState>(e) else {
                return;
            };
            if bar.right_anchored {
                bar.x = Fixed::from_int(bound_w - BAR_W - RIGHT_MARGIN);
            }
            let old_display = if bar.snap { bar.y.floor() } else { bar.y };
            bar.y += bar.speed;
            if bar.y > Fixed::from_int(bound_h - 18) {
                bar.y = Fixed::from_int(58);
            }
            let new_display = if bar.snap { bar.y.floor() } else { bar.y };
            (
                bar.x,
                new_display,
                new_display != old_display || bar.right_anchored,
            )
        };
        if changed {
            ui::set_position(world, e, new_x, new_y);
        }
    });
}
//~focus-end

#[compose]
pub fn build_widgets() {
    ui! {
        View (grow: 1.0, clip_children: true) {
            Row (
                position: Position::Absolute,
                left: 0,
                top: 12,
                width: Dimension::percent(100),
                height: 30,
                align: AlignItems::Center,
                padding: Padding {
                    top: Dimension::px(0),
                    right: Dimension::px(16),
                    bottom: Dimension::px(0),
                    left: Dimension::px(16),
                }
            ) {
                Text (
                    "PIXEL-SNAPPED",
                    grow: 1.0,
                    font_size: 11,
                    text_color: Color::rgb(255, 112, 122),
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "Q24.8 SUBPIXEL",
                    grow: 1.0,
                    font_size: 11,
                    text_color: Color::rgb(112, 202, 255),
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
            }
            View (
                bg_color: Color::rgb(255, 100, 110),
                position: Position::Absolute,
                left: 10,
                top: 58,
                width: BAR_W,
                height: 8,
                border_radius: 4
            ) [
                BarState {
                    y: Fixed::from_int(58),
                    speed: Fixed::from_ratio(9, 256),
                    snap: true,
                    x: Fixed::from_int(10),
                    right_anchored: false,
                },
            ]
            View (
                bg_color: Color::rgb(100, 200, 255),
                position: Position::Absolute,
                left: 0,
                top: 58,
                width: BAR_W,
                height: 8,
                border_radius: 4
            ) [
                BarState {
                    y: Fixed::from_int(58),
                    speed: Fixed::from_ratio(9, 256),
                    snap: false,
                    x: Fixed::ZERO,
                    right_anchored: true,
                },
            ]
        }
    };
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_system(bar_move_system::system());
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
