use super::input::{
    keyboard, pixel_tick_system, surface_drag_end, surface_drag_move, surface_drag_start,
    surface_tap,
};
#[cfg(feature = "persistence")]
use super::persistence::install_persistence;
use super::render::{
    COLUMN_LEFT, COLUMN_WIDTH, FRAMES_TOP, PREVIEW, THUMB_HEIGHT, THUMB_STEP, THUMB_WIDTH,
    modal_render, preview_render, surface_render,
};
use super::state::{PixelModalSurface, PixelPreview, PixelSurface};
use super::style::{ACTIVE, BACKGROUND, BOARD, CONTROL, MUTED, PALETTE, TEMPLATE_NAMES, TEXT};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::core::i18n::{Locale, locale};
use crate::gallery::play::font::GameFontPlugin;
use crate::gallery::play::pixel::{PixelClipboardStatus, PixelModal, PixelModel, PixelTool};
use crate::input::event::focus::Focusable;
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::tr;
use crate::ui::ComputedRect;
use crate::ui::clipboard::{ClipboardFinished, ClipboardOutcome};
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};
use core::fmt;

/// The title row's right side: the last clipboard result until the next
/// edit, otherwise the frame being edited or the playback speed.
struct FrameStatus {
    clipboard: PixelClipboardStatus,
    frame: u8,
    fps: u8,
    playing: bool,
}

impl fmt::Display for FrameStatus {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.clipboard != PixelClipboardStatus::Ready {
            return out.write_str(self.clipboard.label().get());
        }
        match (self.playing, locale() == Locale::ZhCn) {
            (true, true) => write!(out, "▶ {} 帧/秒", self.fps),
            (true, false) => write!(out, "▶ {} FPS", self.fps),
            (false, true) => write!(out, "第 {}/4 帧", self.frame + 1),
            (false, false) => write!(out, "FRAME {}/4", self.frame + 1),
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

fn palette_border(active: bool) -> Color {
    if active {
        Color::rgb(255, 247, 220)
    } else {
        Color::rgba(0, 0, 0, 0)
    }
}

fn frame_left(index: i32) -> i32 {
    index * THUMB_STEP
}

#[compose]
fn compose_header(model: model!(PixelModel)) -> Entity {
    ui! {
        Text (
            tr!(en : "PIXEL LOOM", zh : "像素织机"),
            position: Position::Absolute,
            left: 12,
            top: 6,
            width: 64,
            height: 14,
            font_size: 10,
            text_color: TEXT,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            text: ${
                FrameStatus {
                    clipboard: model.clipboard_status(),
                    frame: model.frame(),
                    fps: model.fps(),
                    playing: model.playing(),
                }
            },
            capacity: 32,
            id: "pixel_frame_status",
            position: Position::Absolute,
            left: 76,
            top: 6,
            width: 80,
            height: 14,
            font_size: 10,
            text_color: ${ if model.clipboard_status() == PixelClipboardStatus::Ready { MUTED } else { ACTIVE } },
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    }
}

#[compose]
fn compose_preview(model: model!(PixelModel)) -> Entity {
    ui! {
        View (
            id: "pixel_preview",
            position: Position::Absolute,
            left: PREVIEW.0,
            top: PREVIEW.1,
            width: PREVIEW.2,
            height: PREVIEW.3
        ) [
            PixelPreview {
                model: model.clone(),
            },
        ]
    };
    ui! {
        Text (
            text: ${ TEMPLATE_NAMES[model.template_id() as usize] },
            capacity: 20,
            id: "pixel_template_name",
            position: Position::Absolute,
            left: 74,
            top: 30,
            width: 78,
            height: 13,
            font_size: 10,
            text_color: TEXT,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Button (
            text: ${
                if model.playing() {
                    tr!(en : "Ⅱ PAUSE", zh : "Ⅱ 暂停")
                } else {
                    tr!(en : "▶ PLAY", zh : "▶ 播放")
                }
            },
            capacity: 20,
            id: "pixel_play",
            position: Position::Absolute,
            left: 74,
            top: 46,
            width: 78,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(model.playing(), true) },
            pressed_color: ACTIVE,
            text_color: ${ control_text_color(model.playing(), true) },
            border_radius: 6
        ) on Tap { model.toggle_playback(); }
    };
    ui! {
        Button (
            text: ${ format_args!("{} {} ↻", model.fps(), tr!(en : "FPS", zh : "帧/秒")) },
            capacity: 20,
            id: "pixel_fps",
            position: Position::Absolute,
            left: 74,
            top: 72,
            width: 78,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: Color::rgb(58, 52, 72),
            pressed_color: ACTIVE,
            pressed_text_color: BOARD,
            text_color: MUTED,
            border_radius: 6
        ) on Tap { model.cycle_fps(); }
    }
}

#[compose]
fn compose_frame_selector(model: model!(PixelModel)) -> Entity {
    ui! {
        Button (
            "",
            id: "pixel_frame_0",
            position: Position::Absolute,
            left: frame_left(0),
            top: FRAMES_TOP,
            width: THUMB_WIDTH,
            height: THUMB_HEIGHT,
            size: ButtonSize::Custom,
            normal_color: Color::rgba(0, 0, 0, 0),
            pressed_color: Color::rgba(203, 179, 240, 48),
            border_radius: 6
        ) on Tap { model.select_frame(0); }
    };
    ui! {
        Button (
            "",
            id: "pixel_frame_1",
            position: Position::Absolute,
            left: frame_left(1),
            top: FRAMES_TOP,
            width: THUMB_WIDTH,
            height: THUMB_HEIGHT,
            size: ButtonSize::Custom,
            normal_color: Color::rgba(0, 0, 0, 0),
            pressed_color: Color::rgba(203, 179, 240, 48),
            border_radius: 6
        ) on Tap { model.select_frame(1); }
    };
    ui! {
        Button (
            "",
            id: "pixel_frame_2",
            position: Position::Absolute,
            left: frame_left(2),
            top: FRAMES_TOP,
            width: THUMB_WIDTH,
            height: THUMB_HEIGHT,
            size: ButtonSize::Custom,
            normal_color: Color::rgba(0, 0, 0, 0),
            pressed_color: Color::rgba(203, 179, 240, 48),
            border_radius: 6
        ) on Tap { model.select_frame(2); }
    };
    ui! {
        Button (
            "",
            id: "pixel_frame_3",
            position: Position::Absolute,
            left: frame_left(3),
            top: FRAMES_TOP,
            width: THUMB_WIDTH,
            height: THUMB_HEIGHT,
            size: ButtonSize::Custom,
            normal_color: Color::rgba(0, 0, 0, 0),
            pressed_color: Color::rgba(203, 179, 240, 48),
            border_radius: 6
        ) on Tap { model.select_frame(3); }
    }
}

#[compose]
fn compose_tool_controls(model: model!(PixelModel)) -> Entity {
    ui! {
        Button (
            tr!(en : "BRUSH", zh : "画笔"),
            id: "pixel_tool_brush",
            position: Position::Absolute,
            left: 0,
            top: 144,
            width: 76,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(model.tool() == PixelTool::Brush, true) },
            pressed_color: ACTIVE,
            text_color: ${ control_text_color(model.tool() == PixelTool::Brush, true) },
            border_radius: 6
        ) on Tap { model.set_tool(PixelTool::Brush); }
    };
    ui! {
        Button (
            tr!(en : "ERASE", zh : "擦除"),
            id: "pixel_tool_erase",
            position: Position::Absolute,
            left: 80,
            top: 144,
            width: 76,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(model.tool() == PixelTool::Erase, true) },
            pressed_color: ACTIVE,
            text_color: ${ control_text_color(model.tool() == PixelTool::Erase, true) },
            border_radius: 6
        ) on Tap { model.set_tool(PixelTool::Erase); }
    };
    ui! {
        Button (
            tr!(en : "MIRROR", zh : "镜像"),
            id: "pixel_tool_mirror",
            position: Position::Absolute,
            left: 0,
            top: 172,
            width: 76,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(model.mirror(), true) },
            pressed_color: ACTIVE,
            text_color: ${ control_text_color(model.mirror(), true) },
            border_radius: 6
        ) on Tap { model.toggle_mirror(); }
    };
    ui! {
        Button (
            tr!(en : "ONION", zh : "叠帧"),
            id: "pixel_tool_onion",
            position: Position::Absolute,
            left: 80,
            top: 172,
            width: 76,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(model.onion(), true) },
            pressed_color: ACTIVE,
            text_color: ${ control_text_color(model.onion(), true) },
            border_radius: 6
        ) on Tap { model.toggle_onion(); }
    }
}

#[compose]
fn compose_palette(model: model!(PixelModel)) -> Entity {
    ui! {
        Button (
            "",
            id: "pixel_color_1",
            position: Position::Absolute,
            left: 0,
            top: 200,
            width: 21,
            height: 20,
            size: ButtonSize::Custom,
            normal_color: PALETTE[1],
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
            left: 27,
            top: 200,
            width: 21,
            height: 20,
            size: ButtonSize::Custom,
            normal_color: PALETTE[2],
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
            left: 54,
            top: 200,
            width: 21,
            height: 20,
            size: ButtonSize::Custom,
            normal_color: PALETTE[3],
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
            left: 81,
            top: 200,
            width: 21,
            height: 20,
            size: ButtonSize::Custom,
            normal_color: PALETTE[4],
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
            left: 108,
            top: 200,
            width: 21,
            height: 20,
            size: ButtonSize::Custom,
            normal_color: PALETTE[5],
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
            left: 135,
            top: 200,
            width: 21,
            height: 20,
            size: ButtonSize::Custom,
            normal_color: PALETTE[6],
            pressed_color: PALETTE[6],
            border_color: ${ palette_border(model.color() == 6) },
            border_width: ${ if model.color() == 6 { 2 } else { 0 } },
            border_radius: 4
        ) on Tap { model.select_color(6); }
    }
}

#[compose]
fn compose_edit_actions(model: model!(PixelModel), editor: Entity) -> Entity {
    ui! {
        Button (
            "↺",
            id: "pixel_undo",
            position: Position::Absolute,
            left: 0,
            top: 224,
            width: 36,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(false, model.can_undo()) },
            pressed_color: ACTIVE,
            text_color: ${ control_text_color(false, model.can_undo()) },
            border_radius: 6
        ) on Tap { model.undo(); }
    };
    ui! {
        Button (
            tr!(en : "COPY PREVIOUS", zh : "复制上一帧"),
            id: "pixel_copy_previous",
            position: Position::Absolute,
            left: 40,
            top: 224,
            width: 116,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: CONTROL,
            pressed_color: ACTIVE,
            pressed_text_color: BOARD,
            text_color: TEXT,
            border_radius: 6
        ) on Tap { model.copy_previous(); }
    };
    ui! {
        Button (
            tr!(en : "CLEAR", zh : "清空"),
            id: "pixel_clear",
            position: Position::Absolute,
            left: 0,
            top: 252,
            width: 56,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: CONTROL,
            pressed_color: ACTIVE,
            pressed_text_color: BOARD,
            text_color: TEXT,
            border_radius: 6
        ) on Tap { model.open_clear(); }
    };
    ui! {
        Button (
            tr!(en : "TEMPLATES", zh : "模板"),
            id: "pixel_templates",
            position: Position::Absolute,
            left: 60,
            top: 252,
            width: 96,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: CONTROL,
            pressed_color: ACTIVE,
            pressed_text_color: BOARD,
            text_color: TEXT,
            border_radius: 6
        ) on Tap { model.open_templates(); }
    };
    ui! {
        Button (
            tr!(en : "COPY", zh : "复制"),
            id: "pixel_copy",
            position: Position::Absolute,
            left: 0,
            top: 280,
            width: 49,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: CONTROL,
            pressed_color: ACTIVE,
            pressed_text_color: BOARD,
            text_color: TEXT,
            border_radius: 6
        ) [
            PointerFocus::Preserve,
        ] on Tap { commands!().clipboard().copy(editor); }
    };
    ui! {
        Button (
            tr!(en : "CUT", zh : "剪切"),
            id: "pixel_cut",
            position: Position::Absolute,
            left: 53,
            top: 280,
            width: 50,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: CONTROL,
            pressed_color: ACTIVE,
            pressed_text_color: BOARD,
            text_color: TEXT,
            border_radius: 6
        ) [
            PointerFocus::Preserve,
        ] on Tap { commands!().clipboard().cut(editor); }
    };
    ui! {
        Button (
            tr!(en : "PASTE", zh : "粘贴"),
            id: "pixel_paste",
            position: Position::Absolute,
            left: 107,
            top: 280,
            width: 49,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: CONTROL,
            pressed_color: ACTIVE,
            pressed_text_color: BOARD,
            text_color: TEXT,
            border_radius: 6
        ) [
            PointerFocus::Preserve,
        ] on Tap { commands!().clipboard().paste(editor); }
    }
}

#[compose]
fn compose_modal(model: model!(PixelModel)) -> Entity {
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
        ] {
            Text (
                text: ${
                    if model.modal() == PixelModal::Templates {
                        tr!(en : "PICK A TEMPLATE", zh : "选一个模板")
                    } else {
                        tr!(en : "CLEAR THIS FRAME?", zh : "清空这一帧？")
                    }
                },
                capacity: 32,
                id: "pixel_modal_title",
                position: Position::Absolute,
                left: 29,
                top: 71,
                width: 390,
                height: 24,
                font_size: 20,
                text_color: ACTIVE,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                text: ${
                    if model.modal() == PixelModal::Templates {
                        tr!(
                            en : "Replaces all four frames. Undo brings them back.", zh :
                            "会替换现在的四帧，可以撤销。"
                        )
                    } else {
                        tr!(
                            en : "Clears only this frame. Undo brings it back.", zh :
                            "只清空这一帧，可以撤销。"
                        )
                    }
                },
                capacity: 64,
                id: "pixel_modal_subtitle",
                position: Position::Absolute,
                left: 29,
                top: 97,
                width: 410,
                height: 13,
                font_size: 10,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Button (
                "×",
                id: "pixel_modal_close",
                position: Position::Absolute,
                left: 428,
                top: 72,
                width: 26,
                height: 24,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                pressed_text_color: BOARD,
                text_color: TEXT,
                border_radius: 6
            ) on Tap { model.close_modal(); }
            Button (
                "",
                id: "pixel_template_card_0",
                visible: ${ model.modal() == PixelModal::Templates },
                position: Position::Absolute,
                left: 29,
                top: 112,
                width: 135,
                height: 104,
                size: ButtonSize::Custom,
                normal_color: Color::rgba(0, 0, 0, 0),
                pressed_color: Color::rgba(203, 179, 240, 48),
                border_radius: 7
            ) on Tap { model.load_template(0); }
            Button (
                "",
                id: "pixel_template_card_1",
                visible: ${ model.modal() == PixelModal::Templates },
                position: Position::Absolute,
                left: 171,
                top: 112,
                width: 135,
                height: 104,
                size: ButtonSize::Custom,
                normal_color: Color::rgba(0, 0, 0, 0),
                pressed_color: Color::rgba(203, 179, 240, 48),
                border_radius: 7
            ) on Tap { model.load_template(1); }
            Button (
                "",
                id: "pixel_template_card_2",
                visible: ${ model.modal() == PixelModal::Templates },
                position: Position::Absolute,
                left: 313,
                top: 112,
                width: 135,
                height: 104,
                size: ButtonSize::Custom,
                normal_color: Color::rgba(0, 0, 0, 0),
                pressed_color: Color::rgba(203, 179, 240, 48),
                border_radius: 7
            ) on Tap { model.load_template(2); }
            Button (
                TEMPLATE_NAMES[0],
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
                pressed_text_color: BOARD,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { model.load_template(0); }
            Button (
                TEMPLATE_NAMES[1],
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
                pressed_text_color: BOARD,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { model.load_template(1); }
            Button (
                TEMPLATE_NAMES[2],
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
                pressed_text_color: BOARD,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { model.load_template(2); }
            Text (
                tr!(en : "THIS FRAME", zh : "当前帧"),
                id: "pixel_clear_preview",
                visible: ${ model.modal() == PixelModal::Clear },
                position: Position::Absolute,
                left: 57,
                top: 212,
                width: 96,
                height: 13,
                font_size: 10,
                text_color: MUTED,
                paragraph: ParagraphStyle::label()
            )
            Button (
                tr!(en : "CLEAR", zh : "清空"),
                id: "pixel_clear_confirm",
                visible: ${ model.modal() == PixelModal::Clear },
                position: Position::Absolute,
                left: 219,
                top: 174,
                width: 211,
                height: 32,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: ACTIVE,
                pressed_color: ACTIVE,
                text_color: BACKGROUND,
                border_radius: 7
            ) on Tap { model.confirm_clear(); }
            Button (
                tr!(en : "CANCEL", zh : "取消"),
                id: "pixel_clear_cancel",
                visible: ${ model.modal() == PixelModal::Clear },
                position: Position::Absolute,
                left: 219,
                top: 214,
                width: 211,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                pressed_text_color: BOARD,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { model.close_modal(); }
        } on Tap { }
    }
}

#[compose]
pub(super) fn build_widgets(model: model!(PixelModel)) {
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
            Focusable,
            TouchAction::None,
        ] {
            View (
                id: "pixel_controls",
                position: Position::Absolute,
                left: COLUMN_LEFT,
                top: 0,
                width: COLUMN_WIDTH,
                height: 320,
                visible: ${ model.modal() == PixelModal::None }
            ) {
                compose_header (model)
                compose_preview (model)
                compose_frame_selector (model)
                compose_tool_controls (model)
                compose_palette (model)
                compose_edit_actions (model, id("pixel_surface"))
            }
            compose_modal (model)
        } on ClipboardFinished(|event: event!(ClipboardFinished)| {
            model
                .set_clipboard_status(
                    match event.result {
                        Ok(ClipboardOutcome::Copied) => PixelClipboardStatus::Copied,
                        Ok(ClipboardOutcome::Cut) => PixelClipboardStatus::Cut,
                        Ok(ClipboardOutcome::Pasted) => PixelClipboardStatus::Pasted,
                        Ok(ClipboardOutcome::CopiedWithoutCut) => {
                            PixelClipboardStatus::Changed
                        }
                        Ok(ClipboardOutcome::EmptySelection) => PixelClipboardStatus::Ready,
                        Err(_) => PixelClipboardStatus::Unavailable,
                    },
                );
        }) on Tap(|event: event!(Tap), rows: query!(PixelSurface, ComputedRect)| {
            let Some((surface, rect)) = rows.get(event.target) else {
                return false;
            };
            let rect = rect.0;
            surface_tap(surface, rect, event.x, event.y)
        }) on DragStart(|event: event!(DragStart), rows: query!(PixelSurface, ComputedRect)| {
            let Some((surface, rect)) = rows.get(event.target) else {
                return false;
            };
            let rect = rect.0;
            surface_drag_start(surface, rect, event.x, event.y)
        }) on DragMove(|event: event!(DragMove), rows: query!(PixelSurface, ComputedRect)| {
            let Some((surface, rect)) = rows.get(event.target) else {
                return false;
            };
            let rect = rect.0;
            surface_drag_move(surface, rect, event.x, event.y)
        }) on DragEnd(|event: event!(DragEnd), rows: query!(PixelSurface)| {
            let Some(surface) = rows.get(event.target) else {
                return false;
            };
            surface_drag_end(surface, false)
        }) on DragCancel(|event: event!(DragCancel), rows: query!(PixelSurface)| {
            let Some(surface) = rows.get(event.target) else {
                return false;
            };
            surface_drag_end(surface, true)
        })
    };
}

pub(super) fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_plugin(GameFontPlugin);
    app.with_widget(surface_render::view())
        .with_widget(preview_render::view())
        .with_widget(modal_render::view());
    #[cfg(feature = "std")]
    app.add_plugin(StdInstantClockPlugin);
    #[cfg(feature = "persistence")]
    install_persistence(app);
    let model = app.add_model(PixelModel::default()).finish();
    app.on_event(Key::EVENT, keyboard::callback(model.clone()));
    app.add_system(pixel_tick_system::system(model.clone()));
    app.compose(parent, build_widgets(model));
}
