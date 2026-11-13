use super::{Session, strips::RhythmStrip};
use crate::prelude::*;
use crate::ui::IgnoreHitTest;
use crate::ui::widgets::{Button, ParagraphStyle, Text};

fn compact(width: Fixed, height: Fixed) -> bool {
    width < Fixed::from_int(256) || height < Fixed::from_int(200)
}

#[compose]
pub(super) fn build_widgets(session: model!(Session)) {
    ui! {
        Column (
            grow: 1.0,
            container: true,
            padding: Padding::all(4),
            row_gap: 2,
            align: AlignItems::Stretch,
            bg_color: ColorToken::Surface
        ) on SizeChanged { session.resize(event.new.width, event.new.height); } {
            View (
                id: "prompter_header", width: Dimension::percent(100), height: 12
            ) [ super::header::Header { model: session.clone() }, IgnoreHitTest ]
            Column(grow:1.0,min_height:62,min_width:0,clip_children:true) {
                Column (
                    id: "prompter_sheet_viewport", grow: 1.0, min_height: 62,
                    min_width: 0, clip_children: true,
                    scroll_offset: ${ crate::input::event::scroll::ScrollOffset {x:session.page_scroll(),y:Fixed::ZERO} }
                ) [
                    crate::input::event::scroll::ScrollOffset::default(),
                    crate::input::event::scroll::ScrollConfig {direction:crate::input::event::scroll::ScrollAxis::Horizontal,elastic:false,..Default::default()},
                ] {
                    Column(id:"prompter_sheet",width:${session.stage_width()*3},height:Dimension::percent(100),shrink:0.0,min_height:0) {
                        View(height:super::strips::CHORD_TRACK_HEIGHT,width:Dimension::percent(100),shrink:0.0) [super::strips::ChordTimeline{model:session.clone()},IgnoreHitTest]
                        Row(width:Dimension::percent(100),grow:1.0,min_height:0,shrink:0.0)
                            on SizeChanged { session.sheet_viewport(session.stage_width(),event.new.height); } {
                            sheet_page(session.clone(),0)
                            sheet_page(session.clone(),1)
                            sheet_page(session.clone(),2)
                        }
                    }
                }
                View(position:Position::Absolute,left:${session.stage_width()/2-Fixed::from_int(3)},top:0,width:6,height:Dimension::percent(100)) [super::strips::TimeNeedle{},IgnoreHitTest]
                View(position:Position::Absolute,left:0,top:0,width:Dimension::percent(100),height:Dimension::percent(100),visible:${session.count_in()!=0}) [super::header::CountIn{model:session.clone()},IgnoreHitTest]
            }
            View (height: 10) [
                RhythmStrip {
                    model: session.clone(),
                    strums: false,
                },
                IgnoreHitTest,
            ]
            View (height: 18) [
                RhythmStrip {
                    model: session.clone(),
                    strums: true,
                },
                IgnoreHitTest,
            ]
            Row (
                id: "prompter_controls",
                height: 20,
                column_gap: 2,
                visible: @(width, height) { !compact(width, height) }
            ) {
                    Text (
                        text: ${ session.tempo() },
                        width: 50,
                        font_size: 12,
                        paragraph: ParagraphStyle::label()
                    )
                    Button (
                        id: "prompter_play",
                        grow: 1.0,
                        padding: Padding::all(1),
                        normal_color: ColorToken::Primary,
                        border_radius: 0
                    ) {
                        Text (
                            text: ${ if session.playing() { "PAUSE" } else { "PLAY" } },
                            grow: 1.0,
                            font_size: 12,
                            text_color: ColorToken::OnPrimary,
                            paragraph: ParagraphStyle::label()
                        ) [
                            IgnoreHitTest,
                        ]
                    } on Tap(super::runtime::toggle::callback(session.clone()))
                    Button (
                        width: 28,
                        padding: Padding::all(1),
                        normal_color: ColorToken::SurfaceVariant,
                        border_radius: 0
                    ) {
                        Text (
                            "STOP",
                            font_size: 12,
                            grow: 1.0,
                            paragraph: ParagraphStyle::label()
                        ) [
                            IgnoreHitTest,
                        ]
                    } on Tap { session.stop(); }
                    Button (
                        width: 14,
                        padding: Padding::all(1),
                        normal_color: ColorToken::SurfaceVariant,
                        border_radius: 0
                    ) {
                        Text (
                            "-",
                            font_size: 12,
                            grow: 1.0,
                            paragraph: ParagraphStyle::label()
                        ) [
                            IgnoreHitTest,
                        ]
                    } on Tap { super::runtime::report(session.adjust_tempo(-4)); }
                    Button (
                        width: 14,
                        padding: Padding::all(1),
                        normal_color: ColorToken::SurfaceVariant,
                        border_radius: 0
                    ) {
                        Text (
                            "+",
                            font_size: 12,
                            grow: 1.0,
                            paragraph: ParagraphStyle::label()
                        ) [
                            IgnoreHitTest,
                        ]
                    } on Tap { super::runtime::report(session.adjust_tempo(4)); }
            }
        }
    };
}

#[compose]
fn sheet_page(session: model!(Session), page: u8) -> Entity {
    ui! {
        Column (
            width: ${ session.stage_width() },
            height: Dimension::percent(100),
            shrink: 0.0,
            min_height: 0,
            min_width: 0,
            clip_children: true,
            visible: ${ page != 2 || session.has_next() },
            scroll_offset: ${
                crate::input::event::scroll::ScrollOffset {
                    x: Fixed::ZERO,
                    y: if page == 0 {
                        session.previous_scroll()
                    } else if page == 1 {
                        session.sheet_scroll()
                    } else {
                        Fixed::ZERO
                    },
                }
            }
        ) [
            crate::input::event::scroll::ScrollOffset::default(),
            crate::input::event::scroll::ScrollConfig {
                elastic: false,
                ..Default::default()
            },
        ] {
            View (
                width: Dimension::percent(100),
                height: ${
                    if page == 0 {
                        session.previous_content_height()
                    } else if page == 1 {
                        session.content_height()
                    } else {
                        session.next_content_height()
                    }
                },
                shrink: 0.0
            ) [
                super::sheet::SheetView {
                    model: session.clone(),
                    page,
                },
                IgnoreHitTest,
            ]
        }
    }
}
