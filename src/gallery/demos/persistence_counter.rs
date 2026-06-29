extern crate alloc;

#[cfg(feature = "persistence")]
use crate::core::lifecycle::PersistencePlugin;
use crate::prelude::*;

pub fn build_widgets(world: &mut World, parent: Entity, count: Signal<i32>) {
    let (dec, inc, label) = (count.clone(), count.clone(), count);

    //~focus-start
    ui! {
        :(
            parent: parent
            world: world
        :)

        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            padding: Padding::all(16)
        ) {
            View (text: ${ alloc::format!("Count: {}", label.get()) }, height: 40)
            Row (padding: Padding::all(8)) {
                View (
                    bg_color: Color::rgb(220, 80, 80),
                    width: 60,
                    height: 40,
                    border_radius: 8,
                    text: "-"
                ) on Tap {
                    dec.update(|n| *n -= 1);
                }
                View (
                    bg_color: Color::rgb(63, 185, 80),
                    width: 60,
                    height: 40,
                    border_radius: 8,
                    text: "+"
                ) on Tap {
                    inc.update(|n| *n += 1);
                }
            }
            View (
                height: 32,
                text: "Counter persists across reloads / restarts"
            )
        }
    };
    //~focus-end
}

#[cfg(all(feature = "std", feature = "persistence"))]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::app::plugins::StdInstantClockPlugin;
    app.add_plugin(StdInstantClockPlugin);

    // PersistencePlugin must register before widgets spawn so the
    // Registry sits in the World when on_pause / on_quit fire.
    let count = Signal::new(0i32);
    let plugin = PersistencePlugin::new(pick_storage())
        .signal("count", count.clone())
        .autosave_every_ms(2000);
    app.add_plugin(plugin);

    build_widgets(&mut app.world, parent, count);
}

#[cfg(all(
    feature = "std",
    feature = "persistence",
    target_arch = "wasm32",
    feature = "web-canvas",
))]
fn pick_storage() -> crate::core::storage::LocalStorageStorage {
    crate::core::storage::LocalStorageStorage::with_prefix("mirui_gallery")
        .expect("localStorage available in supported browsers")
}

// Desktop / std non-wasm: persist to a temp file so the counter
// survives reruns without leaving permanent state on the user's
// machine. A real app would pick a stable config directory instead.
#[cfg(all(feature = "std", feature = "persistence", not(target_arch = "wasm32")))]
fn pick_storage() -> crate::core::storage::FileStorage {
    let mut path = std::env::temp_dir();
    path.push("mirui_persistence_counter.bin");
    crate::core::storage::FileStorage::open(path)
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
        let count = Signal::new(0i32);
        build_widgets(&mut world, parent, count);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty())
        );
    }
}
