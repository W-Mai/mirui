extern crate alloc;

#[cfg(feature = "persistence")]
use crate::core::persistence::PersistencePlugin;
use crate::prelude::*;
use crate::ui::widgets::{Button, ParagraphStyle, Text};

#[compose]
pub fn build_widgets(count: Signal<i32>) {
    let (dec, inc, label) = (count.clone(), count.clone(), count);

    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            padding: Padding::all(20),
            row_gap: 14
        ) {
            Text (
                text: ${ alloc::format!("SAVED  {}", label.get()) },
                width: Dimension::percent(100),
                max_width: 260,
                height: 56,
                font_size: 26,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label()
            )
            Row (
                width: Dimension::percent(100),
                max_width: 180,
                height: 44,
                column_gap: 12
            ) {
                Button (
                    grow: 1.0,
                    height: 44,
                    border_radius: 12,
                    normal_color: ColorToken::Error,
                    pressed_color: ColorToken::SurfaceVariant,
                    text_color: ColorToken::OnPrimary
                ) [
                    Text::label("−"),
                ] on Tap { dec.update(|n| *n -= 1); }
                Button (
                    grow: 1.0,
                    height: 44,
                    border_radius: 12,
                    normal_color: ColorToken::Primary,
                    pressed_color: ColorToken::Secondary,
                    text_color: ColorToken::OnPrimary
                ) [
                    Text::label("+"),
                ] on Tap { inc.update(|n| *n += 1); }
            }
            Text (
                "Persists across reloads and restarts",
                width: Dimension::percent(100),
                max_width: 280,
                height: 34,
                text_color: ColorToken::OnSurfaceVariant,
                paragraph: ParagraphStyle::label()
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
    // Registry sits in the World when on_suspend / on_quit fire.
    let count = Signal::new(0i32);
    let plugin = PersistencePlugin::new(pick_storage())
        .signal("count", count.clone())
        .autosave_every_ms(2000);
    app.add_plugin(plugin);

    app.compose(parent, |cx| build_widgets(cx, count));
}

#[cfg(all(
    feature = "std",
    feature = "persistence",
    target_arch = "wasm32",
    feature = "web-canvas",
))]
fn pick_storage() -> crate::core::storage::StorageHandle {
    use crate::core::storage::{LocalStorageStorage, MemoryStorage, Storage};
    match LocalStorageStorage::with_prefix("mirui_gallery") {
        Some(s) => s.into_handle(),
        // Private mode rejects localStorage; degrade to in-process only.
        None => MemoryStorage::new().into_handle(),
    }
}

// Desktop / std non-wasm: persist to a temp file so the counter
// survives reruns without leaving permanent state on the user's
// machine. A real app would pick a stable config directory instead.
#[cfg(all(feature = "std", feature = "persistence", not(target_arch = "wasm32")))]
fn pick_storage() -> crate::core::storage::StorageHandle {
    use crate::core::storage::Storage;
    let mut path = std::env::temp_dir();
    path.push("mirui_persistence_counter.bin");
    crate::core::storage::FileStorage::open(path).into_handle()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Children;
    use crate::ui::IdMap;
    use crate::ui::UiScope;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let count = Signal::new(0i32);
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx, count);
        drop(cx);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty())
        );
    }
}
