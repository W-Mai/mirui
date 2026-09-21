extern crate alloc;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::input::event::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
use crate::prelude::*;
use crate::ui::widgets::{LazyList, LazyListBinder, LazyListPool, ParagraphStyle, Text, TextAlign};

const ROW_H: i32 = 38;
const POOL_SIZE: usize = 12;
const ITEM_COUNT: u32 = 1 << 16;

fn row_binder(world: &mut World, entity: Entity, index: u32) {
    let label = alloc::format!("Row {index}");
    let Some(label_entity) = world
        .get::<crate::ui::Children>(entity)
        .and_then(|children| children.0.first().copied())
    else {
        return;
    };
    if let Some(t) = world.get_mut::<Text>(label_entity) {
        t.set_content(label);
        world.invalidate_visual(label_entity);
    }
    if let Some(style) = world.get_mut::<Style>(entity) {
        style.set_bg_color(if index % 2 == 0 {
            ColorToken::Surface
        } else {
            ColorToken::SurfaceVariant
        });
        world.invalidate_visual(entity);
    }
}

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

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(320, 320);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Children;
    use crate::ui::IdMap;
    use crate::ui::UiScope;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
        drop(cx);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
        let list = world.find_by_id("lazy_list_view").unwrap();
        let row = world.get::<LazyListPool>(list).unwrap().items[0];
        assert!(!world.has::<Text>(row));
        let label = world.get::<Children>(row).unwrap().0[0];
        let paragraph = world.get::<Text>(label).unwrap().paragraph().clone();
        row_binder(&mut world, row, 42);
        assert_eq!(world.get::<Text>(label).unwrap().resolve(&world), "Row 42");
        assert_eq!(world.get::<Text>(label).unwrap().paragraph(), &paragraph);
        assert_eq!(
            world.get::<Style>(row).unwrap().bg_color,
            Some(ColorToken::Surface.into())
        );
    }
}
