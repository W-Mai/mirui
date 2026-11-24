use super::model::{Studio, transition_config};
use super::style::*;
use crate::prelude::*;
use crate::ui::widgets::{Button, FollowBounds, Text};

#[event]
fn select(
    target: entity!(),
    frame: Entity,
    controls: model!(Studio),
    frames: query!(mut FollowBounds),
) {
    if let Some(bounds) = frames.get(frame) {
        bounds.target = target;
        bounds.transition = Some(transition_config(controls.motion(), controls.preset()));
    }
}

#[compose]
pub(super) fn bounds_panel(controls: model!(Studio)) -> Entity {
    ui! {
        Column (
            height: 118,
            padding: Padding::all(12),
            row_gap: 4,
            bg_color: CARD,
            border_radius: 16,
            clip_children: true
        ) {
            Text ("03 / FOLLOW BOUNDS", height: 20, font_size: 11, text_color: MUTED)
            tiles (controls, id("pages_follow_frame"))
            View (
                id: "pages_follow_frame",
                border_width: 2,
                border_color: INK,
                border_radius: 12
            ) [
                FollowBounds::new(id("pages_follow_a"))
                    .with_outset(4)
                    .with_transition(transition_config(controls.motion(), controls.preset())),
            ]
        }
    }
}

#[compose]
fn tiles(controls: model!(Studio), frame: Entity) -> Entity {
    let tile = style!(pressed_color: LINE, text_color: INK, font_size: 14, border_radius: 8);
    ui! {
        Row (
            grow: 1.0,
            column_gap: 16,
            align: AlignItems::Center,
            justify: JustifyContent::Center
        ) {
            Button (
                id: "pages_follow_a",
                width: 64,
                height: 32,
                normal_color: MINT,
                text: "01",
                style: tile.clone()
            ) on Tap(select::callback(frame, controls))
            Button (
                id: "pages_follow_b",
                width: 92,
                height: 52,
                normal_color: colors(1).0,
                text: "02",
                style: tile.clone()
            ) on Tap(select::callback(frame, controls))
            Button (
                id: "pages_follow_c",
                width: 54,
                height: 40,
                normal_color: colors(2).0,
                text: "03",
                style: tile
            ) on Tap(select::callback(frame, controls))
        }
    }
}
