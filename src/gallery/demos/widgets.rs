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
    Slider, Switch, TabBar, TabContent, Text, TextAlign,
};
use crate::ui::{Children, IdMap, OffscreenRender, Theme};
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
    let Some(label_entity) = world
        .get::<Children>(entity)
        .and_then(|children| children.0.first().copied())
    else {
        return;
    };
    if let Some(text) = world.get_mut::<Text>(label_entity) {
        text.set_content(label);
        world.invalidate_visual(label_entity);
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
    theme::set_theme(world, theme).unwrap();
});

#[compose]
pub fn build_widgets(_view_w: u16, _view_h: u16) {
    const ROW_HEIGHT: i32 = 38;
    if cx.world_mut().resource::<IdMap>().is_none() {
        cx.world_mut().insert_resource(IdMap::new());
    }

    //~focus-start
    ui! {
        Column (
            bg_color: ColorToken::Surface,
            grow: 1.0,
            padding: Padding::all(16),
            row_gap: 12
        ) {
            Column (
                width: Dimension::percent(100),
                height: 50,
                row_gap: 2
            ) {
                Text (
                    "WIDGET WORKBENCH",
                    width: Dimension::percent(100),
                    height: 28,
                    font_size: 20,
                    text_color: ColorToken::OnSurface,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "LIST / CONTROLS / LIVE THEME",
                    width: Dimension::percent(100),
                    height: 18,
                    font_size: 10,
                    text_color: ColorToken::OnSurfaceVariant,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
            }
            TabBar (
                id: "widgets_tabs",
                width: Dimension::percent(100),
                height: 44,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 14,
                clip_children: true,
                count: 3,
                indicator_height: Fixed::from_int(3)
            ) {
                Text (
                    "LIST",
                    grow: 1.0,
                    height: Dimension::percent(100),
                    text_color: ColorToken::OnSurfaceVariant,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    "CONTROLS",
                    grow: 1.0,
                    height: Dimension::percent(100),
                    text_color: ColorToken::OnSurfaceVariant,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    "THEME",
                    grow: 1.0,
                    height: Dimension::percent(100),
                    text_color: ColorToken::OnSurfaceVariant,
                    paragraph: ParagraphStyle::label()
                )
            }
            View (
                width: Dimension::percent(100),
                grow: 1.0,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 18,
                clip_children: true
            ) {
                LazyList (
                    id: "widgets_list",
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    bg_color: ColorToken::SurfaceVariant,
                    item_count: ITEM_COUNT,
                    item_height: Fixed::from_int(ROW_HEIGHT),
                    pool_size: POOL_SIZE as u8
                ) [
                    TabContent {
                        tab_bar: id("widgets_tabs"),
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
                        content_height: Fixed::from_int(ROW_HEIGHT * ITEM_COUNT as i32),
                        content_width: Fixed::ZERO,
                    },
                ] {
                    walk 0..POOL_SIZE with _i {
                        Row (
                            bg_color: ColorToken::Surface,
                            position: Position::Absolute,
                            left: 0,
                            top: 0,
                            width: Dimension::percent(100),
                            height: ROW_HEIGHT,
                            align: AlignItems::Center,
                            padding: Padding {
                                top: Dimension::px(0),
                                right: Dimension::px(14),
                                bottom: Dimension::px(0),
                                left: Dimension::px(14),
                            }
                        ) {
                            Text (
                                "",
                                grow: 1.0,
                                height: 24,
                                text_color: ColorToken::OnSurface,
                                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                            )
                        }
                    }
                }
                Column (
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    padding: Padding::all(18),
                    row_gap: 14,
                    bg_color: ColorToken::SurfaceVariant
                ) [
                    TabContent {
                        tab_bar: id("widgets_tabs"),
                        index: 1,
                    },
                ] {
                    Row (height: 34, align: AlignItems::Center) {
                        Text (
                            "LIVE CONTROLS",
                            grow: 1.0,
                            height: 24,
                            text_color: ColorToken::OnSurface,
                            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                        )
                        Switch (width: 44, height: 24) [
                            OffscreenRender::default(),
                        ]
                    }
                    Text (
                        "INTENSITY",
                        width: Dimension::percent(100),
                        height: 18,
                        font_size: 10,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Slider (
                        width: Dimension::percent(100),
                        height: 20,
                        min: Fixed::ZERO,
                        max: Fixed::from_int(100)
                    ) [
                        FormSlider,
                    ]
                    ProgressBar (
                        width: Dimension::percent(100),
                        height: 8,
                        border_radius: 4
                    ) [
                        FormProgress,
                    ]
                    Row (
                        height: 42,
                        align: AlignItems::Center,
                        column_gap: 10
                    ) {
                        Image (width: 28, height: 28, src: "thumbs_up")
                        Button (
                            grow: 1.0,
                            height: 38,
                            border_radius: 12,
                            normal_color: ColorToken::Success,
                            pressed_color: ColorToken::Primary,
                            text_color: ColorToken::OnPrimary
                        ) [
                            Text::label("Apply"),
                        ]
                        Button (
                            grow: 1.0,
                            height: 38,
                            border_radius: 12,
                            normal_color: ColorToken::Surface,
                            pressed_color: ColorToken::Primary,
                            text_color: ColorToken::OnSurface
                        ) [
                            Text::label("Reset"),
                        ]
                    }
                    Row (height: 32, align: AlignItems::Center, column_gap: 10) {
                        Text (
                            "OPTIONS",
                            grow: 1.0,
                            height: 24,
                            text_color: ColorToken::OnSurface,
                            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                        )
                        Checkbox (
                            width: 22,
                            height: 22,
                            checked: true,
                            checked_color: ColorToken::Primary
                        )
                        Checkbox (
                            width: 22,
                            height: 22,
                            checked_color: ColorToken::Success
                        )
                    }
                }
                Column (
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    padding: Padding::all(20),
                    row_gap: 12,
                    bg_color: ColorToken::SurfaceVariant
                ) [
                    TabContent {
                        tab_bar: id("widgets_tabs"),
                        index: 2,
                    },
                ] {
                    Text (
                        "SEMANTIC PALETTE",
                        width: Dimension::percent(100),
                        height: 28,
                        font_size: 18,
                        text_color: ColorToken::OnSurface,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "PRIMARY",
                        width: Dimension::percent(100),
                        height: 18,
                        font_size: 10,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    View (
                        width: Dimension::percent(100),
                        height: 64,
                        bg_color: ColorToken::Primary,
                        border_radius: 14
                    )
                    Text (
                        "CUSTOM ACCENT",
                        width: Dimension::percent(100),
                        height: 18,
                        font_size: 10,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    View (
                        width: Dimension::percent(100),
                        height: 64,
                        bg_color: ACCENT,
                        border_radius: 14
                    )
                    Text (
                        "Palette tokens update every three seconds.",
                        width: Dimension::percent(100),
                        height: 24,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                }
            }
        }
    };
    //~focus-end

    let list = cx
        .world_mut()
        .find_by_id("widgets_list")
        .expect("widgets list id");
    let pool: Vec<Entity> = cx
        .world_mut()
        .get::<Children>(list)
        .map(|children| children.0.clone())
        .unwrap_or_default();
    cx.world_mut().insert(list, LazyListPool::new(pool));
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

    #[test]
    fn list_binding_updates_content_without_dropping_paragraph_style() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx, DEFAULT_VIEW.0, DEFAULT_VIEW.1);
        drop(cx);

        let list = world.find_by_id("widgets_list").expect("widgets list id");
        let row = world
            .get::<Children>(list)
            .and_then(|children| children.0.first().copied())
            .expect("pooled row");
        let label = world
            .get::<Children>(row)
            .and_then(|children| children.0.first().copied())
            .expect("row label");
        let paragraph = world
            .get::<Text>(label)
            .expect("label text")
            .paragraph()
            .clone();

        row_binder(&mut world, row, 7);

        let text = world.get::<Text>(label).expect("bound label");
        assert_eq!(text.resolve(&world), "Row 7");
        assert_eq!(text.paragraph(), &paragraph);
    }

    #[test]
    fn portrait_layout_keeps_the_workbench_inside_the_viewport() {
        use crate::types::Viewport;
        use crate::ui::ComputedRect;
        use crate::ui::render_system::update_layout;

        let mut app = App::headless(320, 480);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        app.compose(root, |cx| build_widgets(cx, 320, 480));
        app.set_root(root);
        update_layout(&mut app.world, root, &Viewport::new(320, 480, Fixed::ONE));

        let tabs = app.world.find_by_id("widgets_tabs").expect("tabs id");
        let list = app.world.find_by_id("widgets_list").expect("list id");
        let tabs_rect = app.world.get::<ComputedRect>(tabs).expect("tabs rect").0;
        let list_rect = app.world.get::<ComputedRect>(list).expect("list rect").0;

        assert_eq!(tabs_rect.x.to_int(), 16);
        assert_eq!(tabs_rect.w.to_int(), 288);
        assert_eq!(list_rect.x.to_int(), 16);
        assert_eq!(list_rect.w.to_int(), 288);
        assert!(list_rect.y + list_rect.h <= Fixed::from_int(464));
    }
}
