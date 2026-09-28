use super::input::{pixel_tick_system, surface_gesture};
use super::render::{modal_view, surface_view};
use super::state::{PixelModalSurface, PixelNodes, PixelSurface};
use super::style::{ACTIVE, BACKGROUND, CONTROL, MUTED, PALETTE, TEXT};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::gallery::play::font::register_play_font;
use crate::gallery::play::pixel::{PixelModel, PixelTool};
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};

#[compose]
pub(super) fn build_widgets() {
    ui! {
        PixelSurface (
            id: "pixel_surface",
            width: 480,
            height: 320,
            clip_children: true
        ) [
            TouchAction::None,
        ] on Tap { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragStart { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragMove { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragEnd { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragCancel { surface_gesture(ctx.world, ctx.entity, ctx.event); }
        {
            Text (
                "PIXEL LOOM",
                position: Position::Absolute,
                left: 35,
                top: 7,
                width: 220,
                height: 22,
                font_size: 15,
                text_color: TEXT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "FRAME 1 / 4",
                id: "pixel_frame_status",
                position: Position::Absolute,
                left: 350,
                top: 7,
                width: 110,
                height: 22,
                font_size: 10,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "画一格，就改变一点",
                id: "pixel_mode_status",
                position: Position::Absolute,
                left: 17,
                top: 36,
                width: 200,
                height: 17,
                font_size: 10,
                text_color: ACTIVE,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "12 × 12",
                position: Position::Absolute,
                left: 150,
                top: 37,
                width: 58,
                height: 15,
                font_size: 8,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "LIVE PREVIEW",
                position: Position::Absolute,
                left: 243,
                top: 55,
                width: 120,
                height: 14,
                font_size: 8,
                text_color: Color::rgb(158, 153, 173),
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "星际来客",
                id: "pixel_template_name",
                position: Position::Absolute,
                left: 327,
                top: 72,
                width: 127,
                height: 21,
                font_size: 12,
                text_color: TEXT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "四帧小小剧场",
                position: Position::Absolute,
                left: 327,
                top: 94,
                width: 127,
                height: 16,
                font_size: 9,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Button (
                "4 帧/秒 ↻",
                id: "pixel_fps",
                position: Position::Absolute,
                left: 327,
                top: 109,
                width: 127,
                height: 27,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: Color::rgb(58, 52, 72),
                pressed_color: ACTIVE,
                text_color: MUTED,
                border_radius: 7
            ) on Tap { PixelNodes::update(ctx.world, PixelModel::cycle_fps); }
            Button (
                "01",
                id: "pixel_frame_0",
                position: Position::Absolute,
                left: 230,
                top: 162,
                width: 55,
                height: 50,
                size: ButtonSize::Custom,
                font_size: 7,
                normal_color: Color::rgba(0, 0, 0, 0),
                pressed_color: Color::rgba(0, 0, 0, 0),
                text_color: MUTED,
                border_radius: 6
            ) on Tap { PixelNodes::update(ctx.world, |model| model.select_frame(0)); }
            Button (
                "02",
                id: "pixel_frame_1",
                position: Position::Absolute,
                left: 290,
                top: 162,
                width: 55,
                height: 50,
                size: ButtonSize::Custom,
                font_size: 7,
                normal_color: Color::rgba(0, 0, 0, 0),
                pressed_color: Color::rgba(0, 0, 0, 0),
                text_color: MUTED,
                border_radius: 6
            ) on Tap { PixelNodes::update(ctx.world, |model| model.select_frame(1)); }
            Button (
                "03",
                id: "pixel_frame_2",
                position: Position::Absolute,
                left: 350,
                top: 162,
                width: 55,
                height: 50,
                size: ButtonSize::Custom,
                font_size: 7,
                normal_color: Color::rgba(0, 0, 0, 0),
                pressed_color: Color::rgba(0, 0, 0, 0),
                text_color: MUTED,
                border_radius: 6
            ) on Tap { PixelNodes::update(ctx.world, |model| model.select_frame(2)); }
            Button (
                "04",
                id: "pixel_frame_3",
                position: Position::Absolute,
                left: 410,
                top: 162,
                width: 55,
                height: 50,
                size: ButtonSize::Custom,
                font_size: 7,
                normal_color: Color::rgba(0, 0, 0, 0),
                pressed_color: Color::rgba(0, 0, 0, 0),
                text_color: MUTED,
                border_radius: 6
            ) on Tap { PixelNodes::update(ctx.world, |model| model.select_frame(3)); }
            Button (
                "画笔",
                id: "pixel_tool_brush",
                position: Position::Absolute,
                left: 230,
                top: 220,
                width: 55,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: ACTIVE,
                pressed_color: ACTIVE,
                text_color: BACKGROUND,
                border_radius: 7
            ) on Tap { PixelNodes::update(ctx.world, |model| model.set_tool(PixelTool::Brush)); }
            Button (
                "擦除",
                id: "pixel_tool_erase",
                position: Position::Absolute,
                left: 291,
                top: 220,
                width: 55,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PixelNodes::update(ctx.world, |model| model.set_tool(PixelTool::Erase)); }
            Button (
                "镜像",
                id: "pixel_tool_mirror",
                position: Position::Absolute,
                left: 352,
                top: 220,
                width: 55,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PixelNodes::update(ctx.world, PixelModel::toggle_mirror); }
            Button (
                "叠帧",
                id: "pixel_tool_onion",
                position: Position::Absolute,
                left: 413,
                top: 220,
                width: 55,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PixelNodes::update(ctx.world, PixelModel::toggle_onion); }
            Button (
                "",
                id: "pixel_color_1",
                position: Position::Absolute,
                left: 18,
                top: 260,
                width: 27,
                height: 17,
                size: ButtonSize::Custom,
                normal_color: PALETTE[1],
                pressed_color: PALETTE[1],
                border_radius: 4
            ) on Tap { PixelNodes::update(ctx.world, |model| model.select_color(1)); }
            Button (
                "",
                id: "pixel_color_2",
                position: Position::Absolute,
                left: 51,
                top: 260,
                width: 27,
                height: 17,
                size: ButtonSize::Custom,
                normal_color: PALETTE[2],
                pressed_color: PALETTE[2],
                border_radius: 4
            ) on Tap { PixelNodes::update(ctx.world, |model| model.select_color(2)); }
            Button (
                "",
                id: "pixel_color_3",
                position: Position::Absolute,
                left: 84,
                top: 260,
                width: 27,
                height: 17,
                size: ButtonSize::Custom,
                normal_color: PALETTE[3],
                pressed_color: PALETTE[3],
                border_radius: 4
            ) on Tap { PixelNodes::update(ctx.world, |model| model.select_color(3)); }
            Button (
                "",
                id: "pixel_color_4",
                position: Position::Absolute,
                left: 117,
                top: 260,
                width: 27,
                height: 17,
                size: ButtonSize::Custom,
                normal_color: PALETTE[4],
                pressed_color: PALETTE[4],
                border_radius: 4
            ) on Tap { PixelNodes::update(ctx.world, |model| model.select_color(4)); }
            Button (
                "",
                id: "pixel_color_5",
                position: Position::Absolute,
                left: 150,
                top: 260,
                width: 27,
                height: 17,
                size: ButtonSize::Custom,
                normal_color: PALETTE[5],
                pressed_color: PALETTE[5],
                border_radius: 4
            ) on Tap { PixelNodes::update(ctx.world, |model| model.select_color(5)); }
            Button (
                "",
                id: "pixel_color_6",
                position: Position::Absolute,
                left: 183,
                top: 260,
                width: 27,
                height: 17,
                size: ButtonSize::Custom,
                normal_color: PALETTE[6],
                pressed_color: PALETTE[6],
                border_radius: 4
            ) on Tap { PixelNodes::update(ctx.world, |model| model.select_color(6)); }
            Button (
                "复制上一帧",
                position: Position::Absolute,
                left: 230,
                top: 256,
                width: 146,
                height: 23,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 6
            ) on Tap { PixelNodes::update(ctx.world, PixelModel::copy_previous); }
            Button (
                "清空本帧",
                position: Position::Absolute,
                left: 382,
                top: 256,
                width: 86,
                height: 23,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 6
            ) on Tap { PixelNodes::update(ctx.world, PixelModel::open_clear); }
            Button (
                "模板",
                position: Position::Absolute,
                left: 12,
                top: 288,
                width: 117,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PixelNodes::update(ctx.world, PixelModel::open_templates); }
            Button (
                "撤销",
                id: "pixel_undo",
                position: Position::Absolute,
                left: 136,
                top: 288,
                width: 117,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PixelNodes::update(ctx.world, PixelModel::undo); }
            Button (
                "播放动画",
                id: "pixel_play",
                position: Position::Absolute,
                left: 260,
                top: 288,
                width: 208,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PixelNodes::update(ctx.world, PixelModel::toggle_playback); }
            PixelModalSurface (
                id: "pixel_modal",
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
                    "先借一颗灵感",
                    id: "pixel_modal_title",
                    position: Position::Absolute,
                    left: 29,
                    top: 75,
                    width: 390,
                    height: 22,
                    font_size: 13,
                    text_color: ACTIVE,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "载入模板会替换四帧；可以撤销，不会写入存储。",
                    id: "pixel_modal_subtitle",
                    position: Position::Absolute,
                    left: 29,
                    top: 94,
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
                    pressed_color: ACTIVE,
                    text_color: TEXT,
                    border_radius: 6
                ) on Tap { PixelNodes::update(ctx.world, PixelModel::close_modal); }
                Button (
                    "星际来客",
                    id: "pixel_template_0",
                    position: Position::Absolute,
                    left: 29,
                    top: 224,
                    width: 135,
                    height: 29,
                    size: ButtonSize::Compact,
                    font_size: 10,
                    normal_color: CONTROL,
                    pressed_color: ACTIVE,
                    text_color: TEXT,
                    border_radius: 7
                ) on Tap { PixelNodes::update(ctx.world, |model| model.load_template(0)); }
                Button (
                    "风中绿芽",
                    id: "pixel_template_1",
                    position: Position::Absolute,
                    left: 171,
                    top: 224,
                    width: 135,
                    height: 29,
                    size: ButtonSize::Compact,
                    font_size: 10,
                    normal_color: CONTROL,
                    pressed_color: ACTIVE,
                    text_color: TEXT,
                    border_radius: 7
                ) on Tap { PixelNodes::update(ctx.world, |model| model.load_template(1)); }
                Button (
                    "纸上飞行",
                    id: "pixel_template_2",
                    position: Position::Absolute,
                    left: 313,
                    top: 224,
                    width: 135,
                    height: 29,
                    size: ButtonSize::Compact,
                    font_size: 10,
                    normal_color: CONTROL,
                    pressed_color: ACTIVE,
                    text_color: TEXT,
                    border_radius: 7
                ) on Tap { PixelNodes::update(ctx.world, |model| model.load_template(2)); }
                Text (
                    "当前帧预览",
                    id: "pixel_clear_preview",
                    position: Position::Absolute,
                    left: 57,
                    top: 211,
                    width: 96,
                    height: 16,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label()
                )
                Button (
                    "清空这一帧",
                    id: "pixel_clear_confirm",
                    position: Position::Absolute,
                    left: 219,
                    top: 174,
                    width: 211,
                    height: 32,
                    size: ButtonSize::Compact,
                    font_size: 11,
                    normal_color: ACTIVE,
                    pressed_color: ACTIVE,
                    text_color: BACKGROUND,
                    border_radius: 7
                ) on Tap { PixelNodes::update(ctx.world, PixelModel::confirm_clear); }
                Button (
                    "保留作品",
                    id: "pixel_clear_cancel",
                    position: Position::Absolute,
                    left: 219,
                    top: 214,
                    width: 211,
                    height: 27,
                    size: ButtonSize::Compact,
                    font_size: 10,
                    normal_color: CONTROL,
                    pressed_color: ACTIVE,
                    text_color: TEXT,
                    border_radius: 7
                ) on Tap { PixelNodes::update(ctx.world, PixelModel::close_modal); }
            }
        }
    };
}

pub(super) fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    register_play_font(&mut app.world);
    app.world.insert_resource(PixelModel::default());
    app.with_widget(surface_view()).with_widget(modal_view());
    app.add_system(pixel_tick_system::system());
    #[cfg(feature = "std")]
    app.add_plugin(StdInstantClockPlugin);
    app.compose(parent, build_widgets);
    let find = |id| app.world.find_by_id(id).expect("Pixel Loom node");
    let nodes = PixelNodes {
        surface: find("pixel_surface"),
        frame_status: find("pixel_frame_status"),
        mode_status: find("pixel_mode_status"),
        template_name: find("pixel_template_name"),
        fps: find("pixel_fps"),
        play: find("pixel_play"),
        undo: find("pixel_undo"),
        frames: core::array::from_fn(|index| {
            find(
                [
                    "pixel_frame_0",
                    "pixel_frame_1",
                    "pixel_frame_2",
                    "pixel_frame_3",
                ][index],
            )
        }),
        tools: core::array::from_fn(|index| {
            find(
                [
                    "pixel_tool_brush",
                    "pixel_tool_erase",
                    "pixel_tool_mirror",
                    "pixel_tool_onion",
                ][index],
            )
        }),
        colors: core::array::from_fn(|index| {
            find(
                [
                    "pixel_color_1",
                    "pixel_color_2",
                    "pixel_color_3",
                    "pixel_color_4",
                    "pixel_color_5",
                    "pixel_color_6",
                ][index],
            )
        }),
        modal: find("pixel_modal"),
        modal_title: find("pixel_modal_title"),
        modal_subtitle: find("pixel_modal_subtitle"),
        template_buttons: core::array::from_fn(|index| {
            find(["pixel_template_0", "pixel_template_1", "pixel_template_2"][index])
        }),
        clear_controls: [
            find("pixel_clear_preview"),
            find("pixel_clear_confirm"),
            find("pixel_clear_cancel"),
        ],
    };
    app.world.insert_resource(nodes);
    PixelNodes::sync(&mut app.world);
}
