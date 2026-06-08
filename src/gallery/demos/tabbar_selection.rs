use super::attach_to_parent;
#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::components::{TabBar, Text};
use crate::ecs::{Entity, World};
#[cfg(feature = "std")]
use crate::plugins::StdInstantClockPlugin;
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::widget::IdMap;
use crate::widget::dirty::Dirty;
use alloc::format;

#[derive(Clone, Copy, Default)]
pub struct SelectionStats {
    pub last_new: u8,
    pub last_old: u8,
    pub changes: u32,
}

/// 3-tab TabBar with `on SelectionChanged` callback.
///
/// # Required plugins
/// - [`StdInstantClockPlugin`] — gesture timing
///
/// # Resources auto-inserted
/// - [`IdMap`] (if absent) — `find_by_id("selection_label")`
/// - [`SelectionStats`] (if absent) — populated by the handler
pub fn build_widgets(world: &mut World, parent: Entity) -> Entity {
    if world.resource::<IdMap>().is_none() {
        world.insert_resource(IdMap::new());
    }
    if world.resource::<SelectionStats>().is_none() {
        world.insert_resource(SelectionStats::default());
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
                "selected: 0 (was 0, 0 changes)",
                id: "selection_label",
                height: 30
            ) {}
            TabBar (width: 440, height: 44)
                on SelectionChanged {
                    if let Some(s) = ctx.world.resource_mut::<SelectionStats>() {
                        s.last_new = *new;
                        s.last_old = *old;
                        s.changes += 1;
                    }
                    refresh_label(ctx.world);
                } {}
        }
    };

    let bars = world.query::<TabBar>().collect();
    if let Some(&bar) = bars.first()
        && let Some(tb) = world.get_mut::<TabBar>(bar)
    {
        tb.count = 3;
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
    let stats = world
        .resource::<SelectionStats>()
        .copied()
        .unwrap_or_default();
    let text = format!(
        "selected: {} (was {}, {} changes)",
        stats.last_new, stats.last_old, stats.changes,
    );
    let label = match world.find_by_id("selection_label") {
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
        assert!(world.resource::<SelectionStats>().is_some());
    }
}
