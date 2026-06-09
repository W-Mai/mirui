extern crate alloc;

#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::components::Text;
use crate::components::{LazyList, LazyListBinder, LazyListPool};
use crate::ecs::{Entity, World};
use crate::event::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
#[cfg(feature = "std")]
use crate::plugins::StdInstantClockPlugin;
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;

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
    let list = ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            bg_color: Color::rgb(28, 28, 40),
            width: 320,
            height: 320
        ) [
            LazyList::new(ITEM_COUNT, ROW_H, POOL_SIZE as u8),
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
                    width: 320,
                    height: ROW_H
                ) {}
            }
        }
    };

    let pool: alloc::vec::Vec<Entity> = world
        .get::<crate::widget::Children>(list)
        .map(|c| c.0.clone())
        .unwrap_or_default();
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
    use crate::widget::Children;
    use crate::widget::IdMap;
    use crate::widget::builder::WidgetBuilder;

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
