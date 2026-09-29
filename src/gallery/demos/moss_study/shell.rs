use super::input::{moss_tick_system, surface_gesture};
use super::render::{modal_view, surface_view};
use super::state::{MossModalSurface, MossSurface};
use super::style::{ACTIVE, BACKGROUND, CONTROL, MUTED, TEXT};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::gallery::play::font::register_play_font;
use crate::gallery::play::moss::{MossModal, MossModel, MossTool};
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};
use core::fmt;

enum ModalSubtitle {
    Hidden,
    Seeds,
    Clear(u16),
}

impl fmt::Display for ModalSubtitle {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hidden => Ok(()),
            Self::Seeds => out.write_str("载入会暂停演化；新种子可以撤销。"),
            Self::Clear(live) => write!(out, "当前有 {live} 个活细胞；清空后代数归零。"),
        }
    }
}

fn control_color(active: bool, enabled: bool) -> Color {
    if active {
        ACTIVE
    } else if enabled {
        CONTROL
    } else {
        Color::rgb(40, 51, 41)
    }
}

fn control_text_color(active: bool, enabled: bool) -> Color {
    if active {
        BACKGROUND
    } else if enabled {
        TEXT
    } else {
        Color::rgb(105, 119, 100)
    }
}

#[compose(bind(model))]
fn compose_header_tools(model: MossModel) -> Entity {
    ui! {
            Text (
                "MOSS STUDY",
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
                text: ${ if model.running() { "B3 / S23 · RUN" } else { "B3 / S23 · PAUSE" } },
                text_capacity: 17,
                id: "moss_status",
                position: Position::Absolute,
                left: 300,
                top: 8,
                width: 160,
                height: 20,
                font_size: 9,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
    };
    ui! {
            Button (
                "播种",
                id: "moss_tool_plant",
                position: Position::Absolute,
                left: 13,
                top: 41,
                width: 64,
                height: 22,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ${ control_color(model.tool() == MossTool::Plant, true) },
                pressed_color: ACTIVE,
                text_color: ${ control_text_color(model.tool() == MossTool::Plant, true) },
                border_radius: 6
            ) on Tap { model.set_tool(MossTool::Plant); }
    };
    ui! {
            Button (
                "擦除",
                id: "moss_tool_erase",
                position: Position::Absolute,
                left: 82,
                top: 41,
                width: 64,
                height: 22,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ${ control_color(model.tool() == MossTool::Erase, true) },
                pressed_color: ACTIVE,
                text_color: ${ control_text_color(model.tool() == MossTool::Erase, true) },
                border_radius: 6
            ) on Tap { model.set_tool(MossTool::Erase); }
    };
    ui! {
            Button (
                "滑翔机",
                id: "moss_tool_glider",
                position: Position::Absolute,
                left: 151,
                top: 41,
                width: 79,
                height: 22,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ${ control_color(model.tool() == MossTool::Glider, true) },
                pressed_color: ACTIVE,
                text_color: ${ control_text_color(model.tool() == MossTool::Glider, true) },
                border_radius: 6
            ) on Tap { model.set_tool(MossTool::Glider); }
    };
    ui! {
            Button (
                text: ${ format_args!("{} 度旋转", model.rotation() * 90) },
                text_capacity: 13,
                id: "moss_rotate",
                position: Position::Absolute,
                left: 235,
                top: 41,
                width: 119,
                height: 22,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ${ control_color(false, model.tool() == MossTool::Glider) },
                pressed_color: ACTIVE,
                text_color: ${ control_text_color(false, model.tool() == MossTool::Glider) },
                border_radius: 6
            ) on Tap { model.rotate_glider(); }
    }
}

#[compose(bind(model))]
fn compose_metrics(model: MossModel) -> Entity {
    ui! {
            Text (
                "GENERATION",
                position: Position::Absolute,
                left: 382,
                top: 52,
                width: 74,
                height: 13,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
    };
    ui! {
            Text (
                text: ${ format_args!("{:03}", model.generation()) },
                text_capacity: 10,
                id: "moss_generation",
                position: Position::Absolute,
                left: 382,
                top: 64,
                width: 74,
                height: 30,
                font_size: 24,
                text_color: ACTIVE,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
    };
    ui! {
            Text (
                "活细胞",
                position: Position::Absolute,
                left: 382,
                top: 105,
                width: 45,
                height: 16,
                font_size: 9,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
    };
    ui! {
            Text (
                text: ${ format_args!("{}", model.live_count()) },
                text_capacity: 3,
                id: "moss_live",
                position: Position::Absolute,
                left: 425,
                top: 102,
                width: 31,
                height: 22,
                font_size: 17,
                text_color: Color::rgb(213, 233, 192),
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
    };
    ui! {
            Button (
                text: ${ format_args!("{} 代/秒 ↻", model.rate()) },
                text_capacity: 13,
                id: "moss_rate",
                position: Position::Absolute,
                left: 372,
                top: 168,
                width: 96,
                height: 29,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { model.cycle_rate(); }
    };
    ui! {
            Button (
                "撤销",
                id: "moss_undo",
                position: Position::Absolute,
                left: 372,
                top: 204,
                width: 96,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: ${ control_color(false, model.can_undo() && model.modal() == MossModal::None) },
                pressed_color: ACTIVE,
                text_color: ${ control_text_color(false, model.can_undo() && model.modal() == MossModal::None) },
                border_radius: 7
            ) on Tap { model.undo(); }
    };
    ui! {
            Text (
                "边缘之外为空",
                position: Position::Absolute,
                left: 371,
                top: 242,
                width: 97,
                height: 15,
                font_size: 8,
                text_color: MUTED,
                paragraph: ParagraphStyle::label()
            )
    };
    ui! {
            Text (
                "生命 ≠ 生物模拟",
                position: Position::Absolute,
                left: 371,
                top: 257,
                width: 97,
                height: 13,
                font_size: 7,
                text_color: Color::rgb(128, 153, 115),
                paragraph: ParagraphStyle::label()
            )
    }
}

#[compose(bind(model))]
fn compose_footer(model: MossModel) -> Entity {
    ui! {
            Button (
                text: ${ if model.running() { "暂停" } else { "运行" } },
                text_capacity: 6,
                id: "moss_run",
                position: Position::Absolute,
                left: 12,
                top: 288,
                width: 109,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: ${ control_color(model.running(), model.modal() == MossModal::None) },
                pressed_color: ACTIVE,
                text_color: ${ control_text_color(model.running(), model.modal() == MossModal::None) },
                border_radius: 7
            ) on Tap { model.toggle_running(); }
    };
    ui! {
            Button (
                "单步",
                position: Position::Absolute,
                left: 127,
                top: 288,
                width: 109,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { model.step(); }
    };
    ui! {
            Button (
                "种子",
                position: Position::Absolute,
                left: 242,
                top: 288,
                width: 109,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { model.open_seeds(); }
    };
    ui! {
            Button (
                "清空",
                position: Position::Absolute,
                left: 357,
                top: 288,
                width: 111,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { model.open_clear(); }
    }
}

#[compose(bind(model))]
fn compose_modal(model: MossModel) -> Entity {
    ui! {
            View (
                id: "moss_modal",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                clip_children: true,
                visible: ${ model.modal() != MossModal::None }
            ) [
                MossModalSurface {
                    model: model.clone(),
                },
                TouchAction::None,
            ] on Tap { }
            {
                Text (
                    text: ${ if model.modal() == MossModal::Seeds { "给花园一种新的开始" } else { "让花园重新开始？" } },
                    text_capacity: 27,
                    id: "moss_modal_title",
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
                    text: ${ match model.modal() {
                        MossModal::None => ModalSubtitle::Hidden,
                        MossModal::Seeds => ModalSubtitle::Seeds,
                        MossModal::Clear => ModalSubtitle::Clear(model.live_count()),
                    } },
                    text_capacity: 53,
                    id: "moss_modal_subtitle",
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
                    "漂流花园",
                    id: "moss_seed_0",
                    visible: ${ model.modal() == MossModal::Seeds },
                    position: Position::Absolute,
                    left: 29,
                    top: 224,
                    width: 135,
                    height: 29,
                    size: ButtonSize::Compact,
                    font_size: 10,
                    normal_color: ${ control_color(model.seed_id() == 0, model.modal() == MossModal::Seeds) },
                    pressed_color: ACTIVE,
                    text_color: ${ control_text_color(model.seed_id() == 0, model.modal() == MossModal::Seeds) },
                    border_radius: 7
                ) on Tap { model.load_seed(0); }
                Button (
                    "双生脉冲",
                    id: "moss_seed_1",
                    visible: ${ model.modal() == MossModal::Seeds },
                    position: Position::Absolute,
                    left: 171,
                    top: 224,
                    width: 135,
                    height: 29,
                    size: ButtonSize::Compact,
                    font_size: 10,
                    normal_color: ${ control_color(model.seed_id() == 1, model.modal() == MossModal::Seeds) },
                    pressed_color: ACTIVE,
                    text_color: ${ control_text_color(model.seed_id() == 1, model.modal() == MossModal::Seeds) },
                    border_radius: 7
                ) on Tap { model.load_seed(1); }
                Button (
                    "固定随机种子",
                    id: "moss_seed_2",
                    visible: ${ model.modal() == MossModal::Seeds },
                    position: Position::Absolute,
                    left: 313,
                    top: 224,
                    width: 135,
                    height: 29,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: ${ control_color(model.seed_id() == 2, model.modal() == MossModal::Seeds) },
                    pressed_color: ACTIVE,
                    text_color: ${ control_text_color(model.seed_id() == 2, model.modal() == MossModal::Seeds) },
                    border_radius: 7
                ) on Tap { model.load_seed(2); }
                Text (
                    "清空后可以重新播种，也可以撤销。",
                    id: "moss_clear_note",
                    visible: ${ model.modal() == MossModal::Clear },
                    position: Position::Absolute,
                    left: 171,
                    top: 139,
                    width: 257,
                    height: 18,
                    font_size: 9,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Button (
                    "清空花园",
                    id: "moss_clear_confirm",
                    visible: ${ model.modal() == MossModal::Clear },
                    position: Position::Absolute,
                    left: 171,
                    top: 190,
                    width: 257,
                    height: 32,
                    size: ButtonSize::Compact,
                    font_size: 11,
                    normal_color: ACTIVE,
                    pressed_color: ACTIVE,
                    text_color: BACKGROUND,
                    border_radius: 7
                ) on Tap { model.confirm_clear(); }
                Button (
                    "保留花园",
                    id: "moss_clear_cancel",
                    visible: ${ model.modal() == MossModal::Clear },
                    position: Position::Absolute,
                    left: 171,
                    top: 230,
                    width: 257,
                    height: 25,
                    size: ButtonSize::Compact,
                    font_size: 10,
                    normal_color: CONTROL,
                    pressed_color: ACTIVE,
                    text_color: TEXT,
                    border_radius: 7
                ) on Tap { model.close_modal(); }
                Text (
                    "可撤销",
                    id: "moss_clear_badge",
                    visible: ${ model.modal() == MossModal::Clear },
                    position: Position::Absolute,
                    left: 72,
                    top: 209,
                    width: 54,
                    height: 16,
                    font_size: 8,
                    text_color: ACTIVE,
                    paragraph: ParagraphStyle::label()
                )
            }
    }
}

#[compose(bind(model))]
pub(super) fn build_widgets(model: MossModel) {
    ui! {
        View (id: "moss_surface", width: 480, height: 320, clip_children: true) [
            MossSurface {
                model: model.clone(),
            },
            TouchAction::None,
        ] on Tap { surface_gesture(&ctx); } on DragStart { surface_gesture(&ctx); } on DragMove { surface_gesture(&ctx); } on DragEnd { surface_gesture(&ctx); } on DragCancel { surface_gesture(&ctx); }
        {
            compose_header_tools (model)
            compose_metrics (model)
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
    #[cfg(feature = "std")]
    app.add_plugin(StdInstantClockPlugin);
    register_play_font(&mut app.world);
    app.with_widget(surface_view()).with_widget(modal_view());
    let model = app.add_model(MossModel::default());
    app.add_system(moss_tick_system::system(model.clone()));
    app.compose(parent, |cx| build_widgets(cx, model));
}
