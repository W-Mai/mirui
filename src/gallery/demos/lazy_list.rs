extern crate alloc;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::input::event::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
use crate::prelude::*;
use crate::ui::widgets::{LazyList, LazyListBinder, LazyListPool, Text};

const ROW_H: i32 = 32;
const POOL_SIZE: usize = 12;
const ITEM_COUNT: u32 = 1 << 16;

fn row_binder(world: &mut World, entity: Entity, index: u32) {
    let label = alloc::format!("Row {index}");
    if let Some(t) = world.get_mut::<Text>(entity) {
        t.0 = label.into_bytes();
    } else {
        world.insert(entity, Text(label.into_bytes()));
    }
}

pub fn build_widgets(world: &mut World, parent: Entity) {
    //~focus-start
    let list = ui! {
        :(
            parent: parent
            world: world
        :)

        LazyList (
            bg_color: Color::rgb(28, 28, 40),
            grow: 1.0,
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
                    bg_color: Color::rgb(40, 40, 56),
                    text_color: Color::rgb(220, 220, 230),
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    height: ROW_H
                )
            }
        }
    };
    //~focus-end

    let pool: alloc::vec::Vec<Entity> = world
        .get::<crate::ui::Children>(list)
        .map(|c| c.0.clone())
        .unwrap_or_default();
    // Absolute children resolve Auto width to 0; force Percent so rows track list width.
    for &row in &pool {
        if let Some(style) = world.get_mut::<Style>(row) {
            style.layout.width = Dimension::percent(100);
        }
    }
    world.insert(list, LazyListPool::new(pool));
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_plugin(StdInstantClockPlugin);
    build_widgets(&mut app.world, parent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Children;
    use crate::ui::IdMap;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        build_widgets(&mut world, parent);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
    }
}
