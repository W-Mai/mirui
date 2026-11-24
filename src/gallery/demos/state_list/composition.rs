use super::state::{PALETTE, ROW_H};
use crate::gallery::sample_style::{
    centered_label, centered_sample, primary_action, secondary_action, stepper_track,
};
use crate::input::event::scroll::{ScrollConfig, ScrollOffset};
use crate::prelude::*;
use crate::ui::widgets::{Button, Text};

#[compose]
pub fn build_widgets() {
    let fruits = Signal::new(alloc::vec![
        PALETTE[0].clone(),
        PALETTE[1].clone(),
        PALETTE[2].clone(),
    ]);
    let (dec, inc, label) = (fruits.clone(), fruits.clone(), fruits.clone());
    let rows = fruits.clone();

    //~focus-start
    ui! {
        Column (
            style: centered_sample(),
            row_gap: 10
        ) {
            Text (
                text: ${ format_args!("{} fruits", label.with(| f | f.len())) },
                capacity: usize::MAX.ilog10() as usize + 1 + " fruits".len(),
                style: centered_label(),
                max_width: 260,
                height: 36,
                font_size: 18,
                text_color: ColorToken::OnSurface
            )
            Row (style: stepper_track(), max_width: 188, height: 48, border_radius: 24) {
                Button (
                    "-",
                    style: secondary_action(),
                    grow: 1.0,
                    height: 40,
                    border_radius: 20
                ) on Tap {
                    dec.update(|f| {
                        f.pop();
                    });
                }
                Button (
                    "+",
                    style: primary_action(),
                    grow: 1.0,
                    height: 40,
                    border_radius: 20
                ) on Tap {
                    inc.update(|f| {
                        let next = PALETTE[f.len() % PALETTE.len()].clone();
                        f.push(next);
                    });
                }
            }
            Column (
                id: "state_list_scroll",
                bg_color: ColorToken::SurfaceVariant,
                grow: 1.0,
                width: Dimension::percent(100),
                max_width: 260,
                max_height: 180,
                clip_children: true,
                border_radius: 18,
                padding: Padding::all(6),
                row_gap: 4
            ) [
                ScrollOffset::default(),
                ScrollConfig::default(),
            ] {
                walk ${ rows.get() } with fruit {
                    Row (
                        bg_color: ColorToken::Outline,
                        width: Dimension::percent(100),
                        height: ROW_H,
                        padding: Padding::all(7),
                        column_gap: 10,
                        align: AlignItems::Center,
                        border_radius: 12
                    ) {
                        View (width: 8, height: 8, border_radius: 4, bg_color: fruit.color)
                        Text (
                            text: fruit.name,
                            grow: 1.0,
                            font_size: 14,
                            text_color: ColorToken::OnSurface
                        )
                    }
                }
            }
        }
    };
    //~focus-end
}
