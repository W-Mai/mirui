use super::binding::{ITEM_COUNT, POOL_SIZE, ROW_H, row_binder};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::input::event::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
use crate::prelude::*;
use crate::ui::widgets::{LazyList, LazyListBinder, LazyListPool, ParagraphStyle, Text, TextAlign};

#[compose]
fn compose_list() -> Entity {
    let list_entity = ui! {
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
            item_height: Fixed::from_int(ROW_H),
            pool_size: POOL_SIZE as u8
        ) [
            LazyListBinder { bind: row_binder },
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
            ScrollConfig {
                direction: ScrollAxis::Vertical,
                elastic: true,
                content_height: Fixed::from_int(ROW_H * ITEM_COUNT as i32),
                content_width: Fixed::ZERO,
            },
        ] {
            walk 0..POOL_SIZE with _i {
                Row (
                    bg_color: ColorToken::Surface,
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: Dimension::percent(100),
                    height: ROW_H,
                    align: AlignItems::Center,
                    padding: Padding {
                        top: Dimension::px(0),
                        right: Dimension::px(12),
                        bottom: Dimension::px(0),
                        left: Dimension::px(12),
                    }
                ) {
                    Text (
                        "",
                        grow: 1.0,
                        height: 24,
                        text_color: ColorToken::OnSurface,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    View (
                        width: 5,
                        height: 5,
                        bg_color: ColorToken::Primary,
                        border_radius: 3
                    )
                }
            }
        }
    };
    let pool: alloc::vec::Vec<Entity> = cx
        .world_mut()
        .get::<crate::ui::Children>(list_entity)
        .map(|c| c.0.clone())
        .unwrap_or_default();
    cx.world_mut().insert(list_entity, LazyListPool::new(pool));
    list_entity
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
                    "65,536 ROWS / 12 LIVE ENTITIES",
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
    app.compose(parent, build_widgets);
}
