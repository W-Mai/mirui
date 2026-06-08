use super::attach_to_parent;
#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::components::{Checkbox, Switch, Text};
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
pub fn build_widgets(world: &mut World, parent: Entity) -> Entity {
    if world.resource::<IdMap>().is_none() {
        world.insert_resource(IdMap::new());
    }
    if world.resource::<ToggleStats>().is_none() {
        world.insert_resource(ToggleStats::default());
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
                "switch: OFF (0 flips)   checkbox: [ ] (0 flips)",
                id: "toggle_label",
                height: 30
            ) {}
            Row (grow: 1.0, justify: JustifyContent::SpaceEvenly, align: AlignItems::Center) {
                Switch (width: 60, height: 32)
                    on Toggled {
                        if let Some(s) = ctx.world.resource_mut::<ToggleStats>() {
                            s.switch_on = *now;
                            s.switch_changes += 1;
                        }
                        refresh_label(ctx.world);
                    } {}
                Checkbox (width: 32, height: 32)
                    on Toggled {
                        if let Some(s) = ctx.world.resource_mut::<ToggleStats>() {
                            s.checkbox_checked = *now;
                            s.checkbox_changes += 1;
                        }
                        refresh_label(ctx.world);
                    } {}
            }
        }
    };

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
    let stats = world.resource::<ToggleStats>().copied().unwrap_or_default();
    let text = format!(
        "switch: {} ({} flips)   checkbox: {} ({} flips)",
        if stats.switch_on { "ON" } else { "OFF" },
        stats.switch_changes,
        if stats.checkbox_checked { "[x]" } else { "[ ]" },
        stats.checkbox_changes,
    );
    let label = match world.find_by_id("toggle_label") {
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
        assert!(world.resource::<ToggleStats>().is_some());
    }
}
