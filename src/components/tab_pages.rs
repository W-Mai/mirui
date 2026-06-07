use crate::anim::Tween;
use crate::components::tabbar::{INDICATOR_TWEEN_MS, TabBar, TabBarPrev, TabIndicatorTween};
use crate::ecs::{Entity, World};
use crate::types::Fixed;
use crate::widget::dirty::Dirty;
use crate::widget::{Hidden, Parent};
use alloc::vec::Vec;

/// Marker placed on each tab's content widget. The `tab_pages_system`
/// keeps `Hidden` synced: only the entity whose `index` matches the
/// referenced `TabBar`'s `selected` is rendered.
pub struct TabContent {
    pub tab_bar: Entity,
    pub index: u8,
}

#[crate::system(order = TAB_PAGES, expect = TabBar)]
pub fn tab_pages_system(world: &mut World) {
    drive_indicator_tweens(world);
    detect_programmatic_selection_changes(world);
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

/// Catches programmatic `TabBar.selected` mutations and starts the
/// indicator tween. `tabbar_handler` syncs `TabBarPrev` itself, so tap
/// path doesn't double-fire here.
fn detect_programmatic_selection_changes(world: &mut World) {
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
                // The walker skips Hidden, so a Dirty here would never
                // get swept; clear the subtree's leftover markers and
                // push Dirty up to the parent whose rect still covers
                // the freshly-hidden area.
                crate::widget::dirty::clear_subtree_dirty(world, e);
                if let Some(parent) = world.get::<Parent>(e).map(|p| p.0) {
                    world.insert(parent, Dirty);
                }
            }
            (false, true) => {
                world.remove::<Hidden>(e);
                // Mark the whole subtree, not just `e`. While `e` was
                // Hidden any global event (theme swap, viewport
                // resize) that walked from the root via
                // `mark_subtree_dirty` skipped the subtree, leaving
                // descendants and any cached offscreen buffers with
                // stale data. Marking the subtree on unhide bumps
                // every `OffscreenGeneration` inside.
                crate::widget::dirty::mark_subtree_dirty(world, e);
            }
            _ => {}
        }
    }
}

pub fn view() -> crate::widget::view::View {
    crate::widget::view::View::systems_only("TabPages", const { &[tab_pages_system::system()] })
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

    /// Switching tabs after a theme swap that happened while the
    /// destination tab was hidden must leave every descendant of the
    /// freshly-revealed tab Dirty, not only the tab content entity
    /// itself. Otherwise `OffscreenRender` (mirror, blur cache, ...)
    /// descendants keep their stale cached buffers.
    #[test]
    fn unhide_marks_whole_subtree_dirty() {
        use crate::widget::Children;
        use crate::widget::dirty::mark_subtree_dirty;

        let mut world = World::default();
        let bar = make_bar(&mut world, 2);
        let p0 = make_content(&mut world, bar, 0);
        let p1 = make_content(&mut world, bar, 1);
        // Give p1 a child so we can verify subtree-wide Dirty marking.
        let p1_child = world.spawn();
        world.insert(p1_child, crate::widget::Parent(p1));
        world.insert(p1, Children(alloc::vec![p1_child]));

        // Initial selection = 0 → p1 + p1_child get Hidden.
        tab_pages_system(&mut world);
        assert!(world.get::<Hidden>(p1).is_some());

        // Theme swap (or any global event) walks from a wider root.
        // `mark_subtree_dirty` skips Hidden, so neither p1 nor
        // p1_child receives Dirty here. Sanity-check that.
        mark_subtree_dirty(&mut world, p1);
        assert!(world.get::<Dirty>(p1).is_none());
        assert!(world.get::<Dirty>(p1_child).is_none());

        // User switches to tab 1.
        if let Some(tb) = world.get_mut::<TabBar>(bar) {
            tb.selected = 1;
        }
        tab_pages_system(&mut world);

        // After unhide both the tab content and its descendant must
        // be Dirty so a downstream `OffscreenRender` cache inside
        // misses on the next render.
        assert!(
            world.get::<Hidden>(p1).is_none(),
            "tab content should be unhidden"
        );
        assert!(
            world.get::<Dirty>(p1).is_some(),
            "tab content itself must be Dirty after unhide"
        );
        assert!(
            world.get::<Dirty>(p1_child).is_some(),
            "tab content's descendants must be Dirty after unhide so \
             cached offscreen buffers inside invalidate"
        );

        // p0 went the other way and should be Hidden + clean.
        assert!(world.get::<Hidden>(p0).is_some());
    }
}
