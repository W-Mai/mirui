use super::attach_to_parent;
#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::components::{Slider, Text};
use crate::ecs::{Entity, World};
#[cfg(feature = "std")]
use crate::plugins::StdInstantClockPlugin;
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::types::Fixed;
use crate::widget::IdMap;
use crate::widget::dirty::Dirty;
use alloc::format;

#[derive(Clone, Copy, Default)]
pub struct Stats {
    pub last_value: i32,
    pub changes: u32,
    pub drags: u32,
}

/// Slider 0..100 with `on ValueChanged / DragStarted / DragEnded` callbacks.
///
/// # Required plugins
/// - [`StdInstantClockPlugin`] — gesture timing
///
/// # Resources auto-inserted
/// - [`IdMap`] (if absent) — `find_by_id("stats_label")`
/// - [`Stats`] (if absent) — populated by the handlers
pub fn build_widgets(world: &mut World, parent: Entity) -> Entity {
    if world.resource::<IdMap>().is_none() {
        world.insert_resource(IdMap::new());
    }
    if world.resource::<Stats>().is_none() {
        world.insert_resource(Stats::default());
    }

    let root = WidgetBuilder::new(world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            padding: Padding::all(20),
            ..Default::default()
        })
        .id();

    ui! {
        :(
            parent: root
            world: world
        :)

        Column (grow: 1.0) {
            Text (
                "value: 0   changes: 0   drags started/ended: 0",
                id: "stats_label",
                height: 30
            ) {}
            Slider (width: 600, height: 24)
                on ValueChanged {
                    let new_value = new.to_int();
                    let _ = old;
                    if let Some(s) = ctx.world.resource_mut::<Stats>() {
                        s.last_value = new_value;
                        s.changes += 1;
                    }
                    refresh_label(ctx.world);
                }
                on DragStarted {
                    if let Some(s) = ctx.world.resource_mut::<Stats>() {
                        s.drags += 1;
                    }
                    refresh_label(ctx.world);
                }
                on DragEnded {
                    if let Some(s) = ctx.world.resource_mut::<Stats>() {
                        s.drags += 1;
                    }
                    refresh_label(ctx.world);
                } {}
        }
    };

    let sliders = world.query::<Slider>().collect();
    if let Some(&slider_entity) = sliders.first()
        && let Some(s) = world.get_mut::<Slider>(slider_entity)
    {
        s.min = Fixed::ZERO;
        s.max = Fixed::from_int(100);
        s.value = Fixed::ZERO;
    }

    attach_to_parent(world, parent, root);
    root
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity) -> Entity
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_plugin(StdInstantClockPlugin);
    build_widgets(&mut app.world, parent)
}

fn refresh_label(world: &mut World) {
    let stats = world.resource::<Stats>().copied().unwrap_or_default();
    let text = format!(
        "value: {}   changes: {}   drags started/ended: {}",
        stats.last_value, stats.changes, stats.drags,
    );
    let label = match world.find_by_id("stats_label") {
        Some(e) => e,
        None => return,
    };
    if let Some(t) = world.get_mut::<Text>(label) {
        t.0 = text.into_bytes();
    }
    world.insert(label, Dirty);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::Children;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        let parent = WidgetBuilder::new(&mut world).id();
        let root = build_widgets(&mut world, parent);
        assert_ne!(root, parent);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| c.0.contains(&root))
        );
        assert!(world.resource::<Stats>().is_some());
    }
}
