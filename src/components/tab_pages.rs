use crate::anim::Tween;
use crate::components::tabbar::TabBar;
use crate::ecs::{Entity, World};
use crate::types::Fixed;
use crate::widget::Hidden;
use crate::widget::dirty::Dirty;
use alloc::vec::Vec;

/// Marker placed on each tab's content widget. The `tab_pages_system`
/// keeps `Hidden` synced: only the entity whose `index` matches the
/// referenced `TabBar`'s `selected` is rendered.
pub struct TabContent {
    pub tab_bar: Entity,
    pub index: u8,
}

struct TabBarPrev {
    selected: u8,
}

struct TabIndicatorTween {
    tween: Tween,
}

const INDICATOR_TWEEN_MS: u16 = 220;

pub fn tab_pages_system(world: &mut World) {
    drive_indicator_tweens(world);
    detect_selection_changes(world);
    apply_visibility(world);
}

fn drive_indicator_tweens(world: &mut World) {
    let dt = world
        .resource::<crate::ecs::DeltaTimeMs>()
        .map_or(16, |d| d.0);
    let entities: Vec<Entity> = world.query::<TabIndicatorTween>().collect();
    for e in entities {
        let (value, done) = {
            let Some(t) = world.get_mut::<TabIndicatorTween>(e) else {
                continue;
            };
            t.tween.tick(dt);
            (t.tween.value(), t.tween.is_finished())
        };
        if let Some(tb) = world.get_mut::<TabBar>(e) {
            tb.indicator_offset = value;
        }
        world.insert(e, Dirty);
        if done {
            world.remove::<TabIndicatorTween>(e);
        }
    }
}

fn detect_selection_changes(world: &mut World) {
    let bars: Vec<Entity> = world.query::<TabBar>().collect();
    for bar in bars {
        let current = match world.get::<TabBar>(bar) {
            Some(tb) => tb.selected,
            None => continue,
        };
        let previous = world.get::<TabBarPrev>(bar).map(|p| p.selected);
        match previous {
            Some(prev) if prev == current => continue,
            Some(prev) => {
                let from = Fixed::from_int(prev as i32);
                let to = Fixed::from_int(current as i32);
                world.insert(
                    bar,
                    TabIndicatorTween {
                        tween: Tween::ease_to(from, to, INDICATOR_TWEEN_MS),
                    },
                );
            }
            None => {
                if let Some(tb) = world.get_mut::<TabBar>(bar) {
                    tb.indicator_offset = Fixed::from_int(current as i32);
                }
            }
        }
        world.insert(bar, TabBarPrev { selected: current });
    }
}

fn apply_visibility(world: &mut World) {
    let entities: Vec<Entity> = world.query::<TabContent>().collect();
    for e in entities {
        let Some(tc) = world.get::<TabContent>(e) else {
            continue;
        };
        let bar = tc.tab_bar;
        let index = tc.index;
        let bar_selected = match world.get::<TabBar>(bar) {
            Some(tb) => tb.selected,
            None => continue,
        };
        let want_hidden = index != bar_selected;
        let is_hidden = world.get::<Hidden>(e).is_some();
        match (want_hidden, is_hidden) {
            (true, false) => {
                world.insert(e, Hidden);
                world.insert(e, Dirty);
            }
            (false, true) => {
                world.remove::<Hidden>(e);
                world.insert(e, Dirty);
            }
            _ => {}
        }
    }
}

const SYSTEMS: &[crate::ecs::System] = &[crate::ecs::System::new(
    "tab_pages",
    crate::ecs::run_order::TAB_PAGES,
    tab_pages_system,
)];

pub fn view() -> crate::widget::view::View {
    crate::widget::view::View::systems_only("TabPages", SYSTEMS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::World;

    fn make_bar(world: &mut World, count: u8) -> Entity {
        let e = world.spawn();
        world.insert(e, TabBar::new(count));
        e
    }

    fn make_content(world: &mut World, bar: Entity, index: u8) -> Entity {
        let e = world.spawn();
        world.insert(
            e,
            TabContent {
                tab_bar: bar,
                index,
            },
        );
        e
    }

    #[test]
    fn first_tick_seeds_prev_and_jumps_indicator() {
        let mut world = World::default();
        let bar = make_bar(&mut world, 3);
        if let Some(tb) = world.get_mut::<TabBar>(bar) {
            tb.selected = 2;
        }
        tab_pages_system(&mut world);
        let tb = world.get::<TabBar>(bar).unwrap();
        assert_eq!(tb.indicator_offset, Fixed::from_int(2));
        assert!(world.get::<TabIndicatorTween>(bar).is_none());
    }

    #[test]
    fn selection_change_starts_tween() {
        let mut world = World::default();
        let bar = make_bar(&mut world, 3);
        tab_pages_system(&mut world);
        if let Some(tb) = world.get_mut::<TabBar>(bar) {
            tb.selected = 1;
        }
        tab_pages_system(&mut world);
        assert!(world.get::<TabIndicatorTween>(bar).is_some());
    }

    #[test]
    fn content_visibility_tracks_selected() {
        let mut world = World::default();
        let bar = make_bar(&mut world, 3);
        let p0 = make_content(&mut world, bar, 0);
        let p1 = make_content(&mut world, bar, 1);
        let p2 = make_content(&mut world, bar, 2);

        tab_pages_system(&mut world);
        assert!(
            world.get::<Hidden>(p0).is_none(),
            "selected=0 should show p0"
        );
        assert!(world.get::<Hidden>(p1).is_some());
        assert!(world.get::<Hidden>(p2).is_some());

        if let Some(tb) = world.get_mut::<TabBar>(bar) {
            tb.selected = 2;
        }
        tab_pages_system(&mut world);
        assert!(world.get::<Hidden>(p0).is_some());
        assert!(world.get::<Hidden>(p1).is_some());
        assert!(world.get::<Hidden>(p2).is_none());
    }
}
