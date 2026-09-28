use super::input::{PostKeyboardPlugin, post_tick_system, surface_gesture};
use super::render::{modal_view, surface_view};
use super::state::{PostModalSurface, PostNodes, PostSurface};
use super::style::{ACCENT, BACKGROUND, CONTROL, MUTED, STATION_COLORS, TEXT};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::gallery::play::font::register_play_font;
use crate::gallery::play::post::PostModel;
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};

#[compose]
pub(super) fn build_widgets() {
    ui! {
        PostSurface (id: "post_surface", width: 480, height: 320, clip_children: true) [
            TouchAction::None,
        ] on Tap { surface_gesture(ctx.world, ctx.entity, ctx.event); }
        {
            Text (
                "POCKET POST",
                position: Position::Absolute,
                left: 35,
                top: 7,
                width: 240,
                height: 22,
                font_size: 14,
                text_color: TEXT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "SORTED 0 / 9",
                id: "post_sorted",
                position: Position::Absolute,
                left: 306,
                top: 9,
                width: 158,
                height: 18,
                font_size: 9,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "准备好，拨动你的第一班轨道",
                id: "post_status",
                position: Position::Absolute,
                left: 16,
                top: 42,
                width: 282,
                height: 17,
                font_size: 10,
                text_color: ACCENT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "错投 0   连对 0",
                id: "post_misses",
                position: Position::Absolute,
                left: 304,
                top: 43,
                width: 160,
                height: 15,
                font_size: 8,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "IN",
                position: Position::Absolute,
                left: 19,
                top: 142,
                width: 28,
                height: 13,
                font_size: 8,
                text_color: ACCENT,
                paragraph: ParagraphStyle::label()
            )
            Text (
                "S1",
                position: Position::Absolute,
                left: 137,
                top: 181,
                width: 28,
                height: 14,
                font_size: 8,
                text_color: ACCENT,
                paragraph: ParagraphStyle::label()
            )
            Text (
                "S2",
                position: Position::Absolute,
                left: 251,
                top: 181,
                width: 28,
                height: 14,
                font_size: 8,
                text_color: ACCENT,
                paragraph: ParagraphStyle::label()
            )
            Text (
                "A",
                position: Position::Absolute,
                left: 393,
                top: 75,
                width: 18,
                height: 14,
                font_size: 9,
                text_color: STATION_COLORS[0],
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "薄荷港",
                position: Position::Absolute,
                left: 393,
                top: 90,
                width: 48,
                height: 12,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "0",
                id: "post_station_0",
                position: Position::Absolute,
                left: 432,
                top: 80,
                width: 15,
                height: 18,
                font_size: 13,
                text_color: STATION_COLORS[0],
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "B",
                position: Position::Absolute,
                left: 393,
                top: 143,
                width: 18,
                height: 14,
                font_size: 9,
                text_color: STATION_COLORS[1],
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "紫藤站",
                position: Position::Absolute,
                left: 393,
                top: 158,
                width: 48,
                height: 12,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "0",
                id: "post_station_1",
                position: Position::Absolute,
                left: 432,
                top: 148,
                width: 15,
                height: 18,
                font_size: 13,
                text_color: STATION_COLORS[1],
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "C",
                position: Position::Absolute,
                left: 393,
                top: 211,
                width: 18,
                height: 14,
                font_size: 9,
                text_color: STATION_COLORS[2],
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "日落湾",
                position: Position::Absolute,
                left: 393,
                top: 226,
                width: 48,
                height: 12,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "0",
                id: "post_station_2",
                position: Position::Absolute,
                left: 432,
                top: 216,
                width: 15,
                height: 18,
                font_size: 13,
                text_color: STATION_COLORS[2],
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "待发",
                position: Position::Absolute,
                left: 23,
                top: 243,
                width: 28,
                height: 13,
                font_size: 8,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "",
                id: "post_queue_0",
                position: Position::Absolute,
                left: 47,
                top: 241,
                width: 17,
                height: 15,
                font_size: 8,
                text_color: TEXT,
                paragraph: ParagraphStyle::label()
            )
            Text (
                "",
                id: "post_queue_1",
                position: Position::Absolute,
                left: 72,
                top: 241,
                width: 17,
                height: 15,
                font_size: 8,
                text_color: TEXT,
                paragraph: ParagraphStyle::label()
            )
            Text (
                "",
                id: "post_queue_2",
                position: Position::Absolute,
                left: 97,
                top: 241,
                width: 17,
                height: 15,
                font_size: 8,
                text_color: TEXT,
                paragraph: ParagraphStyle::label()
            )
            Text (
                "",
                id: "post_queue_3",
                position: Position::Absolute,
                left: 122,
                top: 241,
                width: 17,
                height: 15,
                font_size: 8,
                text_color: TEXT,
                paragraph: ParagraphStyle::label()
            )
            Text (
                "",
                id: "post_queue_4",
                position: Position::Absolute,
                left: 147,
                top: 241,
                width: 17,
                height: 15,
                font_size: 8,
                text_color: TEXT,
                paragraph: ParagraphStyle::label()
            )
            Text (
                "0 / 3 在途",
                id: "post_in_transit",
                position: Position::Absolute,
                left: 218,
                top: 243,
                width: 86,
                height: 13,
                font_size: 8,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Button (
                "班次",
                position: Position::Absolute,
                left: 387,
                top: 242,
                width: 66,
                height: 22,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: Color::rgb(49, 72, 76),
                pressed_color: ACCENT,
                text_color: TEXT,
                border_radius: 6
            ) on Tap { PostNodes::update(ctx.world, PostModel::open_manifests); }
            Button (
                "开始",
                id: "post_run",
                position: Position::Absolute,
                left: 12,
                top: 288,
                width: 129,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: ACCENT,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 7
            ) on Tap { PostNodes::update(ctx.world, PostModel::toggle_running); }
            Button (
                "加发一件",
                id: "post_send",
                position: Position::Absolute,
                left: 148,
                top: 288,
                width: 113,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACCENT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PostNodes::update(ctx.world, PostModel::spawn_manual); }
            Button (
                "1× 速度",
                id: "post_speed",
                position: Position::Absolute,
                left: 268,
                top: 288,
                width: 95,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACCENT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PostNodes::update(ctx.world, PostModel::cycle_speed); }
            Button (
                "重来",
                position: Position::Absolute,
                left: 370,
                top: 288,
                width: 98,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACCENT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PostNodes::update(ctx.world, PostModel::open_reset); }
            PostModalSurface (
                id: "post_modal",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                clip_children: true
            ) [
                TouchAction::None,
            ] on Tap { }
            {
                Text (
                    "",
                    id: "post_modal_title",
                    position: Position::Absolute,
                    left: 29,
                    top: 76,
                    width: 390,
                    height: 22,
                    font_size: 13,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "",
                    id: "post_modal_subtitle",
                    position: Position::Absolute,
                    left: 29,
                    top: 96,
                    width: 410,
                    height: 15,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Button (
                    "×",
                    position: Position::Absolute,
                    left: 428,
                    top: 73,
                    width: 24,
                    height: 22,
                    size: ButtonSize::Compact,
                    font_size: 12,
                    normal_color: CONTROL,
                    pressed_color: ACCENT,
                    text_color: TEXT,
                    border_radius: 6
                ) on Tap { PostNodes::update(ctx.world, PostModel::close_modal); }
                Button (
                    "晨间邮路 · 9 件",
                    id: "post_manifest_0",
                    position: Position::Absolute,
                    left: 29,
                    top: 113,
                    width: 420,
                    height: 37,
                    size: ButtonSize::Compact,
                    font_size: 11,
                    normal_color: CONTROL,
                    pressed_color: ACCENT,
                    text_color: TEXT,
                    border_radius: 7
                ) on Tap { PostNodes::update(ctx.world, |model| model.load_manifest(0)); }
                Button (
                    "忙碌午后 · 12 件",
                    id: "post_manifest_1",
                    position: Position::Absolute,
                    left: 29,
                    top: 160,
                    width: 420,
                    height: 37,
                    size: ButtonSize::Compact,
                    font_size: 11,
                    normal_color: CONTROL,
                    pressed_color: ACCENT,
                    text_color: TEXT,
                    border_radius: 7
                ) on Tap { PostNodes::update(ctx.world, |model| model.load_manifest(1)); }
                Button (
                    "慢慢练习 · 6 件",
                    id: "post_manifest_2",
                    position: Position::Absolute,
                    left: 29,
                    top: 207,
                    width: 420,
                    height: 37,
                    size: ButtonSize::Compact,
                    font_size: 11,
                    normal_color: CONTROL,
                    pressed_color: ACCENT,
                    text_color: TEXT,
                    border_radius: 7
                ) on Tap { PostNodes::update(ctx.world, |model| model.load_manifest(2)); }
                Text (
                    "0000",
                    id: "post_summary_score",
                    position: Position::Absolute,
                    left: 42,
                    top: 119,
                    width: 175,
                    height: 48,
                    font_size: 35,
                    text_color: ACCENT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "POST POINTS",
                    id: "post_summary_points",
                    position: Position::Absolute,
                    left: 44,
                    top: 166,
                    width: 130,
                    height: 13,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "正确 0 件",
                    id: "post_summary_correct",
                    position: Position::Absolute,
                    left: 270,
                    top: 121,
                    width: 150,
                    height: 22,
                    font_size: 14,
                    text_color: STATION_COLORS[0],
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "错投 0 件",
                    id: "post_summary_missed",
                    position: Position::Absolute,
                    left: 270,
                    top: 150,
                    width: 150,
                    height: 20,
                    font_size: 12,
                    text_color: STATION_COLORS[2],
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Button (
                    "再开一班",
                    id: "post_summary_again",
                    position: Position::Absolute,
                    left: 29,
                    top: 217,
                    width: 204,
                    height: 34,
                    size: ButtonSize::Compact,
                    font_size: 11,
                    normal_color: ACCENT,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 7
                ) on Tap { PostNodes::update(ctx.world, PostModel::restart); }
                Button (
                    "换个班次",
                    id: "post_summary_choose",
                    position: Position::Absolute,
                    left: 245,
                    top: 217,
                    width: 204,
                    height: 34,
                    size: ButtonSize::Compact,
                    font_size: 11,
                    normal_color: CONTROL,
                    pressed_color: ACCENT,
                    text_color: TEXT,
                    border_radius: 7
                ) on Tap { PostNodes::update(ctx.world, PostModel::open_manifests); }
                Text (
                    "只影响当前内存中的进度。",
                    id: "post_reset_note",
                    position: Position::Absolute,
                    left: 171,
                    top: 146,
                    width: 257,
                    height: 16,
                    font_size: 9,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Button (
                    "重新开始",
                    id: "post_reset_confirm",
                    position: Position::Absolute,
                    left: 171,
                    top: 185,
                    width: 257,
                    height: 32,
                    size: ButtonSize::Compact,
                    font_size: 11,
                    normal_color: ACCENT,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 7
                ) on Tap { PostNodes::update(ctx.world, PostModel::restart); }
                Button (
                    "保留班次",
                    id: "post_reset_cancel",
                    position: Position::Absolute,
                    left: 171,
                    top: 225,
                    width: 257,
                    height: 25,
                    size: ButtonSize::Compact,
                    font_size: 10,
                    normal_color: CONTROL,
                    pressed_color: ACCENT,
                    text_color: TEXT,
                    border_radius: 7
                ) on Tap { PostNodes::update(ctx.world, PostModel::close_modal); }
                Text (
                    "当前班次",
                    id: "post_reset_badge",
                    position: Position::Absolute,
                    left: 67,
                    top: 226,
                    width: 61,
                    height: 14,
                    font_size: 8,
                    text_color: ACCENT,
                    paragraph: ParagraphStyle::label()
                )
            }
        }
    };
}

pub(super) fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    #[cfg(feature = "std")]
    app.add_plugin(StdInstantClockPlugin);
    app.add_plugin(PostKeyboardPlugin);
    register_play_font(&mut app.world);
    app.world.insert_resource(PostModel::default());
    app.with_widget(surface_view()).with_widget(modal_view());
    app.add_system(post_tick_system::system());
    app.compose(parent, build_widgets);
    let find = |id| app.world.find_by_id(id).expect("Pocket Post node");
    let nodes = PostNodes {
        surface: find("post_surface"),
        status: find("post_status"),
        sorted: find("post_sorted"),
        misses: find("post_misses"),
        queue: [
            find("post_queue_0"),
            find("post_queue_1"),
            find("post_queue_2"),
            find("post_queue_3"),
            find("post_queue_4"),
        ],
        in_transit: find("post_in_transit"),
        station_counts: [
            find("post_station_0"),
            find("post_station_1"),
            find("post_station_2"),
        ],
        run: find("post_run"),
        send: find("post_send"),
        speed: find("post_speed"),
        modal: find("post_modal"),
        modal_title: find("post_modal_title"),
        modal_subtitle: find("post_modal_subtitle"),
        manifest_controls: [
            find("post_manifest_0"),
            find("post_manifest_1"),
            find("post_manifest_2"),
        ],
        summary_controls: [
            find("post_summary_score"),
            find("post_summary_points"),
            find("post_summary_correct"),
            find("post_summary_missed"),
            find("post_summary_again"),
            find("post_summary_choose"),
        ],
        reset_controls: [
            find("post_reset_note"),
            find("post_reset_confirm"),
            find("post_reset_cancel"),
            find("post_reset_badge"),
        ],
    };
    app.world.insert_resource(nodes);
    PostNodes::sync(&mut app.world);
}
