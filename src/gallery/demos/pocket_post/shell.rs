use super::input::{PostKeyboardPlugin, post_tick_system, surface_gesture};
use super::render::{modal_view, surface_view};
use super::state::{PostModalSurface, PostSurface};
use super::style::{ACCENT, BACKGROUND, CONTROL, CONTROL_DISABLED, MUTED, STATION_COLORS, TEXT};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::gallery::play::font::PlayFontPlugin;
use crate::gallery::play::post::{PostModal, PostModel};
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};

fn status_label(finished: bool, running: bool, started: bool) -> &'static str {
    if finished {
        "班次完成"
    } else if running {
        "包裹正在路上"
    } else if started {
        "已暂停调度"
    } else {
        "准备好，拨动你的第一班轨道"
    }
}

fn run_label(finished: bool, running: bool, started: bool) -> &'static str {
    if running {
        "暂停"
    } else if finished {
        "再开一班"
    } else if started {
        "继续"
    } else {
        "开始"
    }
}

fn speed_label(speed_x2: u8) -> &'static str {
    match speed_x2 {
        1 => "0.5× 速度",
        3 => "1.5× 速度",
        _ => "1× 速度",
    }
}

fn destination_letter(destination: Option<u8>) -> &'static str {
    match destination {
        Some(0) => "A",
        Some(1) => "B",
        Some(_) => "C",
        None => "",
    }
}

fn destination_color(destination: Option<u8>) -> Color {
    destination
        .map(|station| STATION_COLORS[usize::from(station)])
        .unwrap_or(TEXT)
}

fn control_color(active: bool, enabled: bool) -> Color {
    if active {
        ACCENT
    } else if enabled {
        CONTROL
    } else {
        CONTROL_DISABLED
    }
}

fn control_text_color(active: bool, enabled: bool) -> Color {
    if active {
        BACKGROUND
    } else if enabled {
        TEXT
    } else {
        Color::rgb(103, 120, 111)
    }
}

fn modal_title(modal: PostModal) -> &'static str {
    match modal {
        PostModal::Manifests => "选择今天的班次",
        PostModal::Summary => "本班投递完成",
        PostModal::Reset => "重新开始这一班？",
        PostModal::None => "",
    }
}

fn modal_subtitle(modal: PostModal) -> &'static str {
    match modal {
        PostModal::Manifests => "更换班次会从头开始；没有时间惩罚，可以随时暂停。",
        PostModal::Summary => "不需要抢时间，准确到达就很棒。",
        PostModal::Reset => "当前包裹、计分和道岔都会复位。",
        PostModal::None => "",
    }
}

#[compose(bind(model))]
fn compose_header(model: PostModel) -> Entity {
    ui! {
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
    };
    ui! {
        Text (
            text: ${ format_args!("SORTED {} / {}", model.delivered(), model.manifest_len()) },
            text_capacity: 14,
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
    };
    ui! {
        Text (
            text: ${ status_label(model.finished(), model.running(), model.started()) },
            text_capacity: 39,
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
    };
    ui! {
        Text (
            text: ${ format_args!("错投 {}   连对 {}", model.missed(), model.streak()) },
            text_capacity: 23,
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
    }
}

#[compose(bind(model))]
fn compose_route_summary(model: PostModel) -> Entity {
    ui! {
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
    };
    ui! {
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
    };
    ui! {
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
    };
    ui! {
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
    };
    ui! {
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
    };
    ui! {
        Text (
            text: ${ format_args!("{}", model.station_a_count()) },
            text_capacity: 3,
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
    };
    ui! {
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
    };
    ui! {
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
    };
    ui! {
        Text (
            text: ${ format_args!("{}", model.station_b_count()) },
            text_capacity: 3,
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
    };
    ui! {
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
    };
    ui! {
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
    };
    ui! {
        Text (
            text: ${ format_args!("{}", model.station_c_count()) },
            text_capacity: 3,
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
    }
}

#[compose(bind(model))]
fn compose_dispatch_queue(model: PostModel) -> Entity {
    ui! {
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
    };
    ui! {
        Text (
            text: ${ destination_letter(model.queued_slots()[0]) },
            text_capacity: 1,
            id: "post_queue_0",
            visible: ${ model.queued_slots()[0].is_some() },
            position: Position::Absolute,
            left: 47,
            top: 241,
            width: 17,
            height: 15,
            font_size: 8,
            text_color: ${ destination_color(model.queued_slots()[0]) },
            paragraph: ParagraphStyle::label()
        )
    };
    ui! {
        Text (
            text: ${ destination_letter(model.queued_slots()[1]) },
            text_capacity: 1,
            id: "post_queue_1",
            visible: ${ model.queued_slots()[1].is_some() },
            position: Position::Absolute,
            left: 72,
            top: 241,
            width: 17,
            height: 15,
            font_size: 8,
            text_color: ${ destination_color(model.queued_slots()[1]) },
            paragraph: ParagraphStyle::label()
        )
    };
    ui! {
        Text (
            text: ${ destination_letter(model.queued_slots()[2]) },
            text_capacity: 1,
            id: "post_queue_2",
            visible: ${ model.queued_slots()[2].is_some() },
            position: Position::Absolute,
            left: 97,
            top: 241,
            width: 17,
            height: 15,
            font_size: 8,
            text_color: ${ destination_color(model.queued_slots()[2]) },
            paragraph: ParagraphStyle::label()
        )
    };
    ui! {
        Text (
            text: ${ destination_letter(model.queued_slots()[3]) },
            text_capacity: 1,
            id: "post_queue_3",
            visible: ${ model.queued_slots()[3].is_some() },
            position: Position::Absolute,
            left: 122,
            top: 241,
            width: 17,
            height: 15,
            font_size: 8,
            text_color: ${ destination_color(model.queued_slots()[3]) },
            paragraph: ParagraphStyle::label()
        )
    };
    ui! {
        Text (
            text: ${ destination_letter(model.queued_slots()[4]) },
            text_capacity: 1,
            id: "post_queue_4",
            visible: ${ model.queued_slots()[4].is_some() },
            position: Position::Absolute,
            left: 147,
            top: 241,
            width: 17,
            height: 15,
            font_size: 8,
            text_color: ${ destination_color(model.queued_slots()[4]) },
            paragraph: ParagraphStyle::label()
        )
    };
    ui! {
        Text (
            text: ${ format_args!("{} / 3 在途", model.active_len()) },
            text_capacity: 12,
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
    };
    ui! {
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
        ) on Tap { model.open_manifests(); }
    }
}

#[compose(bind(model))]
fn compose_footer(model: PostModel) -> Entity {
    ui! {
        Button (
            text: ${ run_label(model.finished(), model.running(), model.started()) },
            text_capacity: 12,
            id: "post_run",
            position: Position::Absolute,
            left: 12,
            top: 288,
            width: 129,
            height: 26,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(model.running() || !model.started(), model.modal() == PostModal::None) },
            pressed_color: ACCENT,
            text_color: ${ control_text_color(model.running() || !model.started(), model.modal() == PostModal::None) },
            border_radius: 7
        ) on Tap { model.toggle_running(); }
    };
    ui! {
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
            normal_color: ${ control_color(false, model.can_send()) },
            pressed_color: ACCENT,
            text_color: ${ control_text_color(false, model.can_send()) },
            border_radius: 7
        ) on Tap { model.spawn_manual(); }
    };
    ui! {
        Button (
            text: ${ speed_label(model.speed_x2()) },
            text_capacity: 12,
            id: "post_speed",
            position: Position::Absolute,
            left: 268,
            top: 288,
            width: 95,
            height: 26,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(false, model.modal() == PostModal::None) },
            pressed_color: ACCENT,
            text_color: ${ control_text_color(false, model.modal() == PostModal::None) },
            border_radius: 7
        ) on Tap { model.cycle_speed(); }
    };
    ui! {
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
        ) on Tap { model.open_reset(); }
    }
}

#[compose(bind(model))]
pub(super) fn build_widgets(model: PostModel) {
    ui! {
        View (id: "post_surface", width: 480, height: 320, clip_children: true) [
            PostSurface {
                model: model.clone(),
            },
            TouchAction::None,
        ] on Tap { surface_gesture(&ctx); }
        {
            compose_header (model)
            compose_route_summary (model)
            compose_dispatch_queue (model)
            compose_footer (model)
            compose_modal (model)
        }
    };
}

#[compose(bind(model))]
fn compose_modal(model: PostModel) -> Entity {
    ui! {
        View (
                id: "post_modal",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                clip_children: true,
                visible: ${ model.modal() != PostModal::None }
            ) [
                PostModalSurface {
                    model: model.clone(),
                },
                TouchAction::None,
            ] on Tap { }
            {
                Text (
                    text: ${ modal_title(model.modal()) },
                    text_capacity: 24,
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
                    text: ${ modal_subtitle(model.modal()) },
                    text_capacity: 72,
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
                ) on Tap { model.close_modal(); }
                Button (
                    "晨间邮路 · 9 件",
                    id: "post_manifest_0",
                    visible: ${ model.modal() == PostModal::Manifests },
                    position: Position::Absolute,
                    left: 29,
                    top: 113,
                    width: 420,
                    height: 37,
                    size: ButtonSize::Compact,
                    font_size: 11,
                    normal_color: ${ control_color(model.manifest_id() == 0, model.modal() == PostModal::Manifests) },
                    pressed_color: ACCENT,
                    text_color: ${ control_text_color(model.manifest_id() == 0, model.modal() == PostModal::Manifests) },
                    border_radius: 7
                ) on Tap { model.load_manifest(0); }
                Button (
                    "忙碌午后 · 12 件",
                    id: "post_manifest_1",
                    visible: ${ model.modal() == PostModal::Manifests },
                    position: Position::Absolute,
                    left: 29,
                    top: 160,
                    width: 420,
                    height: 37,
                    size: ButtonSize::Compact,
                    font_size: 11,
                    normal_color: ${ control_color(model.manifest_id() == 1, model.modal() == PostModal::Manifests) },
                    pressed_color: ACCENT,
                    text_color: ${ control_text_color(model.manifest_id() == 1, model.modal() == PostModal::Manifests) },
                    border_radius: 7
                ) on Tap { model.load_manifest(1); }
                Button (
                    "慢慢练习 · 6 件",
                    id: "post_manifest_2",
                    visible: ${ model.modal() == PostModal::Manifests },
                    position: Position::Absolute,
                    left: 29,
                    top: 207,
                    width: 420,
                    height: 37,
                    size: ButtonSize::Compact,
                    font_size: 11,
                    normal_color: ${ control_color(model.manifest_id() == 2, model.modal() == PostModal::Manifests) },
                    pressed_color: ACCENT,
                    text_color: ${ control_text_color(model.manifest_id() == 2, model.modal() == PostModal::Manifests) },
                    border_radius: 7
                ) on Tap { model.load_manifest(2); }
                Text (
                    text: ${ format_args!("{:04}", model.score()) },
                    text_capacity: 5,
                    id: "post_summary_score",
                    visible: ${ model.modal() == PostModal::Summary },
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
                    visible: ${ model.modal() == PostModal::Summary },
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
                    text: ${ format_args!("正确 {} 件", model.delivered()) },
                    text_capacity: 14,
                    id: "post_summary_correct",
                    visible: ${ model.modal() == PostModal::Summary },
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
                    text: ${ format_args!("错投 {} 件", model.missed()) },
                    text_capacity: 14,
                    id: "post_summary_missed",
                    visible: ${ model.modal() == PostModal::Summary },
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
                    visible: ${ model.modal() == PostModal::Summary },
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
                ) on Tap { model.restart(); }
                Button (
                    "换个班次",
                    id: "post_summary_choose",
                    visible: ${ model.modal() == PostModal::Summary },
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
                ) on Tap { model.open_manifests(); }
                Text (
                    "只影响当前内存中的进度。",
                    id: "post_reset_note",
                    visible: ${ model.modal() == PostModal::Reset },
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
                    visible: ${ model.modal() == PostModal::Reset },
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
                ) on Tap { model.restart(); }
                Button (
                    "保留班次",
                    id: "post_reset_cancel",
                    visible: ${ model.modal() == PostModal::Reset },
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
                ) on Tap { model.close_modal(); }
                Text (
                    "当前班次",
                    id: "post_reset_badge",
                    visible: ${ model.modal() == PostModal::Reset },
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
}

pub(super) fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    #[cfg(feature = "std")]
    app.add_plugin(StdInstantClockPlugin);
    app.add_plugin(PlayFontPlugin);
    app.with_widget(surface_view()).with_widget(modal_view());
    let model = app.add_model(PostModel::default());
    app.add_plugin(PostKeyboardPlugin::new(model.clone()));
    app.add_system(post_tick_system::system(model.clone()));
    app.compose(parent, |cx| build_widgets(cx, model));
}
