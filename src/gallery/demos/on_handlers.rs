#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, World};
#[cfg(feature = "std")]
use crate::plugins::StdInstantClockPlugin;
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::ui::IdMap;
use crate::ui::dirty::Dirty;
use crate::ui::widgets::Text;
use alloc::format;

#[derive(Clone, Copy, Default)]
pub struct ClickCounter {
    pub single: u32,
    pub double: u32,
    pub triple: u32,
    pub long: u32,
}

/// Single / double / triple tap and long-press counters with a live label.
///
/// # Required plugins
/// - [`StdInstantClockPlugin`] — multi-tap window depends on the clock
///
/// # Resources auto-inserted
/// - [`IdMap`] (if absent) — for `find_by_id("counter_label")`
/// - [`ClickCounter`] (if absent) — populated by the handlers
pub fn build_widgets(world: &mut World, parent: Entity) {
    if world.resource::<IdMap>().is_none() {
        world.insert_resource(IdMap::new());
    }
    if world.resource::<ClickCounter>().is_none() {
        world.insert_resource(ClickCounter::default());
    }

    //~focus-start
    ui! {
        :(
            parent: parent
            world: world
        :)

        Column (grow: 1.0, padding: Padding::all(20)) {
            Text (
                "single: 0   double: 0   triple: 0   long: 0",
                id: "counter_label",
                height: 40
            )
            Row (grow: 1.0, justify: JustifyContent::SpaceEvenly, align: AlignItems::Center) {
                View (
                    bg_color: Color::rgb(88, 166, 255),
                    width: 140,
                    height: 100,
                    border_radius: 10
                ) on Tap {
                    if let Some(c) = ctx.world.resource_mut::<ClickCounter>() {
                        c.single += 1;
                    }
                    refresh_label(ctx.world);
                }
                View (
                    bg_color: Color::rgb(63, 185, 80),
                    width: 140,
                    height: 100,
                    border_radius: 10
                ) on Tap(2) {
                    if let Some(c) = ctx.world.resource_mut::<ClickCounter>() {
                        c.double += 1;
                    }
                    refresh_label(ctx.world);
                }
                View (
                    bg_color: Color::rgb(248, 81, 73),
                    width: 140,
                    height: 100,
                    border_radius: 10
                ) on Tap(3) {
                    if let Some(c) = ctx.world.resource_mut::<ClickCounter>() {
                        c.triple += 1;
                    }
                    refresh_label(ctx.world);
                }
                View (
                    bg_color: Color::rgb(210, 168, 255),
                    width: 140,
                    height: 100,
                    border_radius: 10
                ) on LongPress {
                    if let Some(c) = ctx.world.resource_mut::<ClickCounter>() {
                        c.long += 1;
                    }
                    refresh_label(ctx.world);
                }
            }
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
    build_widgets(&mut app.world, parent);
}

fn refresh_label(world: &mut World) {
    let counter = world
        .resource::<ClickCounter>()
        .copied()
        .unwrap_or_default();
    let text = format!(
        "single: {}   double: {}   triple: {}   long: {}",
        counter.single, counter.double, counter.triple, counter.long,
    );
    let label = match world.find_by_id("counter_label") {
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
    use crate::ui::Children;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        let parent = WidgetBuilder::new(&mut world).id();
        build_widgets(&mut world, parent);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty())
        );
        assert!(world.resource::<ClickCounter>().is_some());
    }
}
