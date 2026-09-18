use crate::anim::ease;
use crate::input::event::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
use crate::input::event::sim::{SimAction, SimTimeline};
use crate::prelude::*;
use crate::render::font::FontToken;
use crate::types::DimPoint;
use crate::ui::theme;
use crate::ui::widgets::{
    Button, ButtonSize, LazyList, LazyListBinder, LazyListPool, ParagraphStyle, ProgressBar,
    Slider, Switch, TabBar, TabContent, Text, TextAlign, TextVerticalAlign, TextWrap,
};
use crate::ui::{Children, IdMap, Theme};
use alloc::vec;

pub const VIEWPORT: (u16, u16) = (128, 128);

struct CompactSlider;
struct CompactProgress;
struct CompactTheme(Theme);

const ROW_HEIGHT: i32 = 12;
const POOL_SIZE: usize = 9;
const VIRTUAL_ITEM_COUNT: u32 = 600_000;
const VIRTUAL_CONTENT_HEIGHT: i32 = ROW_HEIGHT * VIRTUAL_ITEM_COUNT as i32;
const ROW_LABELS: [&str; 8] = [
    "ALPHA", "BETA", "GAMMA", "DELTA", "EPS", "ZETA", "ETA", "THETA",
];

fn bitmap_label(align: TextAlign) -> ParagraphStyle {
    ParagraphStyle {
        wrap: TextWrap::NoWrap,
        align,
        vertical_align: TextVerticalAlign::Center,
        max_lines: Some(1),
        ..ParagraphStyle::default()
    }
}

#[mirui_macros::system]
fn sync_progress(world: &mut World) {
    let value = world
        .query::<CompactSlider>()
        .iter()
        .find_map(|(entity, _)| world.get::<Slider>(entity))
        .map(Slider::ratio);
    let Some(value) = value else { return };
    world.for_each_stable::<CompactProgress>(|world, entity| {
        let value = value.to_f32();
        if let Some(progress) = world.get_mut::<ProgressBar>(entity)
            && (progress.value - value).abs() > 0.001
        {
            progress.value = value;
            world.invalidate(entity);
        }
    });
}

fn bind_row(world: &mut World, entity: Entity, index: u32) {
    let Some(label) = world
        .get::<Children>(entity)
        .and_then(|children| children.0.first().copied())
    else {
        return;
    };
    if let Some(text) = world.get_mut::<Text>(label) {
        text.set_content(ROW_LABELS[index as usize % ROW_LABELS.len()]);
        world.invalidate_visual(label);
    }
}

#[compose]
pub fn build_widgets() {
    if cx.world_mut().resource::<IdMap>().is_none() {
        cx.world_mut().insert_resource(IdMap::new());
    }

    ui! {
        View (
            id: "compact_widgets_shell",
            grow: 1.0,
            direction: @(id(compact_widgets_shell).width, id(compact_widgets_shell).height) {
                super::compact_layout::shell_direction(compact_widgets_shell.width, compact_widgets_shell.height)
            },
            padding: @(id(compact_widgets_shell).width, id(compact_widgets_shell).height) {
                super::compact_layout::padding(compact_widgets_shell.width, compact_widgets_shell.height, 4, 6)
            },
            row_gap: 4,
            column_gap: 4,
            bg_color: ColorToken::Surface
        ) {
            View (
                id: "compact_widgets_header",
                width: @(id(compact_widgets_shell).width, id(compact_widgets_shell).height) {
                    super::compact_layout::header_width(compact_widgets_shell.width, compact_widgets_shell.height, 30)
                },
                height: @(id(compact_widgets_shell).width, id(compact_widgets_shell).height) {
                    super::compact_layout::header_height(compact_widgets_shell.width, compact_widgets_shell.height)
                },
                direction: @(id(compact_widgets_shell).width, id(compact_widgets_shell).height) {
                    super::compact_layout::header_direction(compact_widgets_shell.width, compact_widgets_shell.height)
                },
                align: AlignItems::Center,
                justify: JustifyContent::Center,
                row_gap: 3,
                column_gap: 4
            ) {
                View (
                    id: "compact_widgets_marker",
                    width: @(id(compact_widgets_shell).width, id(compact_widgets_shell).height) {
                        super::compact_layout::marker_width(compact_widgets_shell.width, compact_widgets_shell.height)
                    },
                    height: @(id(compact_widgets_shell).width, id(compact_widgets_shell).height) {
                        super::compact_layout::marker_height(compact_widgets_shell.width, compact_widgets_shell.height)
                    },
                    bg_color: ColorToken::Primary,
                    border_radius: 2
                )
                Text (
                    id: "compact_widgets_title",
                    "WIDGETS",
                    grow: 1.0,
                    width: @(id(compact_widgets_shell).width, id(compact_widgets_shell).height) {
                        super::compact_layout::title_width(compact_widgets_shell.width, compact_widgets_shell.height)
                    },
                    height: @(id(compact_widgets_shell).width, id(compact_widgets_shell).height) {
                        super::compact_layout::title_height(compact_widgets_shell.width, compact_widgets_shell.height)
                    },
                    font: FontToken::Default,
                    font_size: 6,
                    text_color: ColorToken::OnSurface,
                    paragraph: @(id(compact_widgets_shell).width, id(compact_widgets_shell).height) {
                        bitmap_label(super::compact_layout::text_align(compact_widgets_shell.width, compact_widgets_shell.height))
                    }
                )
                Text (
                    id: "compact_widgets_mode",
                    "AUTO",
                    width: @(id(compact_widgets_shell).width, id(compact_widgets_shell).height) {
                        super::compact_layout::mode_width(compact_widgets_shell.width, compact_widgets_shell.height, 30)
                    },
                    height: @(id(compact_widgets_shell).width, id(compact_widgets_shell).height) {
                        super::compact_layout::mode_height(compact_widgets_shell.width, compact_widgets_shell.height)
                    },
                    font: FontToken::Default,
                    font_size: 6,
                    text_color: ColorToken::OnSurfaceVariant,
                    paragraph: @(id(compact_widgets_shell).width, id(compact_widgets_shell).height) {
                        bitmap_label(super::compact_layout::mode_align(compact_widgets_shell.width, compact_widgets_shell.height))
                    }
                )
            }
            Column (
                id: "compact_widgets_body",
                grow: 1.0,
                min_width: 0,
                min_height: 0,
                width: @(id(compact_widgets_shell).width, id(compact_widgets_shell).height) {
                    super::compact_layout::content_width(compact_widgets_shell.width, compact_widgets_shell.height)
                },
                height: @(id(compact_widgets_shell).width, id(compact_widgets_shell).height) {
                    super::compact_layout::content_height(compact_widgets_shell.width, compact_widgets_shell.height)
                },
                row_gap: 4
            ) {
                TabBar (
                    id: "compact_widgets_tabs",
                    width: Dimension::percent(100),
                    height: 20,
                    count: 3,
                    indicator_height: Fixed::from_int(2),
                    bg_color: ColorToken::SurfaceVariant,
                    border_radius: 8,
                    clip_children: true
                ) {
                    Text (
                        id: "compact_tab_list",
                        "LIST",
                        grow: 1.0,
                        height: 20,
                        font: FontToken::Default,
                        font_size: 6,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: bitmap_label(TextAlign::Center)
                    )
                    Text (
                        id: "compact_tab_controls",
                        "CTRL",
                        grow: 1.0,
                        height: 20,
                        font: FontToken::Default,
                        font_size: 6,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: bitmap_label(TextAlign::Center)
                    )
                    Text (
                        id: "compact_tab_color",
                        "THEME",
                        grow: 1.0,
                        height: 20,
                        font: FontToken::Default,
                        font_size: 6,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: bitmap_label(TextAlign::Center)
                    )
                }
                View (
                    grow: 1.0,
                    width: Dimension::percent(100),
                    bg_color: ColorToken::SurfaceVariant,
                    border_radius: 10,
                    clip_children: true
                ) {
                    LazyList (
                        id: "compact_widgets_list",
                        position: Position::Absolute,
                        left: 0,
                        top: 0,
                        width: Dimension::percent(100),
                        height: Dimension::percent(100),
                        bg_color: ColorToken::SurfaceVariant,
                        item_count: VIRTUAL_ITEM_COUNT,
                        item_height: Fixed::from_int(ROW_HEIGHT),
                        pool_size: POOL_SIZE as u8
                    ) [
                        TabContent {
                            tab_bar: id("compact_widgets_tabs"),
                            index: 0,
                        },
                        LazyListBinder { bind: bind_row },
                        ScrollOffset {
                            x: Fixed::ZERO,
                            y: Fixed::ZERO,
                        },
                        ScrollConfig {
                            direction: ScrollAxis::Vertical,
                            elastic: false,
                            content_height: Fixed::from_int(VIRTUAL_CONTENT_HEIGHT),
                            content_width: Fixed::ZERO,
                        },
                    ] {
                        walk 0..POOL_SIZE with _index {
                            Row (
                                position: Position::Absolute,
                                left: 0,
                                top: 0,
                                width: Dimension::percent(100),
                                height: ROW_HEIGHT,
                                padding: Padding {
                                    top: Dimension::px(0),
                                    right: Dimension::px(7),
                                    bottom: Dimension::px(0),
                                    left: Dimension::px(7),
                                },
                                align: AlignItems::Center,
                                bg_color: ColorToken::SurfaceVariant
                            ) {
                                Text (
                                    "",
                                    grow: 1.0,
                                    height: ROW_HEIGHT,
                                    font: FontToken::Default,
                                    font_size: 6,
                                    text_color: ColorToken::OnSurface,
                                    paragraph: bitmap_label(TextAlign::Start)
                                )
                                View (
                                    width: 4,
                                    height: 4,
                                    bg_color: ColorToken::Primary,
                                    border_radius: 2
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
                        padding: Padding::all(6),
                        row_gap: 4,
                        bg_color: ColorToken::SurfaceVariant
                    ) [
                        TabContent {
                            tab_bar: id("compact_widgets_tabs"),
                            index: 1,
                        },
                    ] {
                        Row (height: 14, align: AlignItems::Center) {
                            Text (
                                "LIVE",
                                grow: 1.0,
                                height: 14,
                                font: FontToken::Default,
                                font_size: 6,
                                text_color: ColorToken::OnSurface,
                                paragraph: bitmap_label(TextAlign::Start)
                            )
                            Switch (id: "compact_widgets_switch", width: 28, height: 14, on: true)
                        }
                        Slider (
                            id: "compact_widgets_slider",
                            width: Dimension::percent(100),
                            height: 10,
                            min: Fixed::ZERO,
                            max: Fixed::from_int(100)
                        ) [
                            CompactSlider,
                        ]
                        ProgressBar (
                            id: "compact_widgets_progress",
                            width: Dimension::percent(100),
                            height: 5,
                            border_radius: 2
                        ) [
                            CompactProgress,
                        ]
                        Row (grow: 1.0, align: AlignItems::Center, column_gap: 5) {
                            Button (
                                size: ButtonSize::Custom,
                                grow: 1.0,
                                height: 18,
                                border_radius: 7,
                                normal_color: ColorToken::Primary,
                                pressed_color: ColorToken::Success,
                                text_color: ColorToken::OnPrimary
                            ) {
                                Text (
                                    "RUN",
                                    grow: 1.0,
                                    height: 18,
                                    font: FontToken::Default,
                                    font_size: 6,
                                    text_color: ColorToken::OnPrimary,
                                    paragraph: bitmap_label(TextAlign::Center)
                                )
                            }
                            Button (
                                size: ButtonSize::Custom,
                                grow: 1.0,
                                height: 18,
                                border_radius: 7,
                                normal_color: ColorToken::Surface,
                                pressed_color: ColorToken::Primary,
                                text_color: ColorToken::OnSurface
                            ) {
                                Text (
                                    "RESET",
                                    grow: 1.0,
                                    height: 18,
                                    font: FontToken::Default,
                                    font_size: 6,
                                    text_color: ColorToken::OnSurface,
                                    paragraph: bitmap_label(TextAlign::Center)
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
                        padding: Padding::all(6),
                        row_gap: 4,
                        bg_color: ColorToken::SurfaceVariant
                    ) [
                        TabContent {
                            tab_bar: id("compact_widgets_tabs"),
                            index: 2,
                        },
                    ] {
                        Text (
                            "SEMANTIC TOKENS",
                            width: Dimension::percent(100),
                            height: 10,
                            font: FontToken::Default,
                            font_size: 6,
                            text_color: ColorToken::OnSurface,
                            paragraph: bitmap_label(TextAlign::Start)
                        )
                        Row (grow: 1.0, column_gap: 4) {
                            View (grow: 1.0, bg_color: ColorToken::Primary, border_radius: 5)
                            View (grow: 1.0, bg_color: ColorToken::Success, border_radius: 5)
                            View (grow: 1.0, bg_color: ColorToken::Error, border_radius: 5)
                        }
                        Row (width: Dimension::percent(100), height: 18, column_gap: 4) {
                            Button (
                                id: "compact_theme_light",
                                size: ButtonSize::Custom,
                                grow: 1.0,
                                height: 18,
                                border_radius: 6,
                                normal_color: ColorToken::Primary,
                                pressed_color: ColorToken::Success,
                                text_color: ColorToken::OnPrimary
                            ) [
                                CompactTheme(Theme::light()),
                            ] on Tap {
                                if let Some(theme) = ctx
                                    .world
                                    .get::<CompactTheme>(ctx.entity)
                                    .map(|choice| choice.0.clone())
                                {
                                    theme::set_theme(ctx.world, theme).unwrap();
                                }
                            }
                            {
                                Text (
                                    "LIGHT",
                                    grow: 1.0,
                                    height: 18,
                                    font: FontToken::Default,
                                    font_size: 6,
                                    text_color: ColorToken::OnPrimary,
                                    paragraph: bitmap_label(TextAlign::Center)
                                )
                            }
                            Button (
                                id: "compact_theme_dark",
                                size: ButtonSize::Custom,
                                grow: 1.0,
                                height: 18,
                                border_radius: 6,
                                normal_color: ColorToken::Surface,
                                pressed_color: ColorToken::Primary,
                                text_color: ColorToken::OnSurface
                            ) [
                                CompactTheme(Theme::dark()),
                            ] on Tap {
                                if let Some(theme) = ctx
                                    .world
                                    .get::<CompactTheme>(ctx.entity)
                                    .map(|choice| choice.0.clone())
                                {
                                    theme::set_theme(ctx.world, theme).unwrap();
                                }
                            }
                            {
                                Text (
                                    "DARK",
                                    grow: 1.0,
                                    height: 18,
                                    font: FontToken::Default,
                                    font_size: 6,
                                    text_color: ColorToken::OnSurface,
                                    paragraph: bitmap_label(TextAlign::Center)
                                )
                            }
                        }
                    }
                }
            }
        }
    };

    let list = cx
        .world_mut()
        .find_by_id("compact_widgets_list")
        .expect("compact list");
    let pool = cx
        .world_mut()
        .get::<Children>(list)
        .map(|children| children.0.clone())
        .unwrap_or_default();
    cx.world_mut().insert(list, LazyListPool::new(pool));
}

pub fn install<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_system(sync_progress::system());
    app.compose(parent, build_widgets);

    let slider = app
        .world
        .find_by_id("compact_widgets_slider")
        .expect("compact slider");
    app.world
        .get_mut::<Slider>(slider)
        .expect("slider component")
        .set_ratio(Fixed::from_ratio(62, 100));
    sync_progress(&mut app.world);
}

fn automation(world: &World) -> Option<SimTimeline> {
    let tabs = world.find_by_id("compact_widgets_tabs")?;
    let list = world.find_by_id("compact_widgets_list")?;
    let slider = world.find_by_id("compact_widgets_slider")?;
    let switch = world.find_by_id("compact_widgets_switch")?;
    let light = world.find_by_id("compact_theme_light")?;
    let dark = world.find_by_id("compact_theme_dark")?;

    Some(
        SimTimeline::new(vec![
            SimAction::wait(500),
            SimAction::drag(
                DimPoint::percent(50, 80),
                DimPoint::percent(50, 20),
                700,
                ease::ease_in_out_cubic,
            )
            .on(list),
            SimAction::wait(500),
            SimAction::tap(DimPoint::percent(50, 50)).on(tabs),
            SimAction::wait(400),
            SimAction::drag(
                DimPoint::percent(15, 50),
                DimPoint::percent(85, 50),
                700,
                ease::ease_in_out_cubic,
            )
            .on(slider),
            SimAction::wait(400),
            SimAction::tap(DimPoint::CENTER).on(switch),
            SimAction::wait(700),
            SimAction::tap(DimPoint::percent(83, 50)).on(tabs),
            SimAction::wait(500),
            SimAction::tap(DimPoint::CENTER).on(light),
            SimAction::wait(1_800),
            SimAction::tap(DimPoint::CENTER).on(dark),
            SimAction::wait(1_800),
            SimAction::tap(DimPoint::percent(16, 50)).on(tabs),
            SimAction::wait(400),
        ])
        .looping(true),
    )
}

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    install(app, parent);
    app.add_system(crate::input::event::sim::sim_timeline_system::system());
    #[cfg(feature = "std")]
    if std::env::var("MIRUI_SIM_OFF").ok().as_deref() == Some("1") {
        return;
    }
    if let Some(timeline) = automation(&app.world) {
        app.world.insert_resource(timeline);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactive::flush_signal_dirty;
    use crate::input::event::GestureHandler;
    use crate::input::event::gesture::GestureEvent;
    use crate::surface::FramebufferAccess;
    use crate::types::Viewport;
    use crate::ui::ComputedRect;

    #[test]
    fn compact_widgets_fit_and_render_at_native_size() {
        let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        install(&mut app, root);
        app.set_root(root);
        flush_signal_dirty(&mut app.world);
        app.render().unwrap();

        let shell = app
            .world
            .find_by_id("compact_widgets_shell")
            .expect("compact shell");
        let shell_rect = app.world.get::<ComputedRect>(shell).expect("shell rect").0;
        let progress = app
            .world
            .find_by_id("compact_widgets_progress")
            .expect("compact progress");

        assert_eq!(shell_rect.w.to_int(), 128);
        assert_eq!(shell_rect.h.to_int(), 128);
        assert_eq!(
            app.world.get::<ProgressBar>(progress).unwrap().value,
            Fixed::from_ratio(62, 100).to_f32(),
        );
        assert!(
            app.backend
                .framebuffer()
                .buf
                .as_slice()
                .iter()
                .any(|byte| *byte != 0),
        );
    }

    fn compact_layout(width: u16, height: u16) -> (crate::types::Rect, crate::types::Rect) {
        let mut app = App::headless(width, height);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        install(&mut app, root);
        app.set_root(root);
        app.render().unwrap();

        let header = app.world.find_by_id("compact_widgets_header").unwrap();
        let body = app.world.find_by_id("compact_widgets_body").unwrap();
        (
            app.world.get::<ComputedRect>(header).unwrap().0,
            app.world.get::<ComputedRect>(body).unwrap().0,
        )
    }

    #[test]
    fn compact_widgets_reflow_between_portrait_and_landscape() {
        let (portrait_header, portrait_body) = compact_layout(96, 160);
        let (square_header, square_body) = compact_layout(128, 128);
        let (landscape_header, landscape_body) = compact_layout(160, 96);

        assert!(portrait_body.y >= portrait_header.y + portrait_header.h);
        assert!(square_body.y >= square_header.y + square_header.h);
        assert!(landscape_body.x >= landscape_header.x + landscape_header.w);
        assert!(landscape_body.y <= landscape_header.y + Fixed::from_int(1));
        assert!(portrait_body.w < square_body.w);
        assert!(landscape_body.w > portrait_body.w);
    }

    #[test]
    fn compact_widgets_reflow_after_live_viewport_changes() {
        let mut app = App::headless(128, 128);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        install(&mut app, root);
        app.set_root(root);
        app.render().unwrap();

        crate::ui::render_system::update_layout(
            &mut app.world,
            root,
            &Viewport::new(160, 96, Fixed::ONE),
        );
        let shell = app.world.find_by_id("compact_widgets_shell").unwrap();
        assert_eq!(
            app.world.get::<Style>(shell).unwrap().layout.direction,
            FlexDirection::Row,
        );

        crate::ui::render_system::update_layout(
            &mut app.world,
            root,
            &Viewport::new(96, 160, Fixed::ONE),
        );
        assert_eq!(
            app.world.get::<Style>(shell).unwrap().layout.direction,
            FlexDirection::Column,
        );
    }

    #[test]
    fn compact_tabs_keep_distinct_pages_and_a_bounded_virtual_pool() {
        let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        install(&mut app, root);
        app.set_root(root);

        let mut pages: alloc::vec::Vec<u8> = app
            .world
            .query::<TabContent>()
            .iter()
            .map(|(_, page)| page.index)
            .collect();
        pages.sort_unstable();
        let list = app.world.find_by_id("compact_widgets_list").unwrap();
        let list_state = app.world.get::<LazyList>(list).unwrap();
        let pool = app.world.get::<LazyListPool>(list).unwrap();
        let light = app.world.find_by_id("compact_theme_light").unwrap();
        let dark = app.world.find_by_id("compact_theme_dark").unwrap();
        let light_surface = app
            .world
            .get::<CompactTheme>(light)
            .unwrap()
            .0
            .resolve(ColorToken::Surface);
        let dark_surface = app
            .world
            .get::<CompactTheme>(dark)
            .unwrap()
            .0
            .resolve(ColorToken::Surface);
        let timeline = automation(&app.world).unwrap();

        assert_eq!(pages, [0, 1, 2]);
        assert_eq!(list_state.item_count, VIRTUAL_ITEM_COUNT);
        assert_eq!(pool.items.len(), POOL_SIZE);
        assert_ne!(light_surface, dark_surface);
        assert_eq!(timeline.total_ms, 9_000);
    }

    #[test]
    fn compact_theme_buttons_replace_the_active_theme() {
        let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
        app.with_default_widgets()
            .with_default_systems()
            .with_theme(Theme::dark());
        let root = app.spawn_root().id();
        install(&mut app, root);
        app.set_root(root);

        let light = app.world.find_by_id("compact_theme_light").unwrap();
        GestureHandler::trigger(
            &mut app.world,
            light,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: light,
            },
        );
        assert_eq!(
            app.world
                .resource::<Theme>()
                .unwrap()
                .resolve(ColorToken::Surface),
            Theme::light().resolve(ColorToken::Surface),
        );

        let dark = app.world.find_by_id("compact_theme_dark").unwrap();
        GestureHandler::trigger(
            &mut app.world,
            dark,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: dark,
            },
        );
        assert_eq!(
            app.world
                .resource::<Theme>()
                .unwrap()
                .resolve(ColorToken::Surface),
            Theme::dark().resolve(ColorToken::Surface),
        );
    }
}
