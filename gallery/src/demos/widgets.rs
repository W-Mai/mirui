//! ESP32C3 v0.14 widgets showcase, ported to gallery's `Setup`-based runner
//! so it runs on every desktop / fb backend gallery already supports.
//!
//! Original 128×128 ESP layout is scaled by `SCALE` so the same scene
//! reads on a desktop / Pi fb screen without re-laying-out by hand.

use crate::Setup;
use mirui::anim::ease;
use mirui::components::{LazyList, LazyListBinder, LazyListPool};
use mirui::components::{ProgressBar, Slider, Switch, TabBar, TabContent, Text};
use mirui::ecs::Entity;
use mirui::event::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
use mirui::event::sim::{SimAction, SimTimeline, sim_timeline_system};
use mirui::plugins::{FpsSummaryPlugin, InputFeedbackPlugin, StdInstantClockPlugin};
use mirui::prelude::*;
use mirui::types::{Color, DimPoint, Dimension, Fixed};
use mirui::widget::dirty::Dirty;
use mirui::widget::theme::{self, ColorToken};
use mirui::widget::{Children, OffscreenRender, Theme};

const SCALE: i32 = 4;
const W: i32 = 128 * SCALE;
const H: i32 = 128 * SCALE;
const TABBAR_H: i32 = 14 * SCALE;
const PAGE_H: i32 = H - TABBAR_H;
const ROW_H: i32 = 12 * SCALE;
const POOL_SIZE: usize = 12;
const ITEM_COUNT: u32 = 50;

pub const SIZE: (u16, u16) = (W as u16, H as u16);

const ACCENT: ColorToken = ColorToken::custom("accent");

struct FormSlider;
struct FormProgress;
struct ThemeCycleIndex(u8);

fn dark_with_accent() -> Theme {
    Theme::dark().with(ACCENT, Color::rgb(255, 200, 60))
}

fn light_with_accent() -> Theme {
    Theme::light().with(ACCENT, Color::rgb(220, 60, 90))
}

fn custom_theme() -> Theme {
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
        t.0 = label.into_bytes();
    } else {
        world.insert(entity, Text(label.into_bytes()));
    }
}

#[mirui::system]
fn slider_to_progress_system(world: &mut World) {
    let sliders: Vec<Entity> = world.query::<FormSlider>().collect();
    let mut value = None;
    for e in sliders {
        if let Some(s) = world.get::<Slider>(e) {
            value = Some(s.value.to_f32() / 100.0);
        }
    }
    let Some(v) = value else { return };
    let bars: Vec<Entity> = world.query::<FormProgress>().collect();
    for e in bars {
        if let Some(pb) = world.get_mut::<ProgressBar>(e)
            && (pb.value - v).abs() > 0.001
        {
            pb.value = v;
            world.insert(e, Dirty);
        }
    }
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

pub fn build(setup: &mut Setup<'_>) -> Entity {
    let app = &mut setup.app;
    app.add_plugin(InputFeedbackPlugin::default());
    app.add_plugin(StdInstantClockPlugin);
    app.add_plugin(FpsSummaryPlugin::default());
    app.with_offscreen_pool_budget(512 * 1024);
    app.add_system(sim_timeline_system::system());
    app.add_system(slider_to_progress_system::system());

    app.world.insert_resource(dark_with_accent());
    let cycle_e = Cycle::install(&mut app.world);
    app.world.insert(cycle_e, ThemeCycleIndex(0));

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(ColorToken::Surface)
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(W),
            height: Dimension::px(H),
            ..Default::default()
        })
        .id();

    let tabs = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        tabs (
            bg_color: ColorToken::SurfaceVariant,
            width: W,
            height: TABBAR_H
        ) [
            TabBar::new(3).with_indicator_height(2 * SCALE as u32),
        ] {
            tab0 (
                text: "List",
                text_color: ColorToken::OnSurface,
                grow: 1.0,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) {}
            tab1 (
                text: "Form",
                text_color: ColorToken::OnSurface,
                grow: 1.0,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) {}
            tab2 (
                text: "Thm",
                text_color: ColorToken::OnSurface,
                grow: 1.0,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) {}
        }
    };

    let list = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        list (
            bg_color: ColorToken::Surface,
            position: Position::Absolute,
            left: 0,
            top: TABBAR_H,
            width: W,
            height: PAGE_H
        ) [
            TabContent {
                tab_bar: tabs,
                index: 0,
            },
            LazyList::new(ITEM_COUNT, ROW_H, POOL_SIZE as u8),
            LazyListBinder { bind: row_binder },
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
            ScrollConfig {
                direction: ScrollAxis::Vertical,
                elastic: false,
                content_height: Fixed::from_int(ROW_H * ITEM_COUNT as i32),
                content_width: Fixed::ZERO,
            },
        ] {
            walk 0..POOL_SIZE with _i {
                row (
                    bg_color: ColorToken::SurfaceVariant,
                    text_color: ColorToken::OnSurface,
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: W,
                    height: ROW_H
                ) {}
            }
        }
    };
    let pool: Vec<Entity> = app
        .world
        .get::<Children>(list)
        .map(|c| c.0.clone())
        .unwrap_or_default();
    app.world.insert(list, LazyListPool::new(pool));

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        form_page (
            bg_color: ColorToken::Surface,
            position: Position::Absolute,
            left: 0,
            top: TABBAR_H,
            width: W,
            height: PAGE_H,
            direction: FlexDirection::Column,
            padding: Padding::all(10 * SCALE)
        ) [
            TabContent {
                tab_bar: tabs,
                index: 1,
            },
        ] {
            enable_row (
                direction: FlexDirection::Row,
                height: 28 * SCALE,
                align: AlignItems::Center
            ) {
                enable_label (text: "Enable", text_color: ColorToken::OnSurface, grow: 1.0) {}
                enable_switch (width: 40 * SCALE, height: 20 * SCALE) [
                    Switch::new(),
                    OffscreenRender::default(),
                ] {}
            }
            slider_row (
                height: 14 * SCALE,
                padding: Padding {
                    top: Dimension::px(6 * SCALE),
                    ..Default::default()
                }
            ) {
                value_slider (width: 108 * SCALE, height: 14 * SCALE) [
                    Slider::new(Fixed::ZERO, Fixed::from_int(100)),
                    FormSlider,
                ] {}
            }
            progress_row (
                height: 10 * SCALE,
                padding: Padding {
                    top: Dimension::px(8 * SCALE),
                    ..Default::default()
                }
            ) {
                value_progress (width: 108 * SCALE, height: 8 * SCALE, border_radius: 4 * SCALE as u32) [
                    ProgressBar::new(),
                    FormProgress,
                ] {}
            }
        }
    };

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        theme_page (
            bg_color: ColorToken::Surface,
            position: Position::Absolute,
            left: 0,
            top: TABBAR_H,
            width: W,
            height: PAGE_H,
            direction: FlexDirection::Column,
            padding: Padding::all(12 * SCALE),
            align: AlignItems::Center
        ) [
            TabContent {
                tab_bar: tabs,
                index: 2,
            },
        ] {
            primary_label (text: "Primary", text_color: ColorToken::OnSurface, height: 14 * SCALE) {}
            primary_block (width: 80 * SCALE, height: 18 * SCALE, bg_color: ColorToken::Primary, border_radius: 4 * SCALE as u32) {}
            accent_label (
                text: "accent (custom)",
                text_color: ColorToken::OnSurfaceVariant,
                height: 12 * SCALE,
                padding: Padding {
                    top: Dimension::px(8 * SCALE),
                    ..Default::default()
                }
            ) {}
            accent_block (width: 80 * SCALE, height: 18 * SCALE, bg_color: ACCENT, border_radius: 4 * SCALE as u32) {}
        }
    };

    let tab_kids = app
        .world
        .get::<Children>(tabs)
        .map(|c| c.0.clone())
        .unwrap_or_default();
    let (tab_list, tab_form, tab_theme) = (tab_kids[0], tab_kids[1], tab_kids[2]);
    let switch_e = *app
        .world
        .query::<Switch>()
        .collect()
        .first()
        .expect("form Switch must be installed");
    let slider_e = *app
        .world
        .query::<Slider>()
        .collect()
        .first()
        .expect("form Slider must be installed");

    let list_drag_anchor = list;

    app.world.insert_resource(
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
            .on(list_drag_anchor),
            SimAction::wait(800),
            SimAction::drag(
                DimPoint::percent(50, 20),
                DimPoint::percent(50, 80),
                100,
                ease::linear,
            )
            .on(list_drag_anchor),
            SimAction::wait(800),
        ])
        .looping(true),
    );

    root
}
