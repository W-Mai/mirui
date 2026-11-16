use super::input::{
    keyboard, moss_tick_system, surface_drag_end, surface_drag_move, surface_drag_start,
    surface_tap,
};
#[cfg(feature = "persistence")]
use super::persistence::install_persistence;
use super::render::{SEED_CARD_LEFT, SEED_CARD_TOP, modal_frame, modal_render, surface_render};
use super::state::{MossModalSurface, MossSurface};
use super::style::{ACTIVE, BACKGROUND, BOARD, CONTROL, MUTED, TEXT};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::core::i18n::{Locale, locale};
use crate::gallery::play::font::PlayFontPlugin;
use crate::gallery::play::moss::{MossModal, MossModel, MossTool};
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::tr;
use crate::ui::ComputedRect;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};
use core::fmt;

/// The glider tool shows the way its next glider will fly.
const GLIDER_ARROWS: [&str; 4] = ["↘", "↙", "↖", "↗"];

fn glider_arrow(rotation: u8) -> &'static str {
    GLIDER_ARROWS[usize::from(rotation % 4)]
}

struct Generation(u32);

impl fmt::Display for Generation {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match locale() {
            Locale::ZhCn => write!(out, "第 {:03} 代", self.0),
            _ => write!(out, "Gen {:03}", self.0),
        }
    }
}

struct LiveCount(u16);

impl fmt::Display for LiveCount {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match locale() {
            Locale::ZhCn => write!(out, "存活 {}", self.0),
            _ => write!(out, "{} live", self.0),
        }
    }
}

/// Generations a second.
struct Rate(u8);

impl fmt::Display for Rate {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match locale() {
            Locale::ZhCn => write!(out, "{}代/秒", self.0),
            _ => write!(out, "{}/s", self.0),
        }
    }
}

enum ModalSubtitle {
    Hidden,
    Seeds,
    Clear(u16),
}

impl fmt::Display for ModalSubtitle {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self, locale()) {
            (Self::Hidden, _) => Ok(()),
            (Self::Seeds, _) => write!(
                out,
                "{}",
                tr!(
                    en: "Loads paused. Undo takes it back.",
                    zh: "载入后先暂停，可以撤销。"
                )
            ),
            (Self::Clear(live), Locale::ZhCn) => {
                write!(out, "{live} 个活细胞会被清除，代数归零。")
            }
            (Self::Clear(live), _) => write!(out, "{live} live cells go; generation back to 0."),
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

fn modal_top(modal: MossModal) -> i32 {
    modal_frame(modal).y.to_int()
}

#[compose]
fn compose_header(model: model!(MossModel)) -> Entity {
    ui! {
        Text (
            tr!(en : "MOSS STUDY", zh : "苔藓习作"),
            id: "moss_title",
            position: Position::Absolute,
            left: 32,
            top: 6,
            width: 62,
            height: 20,
            font_size: 10,
            text_color: TEXT,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            text: ${ Generation(model.generation()) },
            capacity: 20,
            id: "moss_generation",
            position: Position::Absolute,
            left: 100,
            top: 6,
            width: 52,
            height: 20,
            font_size: 10,
            text_color: ACTIVE,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            text: ${ LiveCount(model.live_count()) },
            capacity: 16,
            id: "moss_live",
            position: Position::Absolute,
            left: 154,
            top: 6,
            width: 44,
            height: 20,
            font_size: 10,
            text_color: MUTED,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Button (
            "●",
            id: "moss_tool_plant",
            position: Position::Absolute,
            left: 202,
            top: 2,
            width: 28,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(model.tool() == MossTool::Plant, true) },
            pressed_color: ACTIVE,
            text_color: ${ control_text_color(model.tool() == MossTool::Plant, true) },
            border_radius: 7
        ) on Tap { model.set_tool(MossTool::Plant); }
    };
    ui! {
        Button (
            "○",
            id: "moss_tool_erase",
            position: Position::Absolute,
            left: 233,
            top: 2,
            width: 28,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(model.tool() == MossTool::Erase, true) },
            pressed_color: ACTIVE,
            text_color: ${ control_text_color(model.tool() == MossTool::Erase, true) },
            border_radius: 7
        ) on Tap { model.set_tool(MossTool::Erase); }
    };
    ui! {
        Button (
            text: ${ glider_arrow(model.rotation()) },
            capacity: 4,
            id: "moss_tool_glider",
            position: Position::Absolute,
            left: 264,
            top: 2,
            width: 28,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(model.tool() == MossTool::Glider, true) },
            pressed_color: ACTIVE,
            text_color: ${ control_text_color(model.tool() == MossTool::Glider, true) },
            border_radius: 7
        ) on Tap { model.pick_glider(); }
    };
    ui! {
        Button (
            text: ${ if model.running() { "Ⅱ" } else { "▶" } },
            capacity: 4,
            id: "moss_run",
            position: Position::Absolute,
            left: 298,
            top: 2,
            width: 28,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(model.running(), true) },
            pressed_color: ACTIVE,
            text_color: ${ control_text_color(model.running(), true) },
            border_radius: 7
        ) on Tap { model.toggle_running(); }
    };
    ui! {
        Button (
            "+1",
            id: "moss_step",
            position: Position::Absolute,
            left: 329,
            top: 2,
            width: 28,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: CONTROL,
            pressed_color: ACTIVE,
            pressed_text_color: BOARD,
            text_color: TEXT,
            border_radius: 7
        ) on Tap { model.step(); }
    };
    ui! {
        Button (
            text: ${ Rate(model.rate()) },
            capacity: 16,
            id: "moss_rate",
            position: Position::Absolute,
            left: 360,
            top: 2,
            width: 44,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: CONTROL,
            pressed_color: ACTIVE,
            pressed_text_color: BOARD,
            text_color: TEXT,
            border_radius: 7
        ) on Tap { model.cycle_rate(); }
    };
    ui! {
        Button (
            "↺",
            id: "moss_undo",
            position: Position::Absolute,
            left: 410,
            top: 2,
            width: 28,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(false, model.can_undo()) },
            pressed_color: ACTIVE,
            text_color: ${ control_text_color(false, model.can_undo()) },
            border_radius: 7
        ) on Tap { model.undo(); }
    };
    ui! {
        Button (
            "☰",
            id: "moss_menu",
            position: Position::Absolute,
            left: 441,
            top: 2,
            width: 28,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: CONTROL,
            pressed_color: ACTIVE,
            pressed_text_color: BOARD,
            text_color: TEXT,
            border_radius: 7
        ) on Tap { model.open_seeds(); }
    }
}

#[compose]
fn compose_seeds(model: model!(MossModel)) -> Entity {
    ui! {
        View (
            id: "moss_seeds",
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320,
            visible: ${ model.modal() == MossModal::Seeds }
        ) {
            Button (
                "",
                id: "moss_seed_card_0",
                position: Position::Absolute,
                left: SEED_CARD_LEFT[0],
                top: SEED_CARD_TOP,
                width: 135,
                height: 96,
                size: ButtonSize::Custom,
                normal_color: Color::rgba(0, 0, 0, 0),
                pressed_color: Color::rgba(208, 236, 157, 40),
                border_radius: 7
            ) on Tap { model.load_seed(0); }
            Button (
                "",
                id: "moss_seed_card_1",
                position: Position::Absolute,
                left: SEED_CARD_LEFT[1],
                top: SEED_CARD_TOP,
                width: 135,
                height: 96,
                size: ButtonSize::Custom,
                normal_color: Color::rgba(0, 0, 0, 0),
                pressed_color: Color::rgba(208, 236, 157, 40),
                border_radius: 7
            ) on Tap { model.load_seed(1); }
            Button (
                "",
                id: "moss_seed_card_2",
                position: Position::Absolute,
                left: SEED_CARD_LEFT[2],
                top: SEED_CARD_TOP,
                width: 135,
                height: 96,
                size: ButtonSize::Custom,
                normal_color: Color::rgba(0, 0, 0, 0),
                pressed_color: Color::rgba(208, 236, 157, 40),
                border_radius: 7
            ) on Tap { model.load_seed(2); }
            Button (
                id: "moss_seed_0",
                position: Position::Absolute,
                left: SEED_CARD_LEFT[0],
                top: SEED_CARD_TOP + 102,
                width: 135,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: ${ control_color(model.seed_id() == 0, true) },
                pressed_color: ACTIVE,
                text_color: ${ control_text_color(model.seed_id() == 0, true) },
                border_radius: 7
            ) [
                Text::label(tr!(en : "Drifting garden", zh : "漂流花园")),
            ] on Tap { model.load_seed(0); }
            Button (
                id: "moss_seed_1",
                position: Position::Absolute,
                left: SEED_CARD_LEFT[1],
                top: SEED_CARD_TOP + 102,
                width: 135,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: ${ control_color(model.seed_id() == 1, true) },
                pressed_color: ACTIVE,
                text_color: ${ control_text_color(model.seed_id() == 1, true) },
                border_radius: 7
            ) [
                Text::label(tr!(en : "Twin pulse", zh : "双生脉冲")),
            ] on Tap { model.load_seed(1); }
            Button (
                id: "moss_seed_2",
                position: Position::Absolute,
                left: SEED_CARD_LEFT[2],
                top: SEED_CARD_TOP + 102,
                width: 135,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: ${ control_color(model.seed_id() == 2, true) },
                pressed_color: ACTIVE,
                text_color: ${ control_text_color(model.seed_id() == 2, true) },
                border_radius: 7
            ) [
                Text::label(tr!(en : "Random garden", zh : "随机花园")),
            ] on Tap { model.load_seed(2); }
            Text (
                tr!(
                    en :
                    "B3/S23: an empty cell with 3 neighbours grows; a cell with 2 or 3 lives on.", zh
                    :
                    "B3/S23：空格有三个邻居就长出，有两三个邻居的细胞存活。"
                ),
                position: Position::Absolute,
                left: 29,
                top: 226,
                width: 420,
                height: 14,
                font_size: 10,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                tr!(
                    en :
                    "Keys: Space run · N step · Z undo · 1 2 3 tools · R turn glider · Esc seeds",
                    zh :
                    "按键：空格 运行 · N 单步 · Z 撤销 · 1 2 3 工具 · R 转滑翔机 · Esc 种子"
                ),
                position: Position::Absolute,
                left: 29,
                top: 244,
                width: 420,
                height: 14,
                font_size: 10,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Button (
                id: "moss_clear",
                position: Position::Absolute,
                left: 29,
                top: 266,
                width: 140,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                pressed_text_color: BOARD,
                text_color: TEXT,
                border_radius: 7
            ) [
                Text::label(tr!(en : "Clear the garden", zh : "清空花园")),
            ] on Tap { model.open_clear(); }
        }
    }
}

#[compose]
fn compose_clear(model: model!(MossModel)) -> Entity {
    ui! {
        View (
            id: "moss_clear_panel",
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320,
            visible: ${ model.modal() == MossModal::Clear }
        ) {
            Button (
                id: "moss_clear_confirm",
                position: Position::Absolute,
                left: 171,
                top: 140,
                width: 257,
                height: 32,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: ACTIVE,
                pressed_color: ACTIVE,
                text_color: BACKGROUND,
                border_radius: 7
            ) [
                Text::label(tr!(en : "Clear", zh : "清空")),
            ] on Tap { model.confirm_clear(); }
            Button (
                id: "moss_clear_cancel",
                position: Position::Absolute,
                left: 171,
                top: 180,
                width: 257,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                pressed_text_color: BOARD,
                text_color: TEXT,
                border_radius: 7
            ) [
                Text::label(tr!(en : "Cancel", zh : "取消")),
            ] on Tap { model.close_modal(); }
            Text (
                tr!(en : "Undo works", zh : "可撤销"),
                id: "moss_clear_badge",
                position: Position::Absolute,
                left: 59,
                top: 209,
                width: 80,
                height: 16,
                font_size: 10,
                text_color: ACTIVE,
                paragraph: ParagraphStyle::label()
            )
        }
    }
}

#[compose]
fn compose_modal(model: model!(MossModel)) -> Entity {
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
        ] {
            Text (
                text: ${
                    if model.modal() == MossModal::Seeds {
                        tr!(en : "Seeds", zh : "选一个种子")
                    } else {
                        tr!(en : "Clear the garden?", zh : "清空花园？")
                    }
                },
                capacity: 27,
                id: "moss_modal_title",
                position: Position::Absolute,
                left: 29,
                top: ${ modal_top(model.modal()) + 10 },
                width: 390,
                height: 22,
                font_size: 10,
                text_color: ACTIVE,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                text: ${
                    match model.modal() {
                        MossModal::None => ModalSubtitle::Hidden,
                        MossModal::Seeds => ModalSubtitle::Seeds,
                        MossModal::Clear => ModalSubtitle::Clear(model.live_count()),
                    }
                },
                capacity: 64,
                id: "moss_modal_subtitle",
                position: Position::Absolute,
                left: 29,
                top: ${ modal_top(model.modal()) + 30 },
                width: 410,
                height: 15,
                font_size: 10,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Button (
                "×",
                id: "moss_modal_close",
                position: Position::Absolute,
                left: 428,
                top: ${ modal_top(model.modal()) + 8 },
                width: 24,
                height: 24,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                pressed_text_color: BOARD,
                text_color: TEXT,
                border_radius: 6
            ) on Tap { model.close_modal(); }
            compose_seeds (model)
            compose_clear (model)
        } on Tap { }
    }
}

#[compose]
pub(super) fn build_widgets(model: model!(MossModel)) {
    ui! {
        View (id: "moss_surface", width: 480, height: 320, clip_children: true) [
            MossSurface {
                model: model.clone(),
            },
            TouchAction::None,
        ] {
            View (
                id: "moss_page",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                visible: ${ model.modal() == MossModal::None }
            ) {
                compose_header (model)
            }
            compose_modal (model)
        } on Tap(|event: event!(Tap), rows: query!(MossSurface, ComputedRect)| {
            let Some((surface, rect)) = rows.get(event.target) else {
                return false;
            };
            let rect = rect.0;
            surface_tap(surface, rect, event.x, event.y)
        }) on DragStart(|event: event!(DragStart), rows: query!(MossSurface, ComputedRect)| {
            let Some((surface, rect)) = rows.get(event.target) else {
                return false;
            };
            let rect = rect.0;
            surface_drag_start(surface, rect, event.x, event.y)
        }) on DragMove(|event: event!(DragMove), rows: query!(MossSurface, ComputedRect)| {
            let Some((surface, rect)) = rows.get(event.target) else {
                return false;
            };
            let rect = rect.0;
            surface_drag_move(surface, rect, event.x, event.y)
        }) on DragEnd(|event: event!(DragEnd), rows: query!(MossSurface)| {
            let Some(surface) = rows.get(event.target) else {
                return false;
            };
            surface_drag_end(surface, false)
        }) on DragCancel(|event: event!(DragCancel), rows: query!(MossSurface)| {
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
    #[cfg(feature = "std")]
    app.add_plugin(StdInstantClockPlugin);
    app.add_plugin(PlayFontPlugin);
    #[cfg(feature = "persistence")]
    install_persistence(app);
    app.with_widget(surface_render::view())
        .with_widget(modal_render::view());
    let model = app.add_model(MossModel::default()).finish();
    app.on_event(Key::EVENT, keyboard::callback(model.clone()));
    app.add_system(moss_tick_system::system(model.clone()));
    app.compose(parent, build_widgets(model));
}
