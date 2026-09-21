extern crate alloc;

use crate::prelude::*;
use crate::ui;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

const BAR_W: i32 = 50;
const BAR_H: i32 = 8;
const BAR_MARGIN: i32 = 10;
const START_Y: i32 = 12;

pub const DEFAULT_VIEW: (u16, u16) = (480, 320);

pub struct BarState {
    pub y: Fixed,
    pub speed_per_second: Fixed,
    pub snap: bool,
    pub x: Fixed,
    pub right_anchored: bool,
}

#[derive(Clone, Copy)]
struct BarArena(Entity);

//~focus-start
#[mirui_macros::system(order = ANIMATION)]
pub fn bar_move_system(world: &mut World) {
    let Some(arena) = world.resource::<BarArena>().copied() else {
        return;
    };
    let Some(bounds) = world.get::<ui::ComputedRect>(arena.0).map(|rect| rect.0) else {
        return;
    };
    let delta_ms = world
        .resource::<crate::ecs::DeltaTimeMs>()
        .map(|delta| delta.0)
        .unwrap_or(16);
    let elapsed = Fixed::from_ratio(delta_ms as i32, 1_000);
    let bound_w = bounds.w.to_int();
    let bound_h = bounds.h.to_int();
    world.for_each_stable::<BarState>(|world, e| {
        let (new_x, new_y, changed) = {
            let Some(bar) = world.get_mut::<BarState>(e) else {
                return;
            };
            if bar.right_anchored {
                bar.x = Fixed::from_int((bound_w - BAR_W - BAR_MARGIN).max(BAR_MARGIN));
            }
            let old_display = if bar.snap { bar.y.floor() } else { bar.y };
            bar.y += bar.speed_per_second * elapsed;
            if bar.y > Fixed::from_int((bound_h - BAR_H - BAR_MARGIN).max(START_Y)) {
                bar.y = Fixed::from_int(START_Y);
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
        Column (
            grow: 1.0,
            padding: Padding::all(12),
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            bg_color: ColorToken::Surface
        ) {
            View (
                id: "subpixel_stage",
                width: Dimension::percent(100),
                max_width: 480,
                min_height: 140,
                grow: 1.0,
                max_height: 360,
                padding: Padding::all(12),
                direction: FlexDirection::Column,
                bg_color: ColorToken::SurfaceVariant,
                border_color: ColorToken::Outline,
                border_width: 1,
                border_radius: 18,
                clip_children: true
            ) {
                Row (
                    width: Dimension::percent(100),
                    height: 32,
                    align: AlignItems::Center,
                    padding: Padding {
                        top: Dimension::px(0),
                        right: Dimension::px(4),
                        bottom: Dimension::px(0),
                        left: Dimension::px(4),
                    }
                ) {
                    Text (
                        "PIXEL-SNAPPED",
                        grow: 1.0,
                        font_size: 11,
                        text_color: ColorToken::Error,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "Q24.8 SUBPIXEL",
                        grow: 1.0,
                        font_size: 11,
                        text_color: ColorToken::Secondary,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                    )
                }
                View (
                    id: "subpixel_arena",
                    width: Dimension::percent(100),
                    grow: 1.0,
                    bg_color: ColorToken::Surface,
                    border_radius: 12,
                    clip_children: true
                ) {
                    View (
                        id: "subpixel_snapped_bar",
                        bg_color: ColorToken::Error,
                        position: Position::Absolute,
                        left: BAR_MARGIN,
                        top: START_Y,
                        width: BAR_W,
                        height: BAR_H,
                        border_radius: 4
                    ) [
                        BarState {
                            y: Fixed::from_int(START_Y),
                            speed_per_second: Fixed::from_ratio(135, 64),
                            snap: true,
                            x: Fixed::from_int(BAR_MARGIN),
                            right_anchored: false,
                        },
                    ]
                    View (
                        id: "subpixel_smooth_bar",
                        bg_color: ColorToken::Secondary,
                        position: Position::Absolute,
                        left: 0,
                        top: START_Y,
                        width: BAR_W,
                        height: BAR_H,
                        border_radius: 4
                    ) [
                        BarState {
                            y: Fixed::from_int(START_Y),
                            speed_per_second: Fixed::from_ratio(135, 64),
                            snap: false,
                            x: Fixed::ZERO,
                            right_anchored: true,
                        },
                    ]
                }
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
    app.add_system(bar_move_system::system());
    app.compose(parent, build_widgets);
    let arena = app
        .world
        .find_by_id("subpixel_arena")
        .expect("subpixel arena");
    app.world.insert_resource(BarArena(arena));
}

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(480, 320);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Viewport;
    use crate::ui::render_system::update_layout;
    use crate::ui::{Children, ComputedRect, IdMap, Style, UiScope};

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

    #[test]
    fn stage_and_arena_fit_phone_and_desktop_viewports() {
        for (width, height) in [(320, 568), (480, 320), (1024, 640)] {
            let mut app = App::headless(width, height);
            app.with_default_widgets().with_default_systems();
            let root = app.spawn_root().id();
            setup_app(&mut app, root);
            app.set_root(root);
            update_layout(
                &mut app.world,
                root,
                &Viewport::new(width, height, Fixed::ONE),
            );

            for id in ["subpixel_stage", "subpixel_arena"] {
                let entity = app.world.find_by_id(id).unwrap();
                let rect = app.world.get::<ComputedRect>(entity).unwrap().0;
                assert!(rect.x >= Fixed::ZERO);
                assert!(rect.y >= Fixed::ZERO);
                assert!(rect.x + rect.w <= Fixed::from_int(width as i32));
                assert!(rect.y + rect.h <= Fixed::from_int(height as i32));
            }
        }
    }

    #[test]
    fn bars_follow_theme_and_live_arena_bounds() {
        let mut app = App::headless(320, 568);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        setup_app(&mut app, root);
        app.set_root(root);
        update_layout(&mut app.world, root, &Viewport::new(320, 568, Fixed::ONE));

        let snapped = app.world.find_by_id("subpixel_snapped_bar").unwrap();
        let smooth = app.world.find_by_id("subpixel_smooth_bar").unwrap();
        assert_eq!(
            app.world.get::<Style>(snapped).unwrap().bg_color,
            Some(ColorToken::Error.into()),
        );
        assert_eq!(
            app.world.get::<Style>(smooth).unwrap().bg_color,
            Some(ColorToken::Secondary.into()),
        );

        app.world.insert_resource(crate::ecs::DeltaTimeMs(250));
        bar_move_system(&mut app.world);
        let arena_width = app
            .world
            .get::<ComputedRect>(app.world.find_by_id("subpixel_arena").unwrap())
            .unwrap()
            .0
            .w
            .to_int();
        let smooth_state = app.world.get::<BarState>(smooth).unwrap();
        assert_eq!(
            smooth_state.x,
            Fixed::from_int(arena_width - BAR_W - BAR_MARGIN),
        );
        assert!(smooth_state.y > Fixed::from_int(START_Y));
    }
}
