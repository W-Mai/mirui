#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::prelude::*;
use crate::ui::IdMap;
use crate::ui::widgets::{Slider, Text};

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
pub fn build_widgets(world: &mut World, parent: Entity) {
    if world.resource::<IdMap>().is_none() {
        world.insert_resource(IdMap::new());
    }
    if world.resource::<Stats>().is_none() {
        world.insert_resource(Stats::default());
    }

    let stats = Signal::new(Stats::default());
    let (s_read, s_value, s_drag_started, s_drag_ended) =
        (stats.clone(), stats.clone(), stats.clone(), stats.clone());

    //~focus-start
    ui! {
        :(
            parent: parent
            world: world
        :)

        Column (grow: 1.0, padding: Padding::all(20)) {
            Text (
                ${
                    format!(
                        "value: {}   changes: {}   drags started/ended: {}", s_read.get().last_value,
                        s_read.get().changes, s_read.get().drags
                    )
                },
                id: "stats_label",
                height: 30
            )
            Slider (width: 600, height: 24) on ValueChanged {
                let new_value = new.to_int();
                let _ = old;
                s_value
                    .update(|s| {
                        s.last_value = new_value;
                        s.changes += 1;
                    });
            } on DragStarted {
                s_drag_started
                    .update(|s| {
                        s.drags += 1;
                    });
            } on DragEnded {
                s_drag_ended
                    .update(|s| {
                        s.drags += 1;
                    });
            }
        }
    };
    //~focus-end

    let sliders = world.query::<Slider>().collect();
    if let Some(&slider_entity) = sliders.first()
        && let Some(s) = world.get_mut::<Slider>(slider_entity)
    {
        s.min = Fixed::ZERO;
        s.max = Fixed::from_int(100);
        s.value = Fixed::ZERO;
    }
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
        assert!(world.resource::<Stats>().is_some());

        // The reactive Text's first run must seed real content at build time,
        // not leave the label empty until the first event.
        let label = world.find_by_id("stats_label").expect("stats_label exists");
        let text = world.get::<Text>(label).expect("label has Text");
        assert!(
            !text.bytes(&world).is_empty(),
            "reactive Text shows its initial value"
        );
    }
}
