use super::input::{factory_tick_system, footer_action, modal_action, surface_gesture};
use super::render::{modal_view, surface_view};
use super::state::{FactoryModalSurface, FactoryNodes, FactorySurface};
use super::style::{ACCENT, BACKGROUND, INK, MUTED};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::gallery::play::factory::{FactoryModal, FactoryModel, FactoryPage};
use crate::gallery::play::font::register_play_font;
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};

fn label_style() -> ParagraphStyle {
    ParagraphStyle::label().with_align(TextAlign::Start)
}

#[compose]
pub(super) fn build_widgets() {
    ui! {
        FactorySurface (id: "factory_surface", width: 480, height: 320, clip_children: true) [
            TouchAction::None,
        ] on Tap { surface_gesture(ctx.world, ctx.entity, ctx.event); }
        {
            Text (
                "模块工厂",
                position: Position::Absolute,
                left: 24,
                top: 6,
                width: 170,
                height: 20,
                font_size: 14,
                text_color: BACKGROUND,
                paragraph: label_style()
            )
            Text (
                "ORDER 01 / 0:8",
                id: "factory_order_meta",
                position: Position::Absolute,
                left: 304,
                top: 8,
                width: 135,
                height: 16,
                font_size: 9,
                text_color: Color::rgb(174, 186, 190),
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Button (
                "?",
                position: Position::Absolute,
                left: 451,
                top: 4,
                width: 24,
                height: 23,
                size: ButtonSize::Compact,
                font_size: 12,
                normal_color: INK,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { FactoryNodes::update(ctx.world, |model| model.open_modal(FactoryModal::Help)); }
            Button (
                "产线",
                id: "factory_tab_line",
                position: Position::Absolute,
                left: 24,
                top: 33,
                width: 96,
                height: 24,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: ACCENT,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { FactoryNodes::update(ctx.world, |model| model.set_page(FactoryPage::Line)); }
            Button (
                "订单",
                id: "factory_tab_orders",
                position: Position::Absolute,
                left: 124,
                top: 33,
                width: 82,
                height: 24,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: INK,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { FactoryNodes::update(ctx.world, |model| model.set_page(FactoryPage::Orders)); }
            Button (
                "遥测",
                id: "factory_tab_telemetry",
                position: Position::Absolute,
                left: 210,
                top: 33,
                width: 82,
                height: 24,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: INK,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { FactoryNodes::update(ctx.world, |model| model.set_page(FactoryPage::Telemetry)); }
            Text (
                "PWR 7/9",
                id: "factory_power_meta",
                position: Position::Absolute,
                left: 302,
                top: 39,
                width: 80,
                height: 14,
                font_size: 8,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                "T+000",
                id: "factory_tick_meta",
                position: Position::Absolute,
                left: 394,
                top: 39,
                width: 63,
                height: 14,
                font_size: 8,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            View (
                id: "factory_line_page",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 282
            ) {
                Text (
                    "PRODUCTION ORDER",
                    position: Position::Absolute,
                    left: 318,
                    top: 76,
                    width: 140,
                    height: 12,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "0 / 8",
                    id: "factory_delivered",
                    position: Position::Absolute,
                    left: 319,
                    top: 88,
                    width: 139,
                    height: 27,
                    font_size: 20,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "BUILD CREDITS",
                    position: Position::Absolute,
                    left: 318,
                    top: 128,
                    width: 140,
                    height: 12,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "13 / 23",
                    id: "factory_cost",
                    position: Position::Absolute,
                    left: 388,
                    top: 128,
                    width: 68,
                    height: 15,
                    font_size: 10,
                    text_color: INK,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
                Text (
                    "SELECTED MODULE",
                    position: Position::Absolute,
                    left: 318,
                    top: 169,
                    width: 140,
                    height: 12,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "待建造空位",
                    id: "factory_selected",
                    position: Position::Absolute,
                    left: 318,
                    top: 186,
                    width: 138,
                    height: 23,
                    font_size: 13,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "C6 · R3",
                    id: "factory_selected_coord",
                    position: Position::Absolute,
                    left: 388,
                    top: 189,
                    width: 68,
                    height: 15,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
                Text (
                    "已暂停 / 在制 0 / 堵塞 0 / 退回 0",
                    id: "factory_status",
                    position: Position::Absolute,
                    left: 16,
                    top: 259,
                    width: 447,
                    height: 18,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "方向 →",
                    id: "factory_direction",
                    position: Position::Absolute,
                    left: 318,
                    top: 213,
                    width: 65,
                    height: 15,
                    font_size: 9,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "选择建造模块",
                    id: "factory_tool",
                    position: Position::Absolute,
                    left: 318,
                    top: 232,
                    width: 140,
                    height: 15,
                    font_size: 9,
                    text_color: INK,
                    paragraph: label_style()
                )
            }
            View (
                id: "factory_orders_page",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 282
            ) {
                Button (
                    "01  微型装配",
                    id: "factory_order_0",
                    position: Position::Absolute,
                    left: 20,
                    top: 92,
                    width: 276,
                    height: 45,
                    size: ButtonSize::Compact,
                    font_size: 10,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { FactoryNodes::update(ctx.world, |model| model.request_mission(0, false)); }
                Button (
                    "02  折返产线",
                    id: "factory_order_1",
                    position: Position::Absolute,
                    left: 20,
                    top: 148,
                    width: 276,
                    height: 45,
                    size: ButtonSize::Compact,
                    font_size: 10,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { FactoryNodes::update(ctx.world, |model| model.request_mission(1, false)); }
                Button (
                    "03  质量检验",
                    id: "factory_order_2",
                    position: Position::Absolute,
                    left: 20,
                    top: 204,
                    width: 276,
                    height: 45,
                    size: ButtonSize::Compact,
                    font_size: 10,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { FactoryNodes::update(ctx.world, |model| model.request_mission(2, false)); }
                Text (
                    "",
                    id: "factory_order_desc_0",
                    position: Position::Absolute,
                    left: 98,
                    top: 97,
                    width: 188,
                    height: 35,
                    font_size: 7,
                    line_height: 11,
                    text_color: BACKGROUND,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "factory_order_desc_1",
                    position: Position::Absolute,
                    left: 98,
                    top: 153,
                    width: 188,
                    height: 35,
                    font_size: 7,
                    line_height: 11,
                    text_color: BACKGROUND,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "factory_order_desc_2",
                    position: Position::Absolute,
                    left: 98,
                    top: 209,
                    width: 188,
                    height: 35,
                    font_size: 7,
                    line_height: 11,
                    text_color: BACKGROUND,
                    paragraph: label_style()
                )
                Text (
                    "MATERIAL FLOW",
                    position: Position::Absolute,
                    left: 327,
                    top: 78,
                    width: 120,
                    height: 12,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "矿石 / ORE\n↓ 熔炼 3 tick\n板材 / PLATE\n↓ 装配 4 tick\n零件 / GEAR\n↓ 质检 2 tick\n合格品 / CERT",
                    position: Position::Absolute,
                    left: 327,
                    top: 94,
                    width: 125,
                    height: 108,
                    font_size: 8,
                    line_height: 15,
                    text_color: INK,
                    paragraph: label_style()
                )
                Button (
                    "装载示范布局",
                    id: "factory_reference",
                    position: Position::Absolute,
                    left: 327,
                    top: 216,
                    width: 124,
                    height: 26,
                    size: ButtonSize::Compact,
                    font_size: 8,
                    normal_color: ACCENT,
                    pressed_color: INK,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap {
                    let mission = ctx
                        .world
                        .resource::<FactoryModel>()
                        .map_or(0, FactoryModel::mission_index);
                    FactoryNodes::update(ctx.world, |model| model.request_mission(mission, true));
                }
            }
            View (
                id: "factory_telemetry_page",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 282
            ) {
                Text (
                    "DELIVERED / LAST 80 TICKS",
                    position: Position::Absolute,
                    left: 18,
                    top: 72,
                    width: 280,
                    height: 14,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "BLOCKED CELLS",
                    position: Position::Absolute,
                    left: 18,
                    top: 180,
                    width: 280,
                    height: 14,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "试运行 tick",
                    position: Position::Absolute,
                    left: 327,
                    top: 83,
                    width: 90,
                    height: 14,
                    font_size: 9,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "累计投料",
                    position: Position::Absolute,
                    left: 327,
                    top: 112,
                    width: 90,
                    height: 14,
                    font_size: 9,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "正确交付",
                    position: Position::Absolute,
                    left: 327,
                    top: 141,
                    width: 90,
                    height: 14,
                    font_size: 9,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "退回产品",
                    position: Position::Absolute,
                    left: 327,
                    top: 170,
                    width: 90,
                    height: 14,
                    font_size: 9,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "当前在制",
                    position: Position::Absolute,
                    left: 327,
                    top: 199,
                    width: 90,
                    height: 14,
                    font_size: 9,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "0",
                    id: "factory_telemetry_tick",
                    position: Position::Absolute,
                    left: 419,
                    top: 81,
                    width: 35,
                    height: 18,
                    font_size: 12,
                    text_color: INK,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
                Text (
                    "0",
                    id: "factory_telemetry_produced",
                    position: Position::Absolute,
                    left: 419,
                    top: 110,
                    width: 35,
                    height: 18,
                    font_size: 12,
                    text_color: INK,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
                Text (
                    "0",
                    id: "factory_telemetry_delivered",
                    position: Position::Absolute,
                    left: 419,
                    top: 139,
                    width: 35,
                    height: 18,
                    font_size: 12,
                    text_color: INK,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
                Text (
                    "0",
                    id: "factory_telemetry_rejected",
                    position: Position::Absolute,
                    left: 419,
                    top: 168,
                    width: 35,
                    height: 18,
                    font_size: 12,
                    text_color: INK,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
                Text (
                    "0",
                    id: "factory_telemetry_wip",
                    position: Position::Absolute,
                    left: 419,
                    top: 197,
                    width: 35,
                    height: 18,
                    font_size: 12,
                    text_color: INK,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
                Text (
                    "曲线来自真实模型状态，不是设备性能数据。",
                    position: Position::Absolute,
                    left: 16,
                    top: 259,
                    width: 447,
                    height: 18,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
            }
            Button (
                "＋ 建造",
                id: "factory_footer_0",
                position: Position::Absolute,
                left: 8,
                top: 287,
                width: 88,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: INK,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { footer_action(ctx.world, 0); }
            Button (
                "旋转 ↻",
                id: "factory_footer_1",
                position: Position::Absolute,
                left: 101,
                top: 287,
                width: 88,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: INK,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { footer_action(ctx.world, 1); }
            Button (
                "撤销",
                id: "factory_footer_2",
                position: Position::Absolute,
                left: 194,
                top: 287,
                width: 83,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: INK,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { footer_action(ctx.world, 2); }
            Button (
                "单步",
                id: "factory_footer_3",
                position: Position::Absolute,
                left: 282,
                top: 287,
                width: 82,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: INK,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { footer_action(ctx.world, 3); }
            Button (
                "▶ 运行",
                id: "factory_footer_4",
                position: Position::Absolute,
                left: 369,
                top: 287,
                width: 103,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ACCENT,
                pressed_color: INK,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { footer_action(ctx.world, 4); }
            FactoryModalSurface (
                id: "factory_modal",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320
            ) on Tap { }
            {
                Text (
                    "建造模块",
                    id: "factory_modal_title",
                    position: Position::Absolute,
                    left: 43,
                    top: 72,
                    width: 330,
                    height: 20,
                    font_size: 14,
                    text_color: BACKGROUND,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "factory_modal_subtitle",
                    position: Position::Absolute,
                    left: 43,
                    top: 107,
                    width: 386,
                    height: 28,
                    font_size: 9,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Button (
                    "选择 / 检查",
                    id: "factory_modal_0",
                    position: Position::Absolute,
                    left: 43,
                    top: 137,
                    width: 188,
                    height: 35,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 0); }
                Button (
                    "传送带",
                    id: "factory_modal_1",
                    position: Position::Absolute,
                    left: 249,
                    top: 137,
                    width: 188,
                    height: 35,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 1); }
                Button (
                    "熔炼炉",
                    id: "factory_modal_2",
                    position: Position::Absolute,
                    left: 43,
                    top: 180,
                    width: 188,
                    height: 35,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 2); }
                Button (
                    "装配机",
                    id: "factory_modal_3",
                    position: Position::Absolute,
                    left: 249,
                    top: 180,
                    width: 188,
                    height: 35,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 3); }
                Button (
                    "质检台",
                    id: "factory_modal_4",
                    position: Position::Absolute,
                    left: 43,
                    top: 223,
                    width: 188,
                    height: 35,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 4); }
                Button (
                    "拆除模块",
                    id: "factory_modal_5",
                    position: Position::Absolute,
                    left: 249,
                    top: 223,
                    width: 188,
                    height: 35,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 5); }
                Button (
                    "×",
                    position: Position::Absolute,
                    left: 414,
                    top: 70,
                    width: 24,
                    height: 24,
                    size: ButtonSize::Compact,
                    font_size: 12,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { FactoryNodes::update(ctx.world, FactoryModel::close_modal); }
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
    app.world.insert_resource(FactoryModel::default());
    app.with_widget(surface_view()).with_widget(modal_view());
    app.add_system(factory_tick_system::system());
    app.compose(parent, build_widgets);
    let find = |id| app.world.find_by_id(id).expect("Module Factory node");
    let nodes = FactoryNodes {
        surface: find("factory_surface"),
        order_meta: find("factory_order_meta"),
        power_meta: find("factory_power_meta"),
        tick_meta: find("factory_tick_meta"),
        tabs: [
            find("factory_tab_line"),
            find("factory_tab_orders"),
            find("factory_tab_telemetry"),
        ],
        pages: [
            find("factory_line_page"),
            find("factory_orders_page"),
            find("factory_telemetry_page"),
        ],
        line_values: [
            find("factory_delivered"),
            find("factory_cost"),
            find("factory_selected"),
            find("factory_selected_coord"),
            find("factory_status"),
            find("factory_direction"),
            find("factory_tool"),
        ],
        order_buttons: [
            find("factory_order_0"),
            find("factory_order_1"),
            find("factory_order_2"),
        ],
        order_descriptions: [
            find("factory_order_desc_0"),
            find("factory_order_desc_1"),
            find("factory_order_desc_2"),
        ],
        telemetry_values: [
            find("factory_telemetry_tick"),
            find("factory_telemetry_produced"),
            find("factory_telemetry_delivered"),
            find("factory_telemetry_rejected"),
            find("factory_telemetry_wip"),
        ],
        footer: [
            find("factory_footer_0"),
            find("factory_footer_1"),
            find("factory_footer_2"),
            find("factory_footer_3"),
            find("factory_footer_4"),
        ],
        modal: find("factory_modal"),
        modal_title: find("factory_modal_title"),
        modal_subtitle: find("factory_modal_subtitle"),
        modal_buttons: [
            find("factory_modal_0"),
            find("factory_modal_1"),
            find("factory_modal_2"),
            find("factory_modal_3"),
            find("factory_modal_4"),
            find("factory_modal_5"),
        ],
    };
    app.world.insert_resource(nodes);
    FactoryNodes::sync(&mut app.world);
}
