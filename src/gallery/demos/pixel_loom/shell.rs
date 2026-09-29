use super::input::{pixel_tick_system, surface_gesture};
use super::render::{modal_view, surface_view};
use super::state::{PixelModalSurface, PixelSurface};
use super::style::{ACTIVE, BACKGROUND, CONTROL, MUTED, PALETTE, TEMPLATE_NAMES, TEXT};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::gallery::play::font::register_play_font;
use crate::gallery::play::pixel::{PixelModal, PixelModel, PixelTool};
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};
use core::fmt;

struct FrameStatus {
    frame: u8,
    fps: u8,
    playing: bool,
}

impl fmt::Display for FrameStatus {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.playing {
            write!(out, "PLAY · {} FPS", self.fps)
        } else {
            write!(out, "FRAME {} / 4", self.frame + 1)
        }
    }
}

fn control_color(active: bool, enabled: bool) -> Color {
    if active {
        ACTIVE
    } else if enabled {
        CONTROL
    } else {
        Color::rgb(42, 45, 51)
    }
}

fn control_text_color(active: bool, enabled: bool) -> Color {
    if active {
        BACKGROUND
    } else if enabled {
        TEXT
    } else {
        Color::rgb(105, 112, 107)
    }
}

fn frame_text_color(active: bool, enabled: bool) -> Color {
    if active {
        ACTIVE
    } else if enabled {
        MUTED
    } else {
        Color::rgb(105, 112, 107)
    }
}

fn palette_color(index: usize, enabled: bool) -> Color {
    if enabled {
        PALETTE[index]
    } else {
        Color::rgb(61, 62, 64)
    }
}

fn palette_border(active: bool) -> Color {
    if active {
        Color::rgb(255, 247, 220)
    } else {
        Color::rgba(0, 0, 0, 0)
    }
}

#[compose(bind(model))]
fn compose_header(model: PixelModel) -> Entity {
    ui! {
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
    };
    ui! {
        Text (
            text: ${
                FrameStatus {
                    frame: model.frame(),
                    fps: model.fps(),
                    playing: model.playing(),
                }
            },
            text_capacity: 13,
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
    };
    ui! {
        Text (
            text: ${ if model.playing() { "正在播放" } else { "画一格，就改变一点" } },
            text_capacity: 27,
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
    };
    ui! {
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
    };
    ui! {
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
    };
    ui! {
        Text (
            text: ${ TEMPLATE_NAMES[model.template_id() as usize] },
            text_capacity: 12,
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
    };
    ui! {
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
    };
    ui! {
        Button (
            text: ${ format_args!("{} 帧/秒 ↻", model.fps()) },
            text_capacity: 13,
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
        ) on Tap { model.cycle_fps(); }
    }
}

#[compose(bind(model))]
fn compose_frame_selector(model: PixelModel) -> Entity {
    ui! {
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
            text_color: ${ frame_text_color(model.visible_frame() == 0, !model.playing()) },
            border_radius: 6
        ) on Tap { model.select_frame(0); }
    };
    ui! {
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
            text_color: ${ frame_text_color(model.visible_frame() == 1, !model.playing()) },
            border_radius: 6
        ) on Tap { model.select_frame(1); }
    };
    ui! {
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
            text_color: ${ frame_text_color(model.visible_frame() == 2, !model.playing()) },
            border_radius: 6
        ) on Tap { model.select_frame(2); }
    };
    ui! {
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
            text_color: ${ frame_text_color(model.visible_frame() == 3, !model.playing()) },
            border_radius: 6
        ) on Tap { model.select_frame(3); }
    }
}

#[compose(bind(model))]
fn compose_tool_controls(model: PixelModel) -> Entity {
    ui! {
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
            normal_color: ${ control_color(model.tool() == PixelTool::Brush, !model.playing()) },
            pressed_color: ACTIVE,
            text_color: ${ control_text_color(model.tool() == PixelTool::Brush, !model.playing()) },
            border_radius: 7
        ) on Tap { model.set_tool(PixelTool::Brush); }
    };
    ui! {
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
            normal_color: ${ control_color(model.tool() == PixelTool::Erase, !model.playing()) },
            pressed_color: ACTIVE,
            text_color: ${ control_text_color(model.tool() == PixelTool::Erase, !model.playing()) },
            border_radius: 7
        ) on Tap { model.set_tool(PixelTool::Erase); }
    };
    ui! {
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
            normal_color: ${ control_color(model.mirror(), !model.playing()) },
            pressed_color: ACTIVE,
            text_color: ${ control_text_color(model.mirror(), !model.playing()) },
            border_radius: 7
        ) on Tap { model.toggle_mirror(); }
    };
    ui! {
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
            normal_color: ${ control_color(model.onion(), !model.playing()) },
            pressed_color: ACTIVE,
            text_color: ${ control_text_color(model.onion(), !model.playing()) },
            border_radius: 7
        ) on Tap { model.toggle_onion(); }
    }
}

#[compose(bind(model))]
fn compose_edit_actions(model: PixelModel) -> Entity {
    ui! {
        Button (
            "",
            id: "pixel_color_1",
            position: Position::Absolute,
            left: 18,
            top: 260,
            width: 27,
            height: 17,
            size: ButtonSize::Custom,
            normal_color: ${ palette_color(1, !model.playing()) },
            pressed_color: PALETTE[1],
            border_color: ${ palette_border(model.color() == 1) },
            border_width: ${ if model.color() == 1 { 2 } else { 0 } },
            border_radius: 4
        ) on Tap { model.select_color(1); }
    };
    ui! {
        Button (
            "",
            id: "pixel_color_2",
            position: Position::Absolute,
            left: 51,
            top: 260,
            width: 27,
            height: 17,
            size: ButtonSize::Custom,
            normal_color: ${ palette_color(2, !model.playing()) },
            pressed_color: PALETTE[2],
            border_color: ${ palette_border(model.color() == 2) },
            border_width: ${ if model.color() == 2 { 2 } else { 0 } },
            border_radius: 4
        ) on Tap { model.select_color(2); }
    };
    ui! {
        Button (
            "",
            id: "pixel_color_3",
            position: Position::Absolute,
            left: 84,
            top: 260,
            width: 27,
            height: 17,
            size: ButtonSize::Custom,
            normal_color: ${ palette_color(3, !model.playing()) },
            pressed_color: PALETTE[3],
            border_color: ${ palette_border(model.color() == 3) },
            border_width: ${ if model.color() == 3 { 2 } else { 0 } },
            border_radius: 4
        ) on Tap { model.select_color(3); }
    };
    ui! {
        Button (
            "",
            id: "pixel_color_4",
            position: Position::Absolute,
            left: 117,
            top: 260,
            width: 27,
            height: 17,
            size: ButtonSize::Custom,
            normal_color: ${ palette_color(4, !model.playing()) },
            pressed_color: PALETTE[4],
            border_color: ${ palette_border(model.color() == 4) },
            border_width: ${ if model.color() == 4 { 2 } else { 0 } },
            border_radius: 4
        ) on Tap { model.select_color(4); }
    };
    ui! {
        Button (
            "",
            id: "pixel_color_5",
            position: Position::Absolute,
            left: 150,
            top: 260,
            width: 27,
            height: 17,
            size: ButtonSize::Custom,
            normal_color: ${ palette_color(5, !model.playing()) },
            pressed_color: PALETTE[5],
            border_color: ${ palette_border(model.color() == 5) },
            border_width: ${ if model.color() == 5 { 2 } else { 0 } },
            border_radius: 4
        ) on Tap { model.select_color(5); }
    };
    ui! {
        Button (
            "",
            id: "pixel_color_6",
            position: Position::Absolute,
            left: 183,
            top: 260,
            width: 27,
            height: 17,
            size: ButtonSize::Custom,
            normal_color: ${ palette_color(6, !model.playing()) },
            pressed_color: PALETTE[6],
            border_color: ${ palette_border(model.color() == 6) },
            border_width: ${ if model.color() == 6 { 2 } else { 0 } },
            border_radius: 4
        ) on Tap { model.select_color(6); }
    };
    ui! {
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
        ) on Tap { model.copy_previous(); }
    };
    ui! {
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
        ) on Tap { model.open_clear(); }
    }
}

#[compose(bind(model))]
fn compose_footer(model: PixelModel) -> Entity {
    ui! {
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
        ) on Tap { model.open_templates(); }
    };
    ui! {
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
            normal_color: ${ control_color(false, model.can_undo() && !model.playing()) },
            pressed_color: ACTIVE,
            text_color: ${ control_text_color(false, model.can_undo() && !model.playing()) },
            border_radius: 7
        ) on Tap { model.undo(); }
    };
    ui! {
        Button (
            text: ${ if model.playing() { "暂停预览" } else { "播放动画" } },
            text_capacity: 12,
            id: "pixel_play",
            position: Position::Absolute,
            left: 260,
            top: 288,
            width: 208,
            height: 26,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(model.playing(), true) },
            pressed_color: ACTIVE,
            text_color: ${ control_text_color(model.playing(), true) },
            border_radius: 7
        ) on Tap { model.toggle_playback(); }
    }
}

#[compose(bind(model))]
fn compose_modal(model: PixelModel) -> Entity {
    ui! {
        View (
            id: "pixel_modal",
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320,
            clip_children: true,
            visible: ${ model.modal() != PixelModal::None }
        ) [
            PixelModalSurface {
                model: model.clone(),
            },
            TouchAction::None,
        ] on Tap { }
        {
            Text (
                text: ${
                    if model.modal() == PixelModal::Templates {
                        "先借一颗灵感"
                    } else {
                        "清空这一帧？"
                    }
                },
                text_capacity: 18,
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
                text: ${
                    if model.modal() == PixelModal::Templates {
                        "载入模板会替换四帧；可以撤销，不会写入存储。"
                    } else {
                        "其他三帧不受影响；清空以后也能撤销。"
                    }
                },
                text_capacity: 66,
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
            ) on Tap { model.close_modal(); }
            Button (
                "星际来客",
                id: "pixel_template_0",
                visible: ${ model.modal() == PixelModal::Templates },
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
            ) on Tap { model.load_template(0); }
            Button (
                "风中绿芽",
                id: "pixel_template_1",
                visible: ${ model.modal() == PixelModal::Templates },
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
            ) on Tap { model.load_template(1); }
            Button (
                "纸上飞行",
                id: "pixel_template_2",
                visible: ${ model.modal() == PixelModal::Templates },
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
            ) on Tap { model.load_template(2); }
            Text (
                "当前帧预览",
                id: "pixel_clear_preview",
                visible: ${ model.modal() == PixelModal::Clear },
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
                visible: ${ model.modal() == PixelModal::Clear },
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
            ) on Tap { model.confirm_clear(); }
            Button (
                "保留作品",
                id: "pixel_clear_cancel",
                visible: ${ model.modal() == PixelModal::Clear },
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
            ) on Tap { model.close_modal(); }
        }
    }
}

#[compose(bind(model))]
pub(super) fn build_widgets(model: PixelModel) {
    ui! {
        View (
            id: "pixel_surface",
            width: 480,
            height: 320,
            clip_children: true
        ) [
            PixelSurface {
                model: model.clone(),
            },
            TouchAction::None,
        ] on Tap { surface_gesture(&ctx); } on DragStart { surface_gesture(&ctx); } on DragMove { surface_gesture(&ctx); } on DragEnd { surface_gesture(&ctx); } on DragCancel { surface_gesture(&ctx); }
        {
            compose_header (model)
            compose_frame_selector (model)
            compose_tool_controls (model)
            compose_edit_actions (model)
            compose_footer (model)
            compose_modal (model)
        }
    };
}

pub(super) fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    register_play_font(&mut app.world);
    app.with_widget(surface_view()).with_widget(modal_view());
    #[cfg(feature = "std")]
    app.add_plugin(StdInstantClockPlugin);
    let model = app.add_model(PixelModel::default());
    app.add_system(pixel_tick_system::system(model.clone()));
    app.compose(parent, |cx| build_widgets(cx, model));
}
