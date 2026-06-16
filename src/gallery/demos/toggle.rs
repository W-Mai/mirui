extern crate alloc;
use alloc::format;

#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::components::{Checkbox, Switch};
use crate::ecs::{Entity, World};
#[cfg(feature = "std")]
use crate::plugins::StdInstantClockPlugin;
use crate::prelude::*;
use crate::state::{Computed, Signal};
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::widget::IdMap;

#[derive(Clone, Copy, Default)]
pub struct ToggleStats {
    pub switch_on: bool,
    pub switch_changes: u32,
    pub checkbox_checked: bool,
    pub checkbox_changes: u32,
}

/// Switch + Checkbox in modifier-chain form, each with `on Toggled` callback.
///
/// # Required plugins
/// - [`StdInstantClockPlugin`] — gesture timing
///
/// # Resources auto-inserted
/// - [`IdMap`] (if absent) — `find_by_id("toggle_label")`
/// - [`ToggleStats`] (if absent) — populated by the handlers
pub fn build_widgets(world: &mut World, parent: Entity) {
    if world.resource::<IdMap>().is_none() {
        world.insert_resource(IdMap::new());
    }

    let stats = Signal::new(ToggleStats::default());
    let (s_stats_r, s_checkbox, s_switch) = (stats.clone(), stats.clone(), stats.clone());
    let stats_text = Computed::new(move || {
        format!(
            "switch: {} ({} flips)   checkbox: {} ({} flips)",
            if s_stats_r.get().switch_on {
                "ON"
            } else {
                "OFF"
            },
            s_stats_r.get().switch_changes,
            if s_stats_r.get().checkbox_checked {
                "[x]"
            } else {
                "[ ]"
            },
            s_stats_r.get().checkbox_changes
        )
    });

    //~focus-start
    ui! {
        :(
            parent: parent
            world: world
        :)

        Column (grow: 1.0, padding: Padding::all(20)) {
            View (
                text: $stats_text,
                height: 30
            )
            Row (grow: 1.0, justify: JustifyContent::SpaceEvenly, align: AlignItems::Center) {
                Switch (width: 60, height: 32) on Toggled {
                    s_switch
                        .update(|s| {
                            s.switch_on = *now;
                            s.switch_changes += 1;
                        });
                }
                Checkbox (width: 32, height: 32) on Toggled {
                    s_checkbox
                        .update(|s| {
                            s.checkbox_checked = *now;
                            s.checkbox_changes += 1;
                        });
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::Children;

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
    }
}
