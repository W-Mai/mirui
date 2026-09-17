extern crate alloc;

use crate::anim::ease;
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::input::event::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
use crate::input::event::sim::{SimAction, SimTimeline};
#[cfg(feature = "std")]
use crate::prelude::plugin::{FpsSummaryPlugin, InputFeedbackPlugin};
use crate::prelude::*;
use crate::types::DimPoint;
use crate::ui::theme;
use crate::ui::widgets::{
    Button, Checkbox, Image, LazyList, LazyListBinder, LazyListPool, ParagraphStyle, ProgressBar,
    Slider, Switch, TabBar, TabContent, Text,
};
use crate::ui::{Children, OffscreenRender, Theme};
use alloc::format;
use alloc::vec;
use alloc::vec::Vec;

const POOL_SIZE: usize = 12;
const ITEM_COUNT: u32 = 50;

pub const DEFAULT_VIEW: (u16, u16) = (512, 512);

pub const ACCENT: ColorToken = ColorToken::custom("accent");

struct FormSlider;
struct FormProgress;
pub struct ThemeCycleIndex(pub u8);

struct DemoSize {
    tabbar_h: i32,
    row_h: i32,
    scale: i32,
}

impl DemoSize {
    fn for_viewport(view_w: u16, view_h: u16) -> Self {
        let w = (view_w as i32).max(1);
        let h = (view_h as i32).max(1);
        let scale = (w.min(h) / 128).max(1);
        Self {
            tabbar_h: 14 * scale,
            row_h: 12 * scale,
            scale,
        }
    }
}

pub fn dark_with_accent() -> Theme {
    Theme::dark().with(ACCENT, Color::rgb(255, 200, 60))
}

pub fn light_with_accent() -> Theme {
    Theme::light().with(ACCENT, Color::rgb(220, 60, 90))
}

pub fn custom_theme() -> Theme {
    Theme::dark().with_many([
        (ColorToken::Primary, Color::rgb(255, 105, 180)),
        (ColorToken::OnPrimary, Color::rgb(20, 20, 30)),
        (ColorToken::Success, Color::rgb(255, 200, 60)),
        (ColorToken::Surface, Color::rgb(38, 28, 50)),
        (ColorToken::SurfaceVariant, Color::rgb(70, 50, 90)),
        (ColorToken::OnSurface, Color::rgb(245, 235, 255)),
        (ColorToken::OnSurfaceVariant, Color::rgb(180, 150, 200)),
        (ACCENT, Color::rgb(140, 200, 220)),
    ])
}

fn row_binder(world: &mut World, entity: Entity, index: u32) {
    let label = format!("Row {index}");
    if let Some(t) = world.get_mut::<Text>(entity) {
        *t = Text::from(label);
    } else {
        world.insert(entity, Text::from(label));
    }
}

#[mirui_macros::system]
pub fn slider_to_progress_system(world: &mut World) {
    let value = world
        .query::<FormSlider>()
        .iter()
        .find_map(|(entity, _)| world.get::<Slider>(entity))
        .map(|slider| slider.value.to_f32() / 100.0);
    let Some(v) = value else { return };
    world.for_each_stable::<FormProgress>(|world, entity| {
        if let Some(pb) = world.get_mut::<ProgressBar>(entity)
            && (pb.value - v).abs() > 0.001
        {
            pb.value = v;
            world.invalidate(entity);
        }
    });
}

mirui_macros::timer!(Cycle, every: 3_000, |world, entity| {
    let next = world
        .get::<ThemeCycleIndex>(entity)
        .map(|i| (i.0 + 1) % 3)
        .unwrap_or(0);
    world.insert(entity, ThemeCycleIndex(next));
    let theme = match next {
        0 => dark_with_accent(),
        1 => light_with_accent(),
        _ => custom_theme(),
    };
    theme::set_theme(world, theme);
});

#[compose]
pub fn build_widgets(view_w: u16, view_h: u16) {
    let DemoSize {
        tabbar_h: tabbar_h_,
        row_h: row_h_,
        scale: scale_,
    } = DemoSize::for_viewport(view_w, view_h);

    //~focus-start
    let tabs = ui! {
        TabBar (
            bg_color: ColorToken::SurfaceVariant,
            height: tabbar_h_,
            count: 3,
            indicator_height: Fixed::from_int(2 * scale_)
        ) {
            Text (
                "List",
                text_color: ColorToken::OnSurface,
                grow: 1.0,
                paragraph: ParagraphStyle::label()
            )
            Text (
                "Form",
                text_color: ColorToken::OnSurface,
                grow: 1.0,
                paragraph: ParagraphStyle::label()
            )
            Text (
                "Thm",
                text_color: ColorToken::OnSurface,
                grow: 1.0,
                paragraph: ParagraphStyle::label()
            )
        }
    };
    //~focus-end

    //~focus-start
    let list = ui! {
        LazyList (
            bg_color: ColorToken::Surface,
            grow: 1.0,
            item_count: ITEM_COUNT,
            item_height: Fixed::from_int(row_h_),
            pool_size: POOL_SIZE as u8
        ) [
            TabContent {
                tab_bar: tabs,
                index: 0,
            },
            LazyListBinder { bind: row_binder },
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
            ScrollConfig {
                direction: ScrollAxis::Vertical,
                elastic: false,
                content_height: Fixed::from_int(row_h_ * ITEM_COUNT as i32),
                content_width: Fixed::ZERO,
            },
        ] {
            walk 0..POOL_SIZE with _i {
                Row (
                    bg_color: ColorToken::SurfaceVariant,
                    text_color: ColorToken::OnSurface,
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: Dimension::percent(100),
                    height: row_h_
                )
            }
        }
    };
    //~focus-end
    let pool: Vec<Entity> = cx
        .world_mut()
        .get::<Children>(list)
        .map(|c| c.0.clone())
        .unwrap_or_default();
    cx.world_mut().insert(list, LazyListPool::new(pool));

    //~focus-start
    ui! {
        Column (
            bg_color: ColorToken::Surface,
            grow: 1.0,
            padding: Padding::all(10 * scale_)
        ) [
            TabContent {
                tab_bar: tabs,
                index: 1,
            },
        ] {
            Row (
                height: 28 * scale_,
                align: AlignItems::Center
            ) {
                Text ("Enable", text_color: ColorToken::OnSurface, grow: 1.0)
                Switch (width: 40 * scale_, height: 20 * scale_) [
                    OffscreenRender::default(),
                ]
            }
            View (
                height: 14 * scale_,
                padding: Padding {
                    top: Dimension::px(6 * scale_),
                    ..Default::default()
                }
            ) {
                Slider (
                    width: 108 * scale_,
                    height: 14 * scale_,
                    min: Fixed::ZERO,
                    max: Fixed::from_int(100)
                ) [
                    FormSlider,
                ]
            }
            View (
                height: 10 * scale_,
                padding: Padding {
                    top: Dimension::px(8 * scale_),
                    ..Default::default()
                }
            ) {
                ProgressBar (
                    width: 108 * scale_,
                    height: 8 * scale_,
                    border_radius: 4 * scale_ as u32
                ) [
                    FormProgress,
                ]
            }
            Row (
                height: 20 * scale_,
                align: AlignItems::Center,
                column_gap: 4 * scale_
            ) {
                Image (
                    width: 16 * scale_,
                    height: 16 * scale_,
                    src: "thumbs_up"
                )
                Button (
                    grow: 1.0,
                    height: 18 * scale_,
                    border_radius: 4 * scale_ as u32,
                    normal_color: ColorToken::Success,
                    pressed_color: ColorToken::Primary,
                    text_color: ColorToken::OnPrimary
                ) [
                    Text::label("Apply"),
                ]
                Button (
                    grow: 1.0,
                    height: 18 * scale_,
                    border_radius: 4 * scale_ as u32,
                    normal_color: ColorToken::SurfaceVariant,
                    pressed_color: ColorToken::Primary,
                    text_color: ColorToken::OnSurface
                ) [
                    Text::label("Reset"),
                ]
            }
            Row (
                height: 16 * scale_,
                align: AlignItems::Center,
                column_gap: 5 * scale_
            ) {
                Text ("Options", text_color: ColorToken::OnSurface, grow: 1.0)
                Checkbox (
                    width: 14 * scale_,
                    height: 14 * scale_,
                    checked: true,
                    checked_color: ColorToken::Primary
                )
                Checkbox (
                    width: 14 * scale_,
                    height: 14 * scale_,
                    checked_color: ColorToken::Success
                )
            }
        }
    };
    //~focus-end

    //~focus-start
    ui! {
        Column (
            bg_color: ColorToken::Surface,
            grow: 1.0,
            padding: Padding::all(12 * scale_),
            align: AlignItems::Center
        ) [
            TabContent {
                tab_bar: tabs,
                index: 2,
            },
        ] {
            Text ("Primary", text_color: ColorToken::OnSurface, height: 14 * scale_)
            View (
                width: 80 * scale_,
                height: 18 * scale_,
                bg_color: ColorToken::Primary,
                border_radius: 4 * scale_ as u32
            )
            Text (
                "accent (custom)",
                text_color: ColorToken::OnSurfaceVariant,
                height: 12 * scale_,
                padding: Padding {
                    top: Dimension::px(8 * scale_),
                    ..Default::default()
                }
            )
            View (
                width: 80 * scale_,
                height: 18 * scale_,
                bg_color: ACCENT,
                border_radius: 4 * scale_ as u32
            )
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
    let info = app.backend.display_info();
    app.add_plugin(InputFeedbackPlugin::default());
    app.add_plugin(StdInstantClockPlugin);
    app.add_plugin(FpsSummaryPlugin::default());
    app.add_plugin(crate::app::plugins::ImageResourcesPlugin::default());
    app.with_offscreen_pool_budget(512 * 1024);
    app.add_system(crate::input::event::sim::sim_timeline_system::system());
    app.add_system(slider_to_progress_system::system());

    app.world.insert_resource(dark_with_accent());
    let cycle_e = Cycle::install(&mut app.world);
    app.world.insert(cycle_e, ThemeCycleIndex(0));

    app.compose(parent, |cx| build_widgets(cx, info.width, info.height));

    if std::env::var("MIRUI_SIM_OFF").ok().as_deref() == Some("1") {
        return;
    }

    if let Some(timeline) = build_sim_timeline(&app.world) {
        app.world.insert_resource(timeline);
    }
}

/// Construct the demo's looping `SimTimeline` from the live widget tree.
/// Returns `None` when the expected widgets aren't installed (e.g.
/// `build_widgets` skipped) — callers can `insert_resource` the result
/// unconditionally and the sim system stays a no-op without a timeline.
pub fn build_sim_timeline(world: &World) -> Option<SimTimeline> {
    let tab_bars: Vec<Entity> = world.query::<TabBar>().collect();
    let tab_bar_e = *tab_bars.first()?;
    let tabs_kids: Vec<Entity> = world
        .get::<Children>(tab_bar_e)
        .map(|c| c.0.clone())
        .unwrap_or_default();
    if tabs_kids.len() < 3 {
        return None;
    }
    let (tab_list, tab_form, tab_theme) = (tabs_kids[0], tabs_kids[1], tabs_kids[2]);

    let switches: Vec<Entity> = world.query::<Switch>().collect();
    let switch_e = *switches.first()?;
    let sliders: Vec<Entity> = world.query::<Slider>().collect();
    let slider_e = *sliders.first()?;
    let lists: Vec<Entity> = world.query::<LazyList>().collect();
    let list_e = *lists.first()?;

    Some(
        SimTimeline::new(vec![
            SimAction::wait(800),
            SimAction::tap(DimPoint::CENTER).on(tab_form),
            SimAction::wait(800),
            SimAction::tap(DimPoint::CENTER).on(switch_e),
            SimAction::wait(800),
            SimAction::drag(
                DimPoint::percent(10, 50),
                DimPoint::percent(90, 50),
                600,
                ease::ease_in_out_cubic,
            )
            .on(slider_e),
            SimAction::wait(800),
            SimAction::tap(DimPoint::CENTER).on(switch_e),
            SimAction::wait(1500),
            SimAction::tap(DimPoint::CENTER).on(tab_theme),
            SimAction::wait(6500),
            SimAction::tap(DimPoint::CENTER).on(tab_list),
            SimAction::wait(800),
            SimAction::drag(
                DimPoint::percent(50, 80),
                DimPoint::percent(50, 20),
                100,
                ease::linear,
            )
            .on(list_e),
            SimAction::wait(800),
            SimAction::drag(
                DimPoint::percent(50, 20),
                DimPoint::percent(50, 80),
                100,
                ease::linear,
            )
            .on(list_e),
            SimAction::wait(800),
        ])
        .looping(true),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::IdMap;
    use crate::ui::UiScope;
    use crate::ui::dirty::Dirty;

    #[test]
    fn slider_progress_sync_reuses_stable_traversal() {
        let mut world = World::new();
        let slider = world.spawn_empty();
        world.insert(slider, FormSlider);
        let mut control = Slider::new(Fixed::ZERO, Fixed::from_int(100));
        control.value = Fixed::from_int(37);
        world.insert(slider, control);

        let progress = world.spawn_empty();
        world.insert(progress, FormProgress);
        world.insert(progress, ProgressBar::default());

        slider_to_progress_system(&mut world);

        assert!((world.get::<ProgressBar>(progress).unwrap().value - 0.37).abs() < 0.001);
        assert!(world.get::<Dirty>(progress).is_some());
    }

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx, DEFAULT_VIEW.0, DEFAULT_VIEW.1);
        drop(cx);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
        assert_eq!(world.query::<Button>().collect().len(), 2);
        assert_eq!(world.query::<Checkbox>().collect().len(), 2);
        assert_eq!(world.query::<Image>().collect().len(), 1);
        assert!(!world.query::<ProgressBar>().collect().is_empty());
    }
}
