use super::binding::{ITEM_COUNT, ROW_H, create_row};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::input::event::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
use crate::prelude::*;
use crate::ui::widgets::{LazyList, Text};

#[compose]
fn compose_list() -> Entity {
    ui! {
        LazyList (
            id: "lazy_list_view",
            bg_color: ColorToken::SurfaceVariant,
            border_color: ColorToken::Outline,
            border_width: 1,
            width: Dimension::percent(100),
            max_width: 420,
            grow: 1.0,
            border_radius: 12,
            item_count: ITEM_COUNT,
            item_extent: Fixed::from_int(ROW_H)
        ) [
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
            ScrollConfig {
                direction: ScrollAxis::Vertical,
                elastic: true,
                ..Default::default()
            },
        ] on CreateCell(create_row::callback())
    }
}

#[compose]
pub fn build_widgets() {
    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            padding: Padding::all(16),
            row_gap: 10,
            bg_color: ColorToken::Surface
        ) {
            Column (
                width: Dimension::percent(100),
                max_width: 420,
                height: 38,
                row_gap: 2
            ) {
                Text (
                    "VIRTUAL LIST",
                    width: Dimension::percent(100),
                    height: 22,
                    font_size: 18,
                    text_color: ColorToken::OnSurface
                )
                Text (
                    "65,536 ROWS",
                    width: Dimension::percent(100),
                    height: 14,
                    font_size: 10,
                    text_color: ColorToken::OnSurfaceVariant
                )
            }
            compose_list ()
        }
    };
    //~focus-end
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_plugin(StdInstantClockPlugin);
    app.compose(parent, build_widgets());
}
