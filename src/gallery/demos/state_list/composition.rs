use super::state::{PALETTE, ROW_H};
use crate::input::event::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
use crate::prelude::*;
use crate::ui::widgets::{Button, ParagraphStyle, Text};

#[compose]
pub fn build_widgets() {
    let fruits = Signal::new(alloc::vec![
        PALETTE[0].clone(),
        PALETTE[1].clone(),
        PALETTE[2].clone(),
    ]);
    let (dec, inc, label) = (fruits.clone(), fruits.clone(), fruits.clone());
    let rows = fruits.clone();
    let content = fruits.clone();

    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            padding: Padding::all(20),
            row_gap: 10
        ) {
            Text (
                text: ${ alloc::format!("{} FRUITS", label.with(| f | f.len())) },
                width: Dimension::percent(100),
                max_width: 260,
                height: 36,
                font_size: 18,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label()
            )
            Row (
                width: Dimension::percent(100),
                max_width: 180,
                height: 40,
                column_gap: 10
            ) {
                Button (
                    text_color: ColorToken::OnPrimary,
                    grow: 1.0,
                    height: 40,
                    border_radius: 10,
                    normal_color: ColorToken::Error,
                    pressed_color: ColorToken::SurfaceVariant
                ) [
                    Text::label("−"),
                ] on Tap {
                    dec.update(|f| {
                        f.pop();
                    });
                }
                Button (
                    text_color: ColorToken::OnPrimary,
                    grow: 1.0,
                    height: 40,
                    border_radius: 10,
                    normal_color: ColorToken::Primary,
                    pressed_color: ColorToken::Secondary
                ) [
                    Text::label("+"),
                ] on Tap {
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
                border_radius: 12,
                padding: Padding::all(6),
                row_gap: 4
            ) [
                ScrollOffset {
                    x: Fixed::ZERO,
                    y: Fixed::ZERO,
                },
                ScrollConfig {
                    direction: ScrollAxis::Vertical,
                    elastic: true,
                    content_height: Fixed::from_int(3 * ROW_H),
                    content_width: Fixed::ZERO,
                },
            ] {
                walk ${ rows.get() } with fruit {
                    View (
                        bg_color: fruit.color,
                        text_color: ColorToken::OnPrimary,
                        width: Dimension::percent(100),
                        height: ROW_H,
                        border_radius: 8
                    ) [
                        Text::label(fruit.name),
                    ]
                }
            }
        }
    };
    //~focus-end

    let scroll = World::find_by_id(cx.world_mut(), "state_list_scroll")
        .expect("scroll container id registered");
    crate::core::reactive::with_world_scope(cx.world_mut(), || {
        crate::core::reactive::effect_with_widget(scroll, move || {
            let n = content.with(|f| f.len()) as i32;
            crate::core::reactive::with_world(|w| {
                if let Some(cfg) = w.get_mut::<ScrollConfig>(scroll) {
                    cfg.content_height = Fixed::from_int(n * ROW_H);
                }
            });
        });
    });
}
