use super::input::{cancel_node, footer_action, modal_action, orbit_tick_system};
use super::render::{modal_view, surface_view};
use super::state::{OrbitModalSurface, OrbitNodes, OrbitPreview, OrbitSurface};
use super::style::{BACKGROUND, CYAN, INK, MUTED, ORANGE, SPACE, label_style};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::gallery::play::font::register_play_font;
use crate::gallery::play::orbit::{OrbitModal, OrbitModel, OrbitPage};
use crate::prelude::*;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};

#[compose]
fn build_widgets() {
    ui! {
        OrbitSurface (id: "orbit_surface", width: 480, height: 320, clip_children: true) {
            Text (
                "轨道任务台",
                position: Position::Absolute,
                left: 24,
                top: 6,
                width: 160,
                height: 20,
                font_size: 14,
                text_color: BACKGROUND,
                paragraph: label_style()
            )
            Button (
                "任务 01 · 轨道抬升",
                id: "orbit_mission",
                position: Position::Absolute,
                left: 284,
                top: 4,
                width: 158,
                height: 23,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: INK,
                pressed_color: ORANGE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { OrbitNodes::update(ctx.world, |model| model.set_modal(OrbitModal::Missions)); }
            Button (
                "?",
                position: Position::Absolute,
                left: 450,
                top: 4,
                width: 25,
                height: 23,
                size: ButtonSize::Compact,
                font_size: 12,
                normal_color: INK,
                pressed_color: ORANGE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { OrbitNodes::update(ctx.world, |model| model.set_modal(OrbitModal::Help)); }
            Button (
                "航图",
                id: "orbit_tab_map",
                position: Position::Absolute,
                left: 24,
                top: 33,
                width: 90,
                height: 24,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: ORANGE,
                pressed_color: ORANGE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { OrbitNodes::update(ctx.world, |model| model.set_page(OrbitPage::Map)); }
            Button (
                "计划",
                id: "orbit_tab_plan",
                position: Position::Absolute,
                left: 119,
                top: 33,
                width: 82,
                height: 24,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: INK,
                pressed_color: ORANGE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { OrbitNodes::update(ctx.world, |model| model.set_page(OrbitPage::Plan)); }
            Button (
                "记录",
                id: "orbit_tab_record",
                position: Position::Absolute,
                left: 206,
                top: 33,
                width: 82,
                height: 24,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: INK,
                pressed_color: ORANGE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { OrbitNodes::update(ctx.world, |model| model.set_page(OrbitPage::Record)); }
            Text (
                "SIM T+0.0",
                id: "orbit_time",
                position: Position::Absolute,
                left: 356,
                top: 39,
                width: 102,
                height: 14,
                font_size: 8,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            View (
                id: "orbit_map_page",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 282
            ) {
                Text (
                    "48.0 / 48",
                    id: "orbit_fuel",
                    position: Position::Absolute,
                    left: 318,
                    top: 78,
                    width: 139,
                    height: 15,
                    font_size: 9,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "0%",
                    id: "orbit_heat",
                    position: Position::Absolute,
                    left: 318,
                    top: 105,
                    width: 139,
                    height: 15,
                    font_size: 9,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "78.0",
                    id: "orbit_radius",
                    position: Position::Absolute,
                    left: 318,
                    top: 136,
                    width: 62,
                    height: 18,
                    font_size: 12,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "35.8",
                    id: "orbit_speed",
                    position: Position::Absolute,
                    left: 395,
                    top: 136,
                    width: 62,
                    height: 18,
                    font_size: 12,
                    text_color: INK,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
                Text (
                    "3.8",
                    id: "orbit_dv",
                    position: Position::Absolute,
                    left: 417,
                    top: 169,
                    width: 40,
                    height: 18,
                    font_size: 12,
                    text_color: INK,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
                Button (
                    "-",
                    position: Position::Absolute,
                    left: 318,
                    top: 191,
                    width: 34,
                    height: 24,
                    size: ButtonSize::Compact,
                    font_size: 12,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { OrbitNodes::update(ctx.world, |model| model.adjust_dv(-2)); }
                Button (
                    "参考值",
                    position: Position::Absolute,
                    left: 357,
                    top: 191,
                    width: 61,
                    height: 24,
                    size: ButtonSize::Compact,
                    font_size: 8,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { OrbitNodes::update(ctx.world, OrbitModel::reset_dv); }
                Button (
                    "+",
                    position: Position::Absolute,
                    left: 423,
                    top: 191,
                    width: 34,
                    height: 24,
                    size: ButtonSize::Compact,
                    font_size: 12,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { OrbitNodes::update(ctx.world, |model| model.adjust_dv(2)); }
                Button (
                    "-15",
                    position: Position::Absolute,
                    left: 318,
                    top: 222,
                    width: 58,
                    height: 23,
                    size: ButtonSize::Compact,
                    font_size: 8,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { OrbitNodes::update(ctx.world, |model| model.adjust_angle(-15)); }
                Text (
                    "0°",
                    id: "orbit_angle",
                    position: Position::Absolute,
                    left: 377,
                    top: 226,
                    width: 36,
                    height: 13,
                    font_size: 8,
                    text_color: INK,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Center)
                )
                Button (
                    "+15",
                    position: Position::Absolute,
                    left: 414,
                    top: 222,
                    width: 43,
                    height: 23,
                    size: ButtonSize::Compact,
                    font_size: 7,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { OrbitNodes::update(ctx.world, |model| model.adjust_angle(15)); }
                Button (
                    "预演 已开",
                    id: "orbit_preview",
                    position: Position::Absolute,
                    left: 18,
                    top: 231,
                    width: 78,
                    height: 18,
                    size: ButtonSize::Compact,
                    font_size: 7,
                    normal_color: SPACE,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { OrbitNodes::update(ctx.world, OrbitModel::toggle_preview); }
                Button (
                    "1×",
                    id: "orbit_warp",
                    position: Position::Absolute,
                    left: 244,
                    top: 231,
                    width: 45,
                    height: 18,
                    size: ButtonSize::Compact,
                    font_size: 8,
                    normal_color: SPACE,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { OrbitNodes::update(ctx.world, OrbitModel::cycle_warp); }
            }
            View (
                id: "orbit_plan_page",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 282
            ) {
                Text (
                    "01  空节点",
                    id: "orbit_node_0",
                    position: Position::Absolute,
                    left: 35,
                    top: 103,
                    width: 198,
                    height: 17,
                    font_size: 9,
                    text_color: INK,
                    paragraph: label_style()
                )
                Button (
                    "取消",
                    position: Position::Absolute,
                    left: 239,
                    top: 98,
                    width: 40,
                    height: 24,
                    size: ButtonSize::Compact,
                    font_size: 7,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { cancel_node(ctx.world, 0); }
                Text (
                    "02  空节点",
                    id: "orbit_node_1",
                    position: Position::Absolute,
                    left: 35,
                    top: 151,
                    width: 198,
                    height: 17,
                    font_size: 9,
                    text_color: INK,
                    paragraph: label_style()
                )
                Button (
                    "取消",
                    position: Position::Absolute,
                    left: 239,
                    top: 146,
                    width: 40,
                    height: 24,
                    size: ButtonSize::Compact,
                    font_size: 7,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { cancel_node(ctx.world, 1); }
                Text (
                    "03  空节点",
                    id: "orbit_node_2",
                    position: Position::Absolute,
                    left: 35,
                    top: 199,
                    width: 198,
                    height: 17,
                    font_size: 9,
                    text_color: INK,
                    paragraph: label_style()
                )
                Button (
                    "取消",
                    position: Position::Absolute,
                    left: 239,
                    top: 194,
                    width: 40,
                    height: 24,
                    size: ButtonSize::Compact,
                    font_size: 7,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { cancel_node(ctx.world, 2); }
                Text (
                    "0 / 3 个节点",
                    id: "orbit_queue_count",
                    position: Position::Absolute,
                    left: 24,
                    top: 234,
                    width: 130,
                    height: 14,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "3 s",
                    id: "orbit_delay",
                    position: Position::Absolute,
                    left: 326,
                    top: 99,
                    width: 130,
                    height: 28,
                    font_size: 20,
                    text_color: BACKGROUND,
                    paragraph: label_style()
                )
                Button (
                    "−1 s",
                    position: Position::Absolute,
                    left: 326,
                    top: 186,
                    width: 57,
                    height: 25,
                    size: ButtonSize::Compact,
                    font_size: 8,
                    normal_color: Color::rgb(31, 50, 67),
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { OrbitNodes::update(ctx.world, |model| model.adjust_delay(-1)); }
                Button (
                    "＋1 s",
                    position: Position::Absolute,
                    left: 391,
                    top: 186,
                    width: 57,
                    height: 25,
                    size: ButtonSize::Compact,
                    font_size: 8,
                    normal_color: Color::rgb(31, 50, 67),
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { OrbitNodes::update(ctx.world, |model| model.adjust_delay(1)); }
            }
            View (
                id: "orbit_record_page",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 282
            ) {
                Text (
                    "0",
                    id: "orbit_samples",
                    position: Position::Absolute,
                    left: 325,
                    top: 92,
                    width: 45,
                    height: 20,
                    font_size: 14,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "0",
                    id: "orbit_trail",
                    position: Position::Absolute,
                    left: 392,
                    top: 92,
                    width: 45,
                    height: 20,
                    font_size: 14,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "78.0",
                    id: "orbit_record_radius",
                    position: Position::Absolute,
                    left: 325,
                    top: 134,
                    width: 112,
                    height: 18,
                    font_size: 12,
                    text_color: ORANGE,
                    paragraph: label_style()
                )
                Text (
                    "35.8",
                    id: "orbit_record_speed",
                    position: Position::Absolute,
                    left: 325,
                    top: 158,
                    width: 112,
                    height: 18,
                    font_size: 12,
                    text_color: CYAN,
                    paragraph: label_style()
                )
                Text (
                    "等待事件",
                    id: "orbit_event",
                    position: Position::Absolute,
                    left: 325,
                    top: 194,
                    width: 124,
                    height: 28,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "READY",
                    id: "orbit_status",
                    position: Position::Absolute,
                    left: 325,
                    top: 231,
                    width: 124,
                    height: 15,
                    font_size: 9,
                    text_color: ORANGE,
                    paragraph: label_style()
                )
            }
            Button (
                "▶ 滑行",
                id: "orbit_footer_run",
                position: Position::Absolute,
                left: 12,
                top: 288,
                width: 82,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ORANGE,
                pressed_color: ORANGE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { footer_action(ctx.world, 0); }
            Button (
                "点火",
                id: "orbit_footer_burn",
                position: Position::Absolute,
                left: 99,
                top: 288,
                width: 70,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: INK,
                pressed_color: ORANGE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { footer_action(ctx.world, 1); }
            Button (
                "采样",
                id: "orbit_footer_scan",
                position: Position::Absolute,
                left: 174,
                top: 288,
                width: 91,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: INK,
                pressed_color: ORANGE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { footer_action(ctx.world, 2); }
            Button (
                "＋ 节点",
                id: "orbit_footer_schedule",
                position: Position::Absolute,
                left: 270,
                top: 288,
                width: 85,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: INK,
                pressed_color: ORANGE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { footer_action(ctx.world, 3); }
            Button (
                "重新装载",
                id: "orbit_footer_reset",
                position: Position::Absolute,
                left: 360,
                top: 288,
                width: 108,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: INK,
                pressed_color: ORANGE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { footer_action(ctx.world, 4); }
            OrbitModalSurface (
                id: "orbit_modal",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320
            ) {
                Text (
                    "选择轨道任务",
                    id: "orbit_modal_title",
                    position: Position::Absolute,
                    left: 55,
                    top: 80,
                    width: 300,
                    height: 20,
                    font_size: 13,
                    text_color: BACKGROUND,
                    paragraph: label_style()
                )
                Text (
                    "切换会清空当前轨迹、计划与遥测记录。",
                    id: "orbit_modal_subtitle",
                    position: Position::Absolute,
                    left: 55,
                    top: 116,
                    width: 360,
                    height: 28,
                    font_size: 9,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Button (
                    "轨道抬升",
                    id: "orbit_modal_0",
                    position: Position::Absolute,
                    left: 55,
                    top: 151,
                    width: 176,
                    height: 35,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: ORANGE,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 0); }
                Button (
                    "往返测绘",
                    id: "orbit_modal_1",
                    position: Position::Absolute,
                    left: 249,
                    top: 151,
                    width: 176,
                    height: 35,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 1); }
                Button (
                    "远端通信",
                    id: "orbit_modal_2",
                    position: Position::Absolute,
                    left: 55,
                    top: 198,
                    width: 176,
                    height: 35,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 2); }
                Button (
                    "关闭",
                    id: "orbit_modal_3",
                    position: Position::Absolute,
                    left: 249,
                    top: 198,
                    width: 176,
                    height: 35,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 3); }
                Button (
                    "×",
                    position: Position::Absolute,
                    left: 404,
                    top: 77,
                    width: 27,
                    height: 24,
                    size: ButtonSize::Compact,
                    font_size: 12,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { OrbitNodes::update(ctx.world, |model| model.set_modal(OrbitModal::None)); }
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
    register_play_font(&mut app.world);
    app.world.insert_resource(OrbitModel::default());
    app.world.insert_resource(OrbitPreview::default());
    app.with_widget(surface_view()).with_widget(modal_view());
    app.add_system(orbit_tick_system::system());
    app.compose(parent, build_widgets);
    let find = |id| app.world.find_by_id(id).expect("Orbital Mission node");
    let nodes = OrbitNodes {
        surface: find("orbit_surface"),
        mission_meta: find("orbit_mission"),
        time_meta: find("orbit_time"),
        tabs: [
            find("orbit_tab_map"),
            find("orbit_tab_plan"),
            find("orbit_tab_record"),
        ],
        pages: [
            find("orbit_map_page"),
            find("orbit_plan_page"),
            find("orbit_record_page"),
        ],
        map_values: [
            find("orbit_fuel"),
            find("orbit_heat"),
            find("orbit_radius"),
            find("orbit_speed"),
            find("orbit_dv"),
            find("orbit_angle"),
            find("orbit_preview"),
            find("orbit_warp"),
        ],
        plan_values: [
            find("orbit_delay"),
            find("orbit_node_0"),
            find("orbit_node_1"),
            find("orbit_node_2"),
            find("orbit_queue_count"),
        ],
        record_values: [
            find("orbit_samples"),
            find("orbit_trail"),
            find("orbit_record_radius"),
            find("orbit_record_speed"),
            find("orbit_event"),
            find("orbit_status"),
        ],
        footer: [
            find("orbit_footer_run"),
            find("orbit_footer_burn"),
            find("orbit_footer_scan"),
            find("orbit_footer_schedule"),
            find("orbit_footer_reset"),
        ],
        modal: find("orbit_modal"),
        modal_title: find("orbit_modal_title"),
        modal_subtitle: find("orbit_modal_subtitle"),
        modal_buttons: [
            find("orbit_modal_0"),
            find("orbit_modal_1"),
            find("orbit_modal_2"),
            find("orbit_modal_3"),
        ],
    };
    app.world.insert_resource(nodes);
    OrbitNodes::refresh_preview(&mut app.world);
    OrbitNodes::sync(&mut app.world);
}
