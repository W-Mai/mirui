use super::input::{moss_tick_system, surface_gesture};
use super::render::{modal_view, surface_view};
use super::state::{MossModalSurface, MossNodes, MossSurface};
use super::style::{ACTIVE, BACKGROUND, CONTROL, MUTED, TEXT};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::gallery::play::font::register_play_font;
use crate::gallery::play::moss::{MossModel, MossTool};
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};

#[compose]
pub(super) fn build_widgets() {
    ui! {
        MossSurface (id: "moss_surface", width: 480, height: 320, clip_children: true) [
            TouchAction::None,
        ] on Tap { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragStart { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragMove { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragEnd { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragCancel { surface_gesture(ctx.world, ctx.entity, ctx.event); }
        {
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
            Text (
                "B3 / S23 · PAUSE",
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
                normal_color: ACTIVE,
                pressed_color: ACTIVE,
                text_color: BACKGROUND,
                border_radius: 6
            ) on Tap { MossNodes::update(ctx.world, |model| model.set_tool(MossTool::Plant)); }
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
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 6
            ) on Tap { MossNodes::update(ctx.world, |model| model.set_tool(MossTool::Erase)); }
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
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 6
            ) on Tap { MossNodes::update(ctx.world, |model| model.set_tool(MossTool::Glider)); }
            Button (
                "0 度旋转",
                id: "moss_rotate",
                position: Position::Absolute,
                left: 235,
                top: 41,
                width: 119,
                height: 22,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: MUTED,
                border_radius: 6
            ) on Tap { MossNodes::update(ctx.world, MossModel::rotate_glider); }
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
            Text (
                "000",
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
            Text (
                "0",
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
            Button (
                "4 代/秒 ↻",
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
            ) on Tap { MossNodes::update(ctx.world, MossModel::cycle_rate); }
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
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { MossNodes::update(ctx.world, MossModel::undo); }
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
            Button (
                "运行",
                id: "moss_run",
                position: Position::Absolute,
                left: 12,
                top: 288,
                width: 109,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACTIVE,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { MossNodes::update(ctx.world, MossModel::toggle_running); }
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
            ) on Tap { MossNodes::update(ctx.world, MossModel::step); }
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
            ) on Tap { MossNodes::update(ctx.world, MossModel::open_seeds); }
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
            ) on Tap { MossNodes::update(ctx.world, MossModel::open_clear); }
            MossModalSurface (
                id: "moss_modal",
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
                    "给花园一种新的开始",
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
                    "载入会暂停演化；新种子可以撤销。",
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
                ) on Tap { MossNodes::update(ctx.world, MossModel::close_modal); }
                Button (
                    "漂流花园",
                    id: "moss_seed_0",
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
                ) on Tap { MossNodes::update(ctx.world, |model| model.load_seed(0)); }
                Button (
                    "双生脉冲",
                    id: "moss_seed_1",
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
                ) on Tap { MossNodes::update(ctx.world, |model| model.load_seed(1)); }
                Button (
                    "固定随机种子",
                    id: "moss_seed_2",
                    position: Position::Absolute,
                    left: 313,
                    top: 224,
                    width: 135,
                    height: 29,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: CONTROL,
                    pressed_color: ACTIVE,
                    text_color: TEXT,
                    border_radius: 7
                ) on Tap { MossNodes::update(ctx.world, |model| model.load_seed(2)); }
                Text (
                    "清空后可以重新播种，也可以撤销。",
                    id: "moss_clear_note",
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
                ) on Tap { MossNodes::update(ctx.world, MossModel::confirm_clear); }
                Button (
                    "保留花园",
                    id: "moss_clear_cancel",
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
                ) on Tap { MossNodes::update(ctx.world, MossModel::close_modal); }
                Text (
                    "可撤销",
                    id: "moss_clear_badge",
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
    app.world.insert_resource(MossModel::default());
    app.with_widget(surface_view()).with_widget(modal_view());
    app.add_system(moss_tick_system::system());
    app.compose(parent, build_widgets);
    let find = |id| app.world.find_by_id(id).expect("Moss Study node");
    let nodes = MossNodes {
        surface: find("moss_surface"),
        status: find("moss_status"),
        generation: find("moss_generation"),
        live: find("moss_live"),
        rate: find("moss_rate"),
        undo: find("moss_undo"),
        run: find("moss_run"),
        rotate: find("moss_rotate"),
        tools: [
            find("moss_tool_plant"),
            find("moss_tool_erase"),
            find("moss_tool_glider"),
        ],
        modal: find("moss_modal"),
        modal_title: find("moss_modal_title"),
        modal_subtitle: find("moss_modal_subtitle"),
        seed_buttons: [
            find("moss_seed_0"),
            find("moss_seed_1"),
            find("moss_seed_2"),
        ],
        clear_controls: [
            find("moss_clear_note"),
            find("moss_clear_confirm"),
            find("moss_clear_cancel"),
            find("moss_clear_badge"),
        ],
    };
    app.world.insert_resource(nodes);
    MossNodes::sync(&mut app.world);
}
