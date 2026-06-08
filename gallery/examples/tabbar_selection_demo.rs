extern crate alloc;

use mirui::components::{TabBar, Text};
use mirui::plugins::StdInstantClockPlugin;
use mirui::prelude::*;
use mirui::surface::sdl::SdlSurface;
use mirui::widget::IdMap;
use mirui::widget::dirty::Dirty;

#[derive(Clone, Copy, Default)]
struct SelectionStats {
    last_new: u8,
    last_old: u8,
    changes: u32,
}

fn refresh_label(world: &mut World) {
    let stats = world
        .resource::<SelectionStats>()
        .copied()
        .unwrap_or_default();
    let text = std::format!(
        "selected: {} (was {}, {} changes)",
        stats.last_new,
        stats.last_old,
        stats.changes,
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

fn main() {
    let backend = SdlSurface::new("mirui - tabbar selection events", 480, 200);
    let mut app = App::new(backend);
    app.add_plugin(StdInstantClockPlugin::default());
    app.with_default_widgets().with_default_systems();
    app.world.insert_resource(IdMap::new());
    app.world.insert_resource(SelectionStats::default());

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            padding: Padding::all(20),
            width: Dimension::px(480),
            height: Dimension::px(200),
            ..Default::default()
        })
        .id();

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        Column (grow: 1.0) {
            Text (
                "selected: 0 (was 0, 0 changes)",
                id: "selection_label",
                height: 30
            ) {}
            TabBar (width: 440, height: 44)
                on SelectionChanged {
                    ctx.world
                        .resource_mut::<SelectionStats>()
                        .map(|s| {
                            s.last_new = *new;
                            s.last_old = *old;
                            s.changes += 1;
                        });
                    refresh_label(ctx.world);
                } {}
        }
    };

    let bars = app.world.query::<TabBar>().collect();
    if let Some(&bar) = bars.first()
        && let Some(tb) = app.world.get_mut::<TabBar>(bar)
    {
        tb.count = 3;
    }

    app.set_root(root);
    app.run();
}
