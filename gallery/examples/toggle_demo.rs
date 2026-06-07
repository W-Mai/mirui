use mirui::components::{Checkbox, Switch, Text};
use mirui::plugins::StdInstantClockPlugin;
use mirui::prelude::*;
use mirui::surface::sdl::SdlSurface;
use mirui::widget::IdMap;
use mirui::widget::dirty::Dirty;

#[derive(Clone, Copy, Default)]
struct ToggleStats {
    switch_on: bool,
    switch_changes: u32,
    checkbox_checked: bool,
    checkbox_changes: u32,
}

fn refresh_label(world: &mut World) {
    let stats = world.resource::<ToggleStats>().copied().unwrap_or_default();
    let text = std::format!(
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

fn main() {
    let backend = SdlSurface::new("mirui - toggle business events", 480, 240);
    let mut app = App::new(backend);
    app.add_plugin(StdInstantClockPlugin);
    app.with_default_widgets();
    app.world.insert_resource(IdMap::new());
    app.world.insert_resource(ToggleStats::default());

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            padding: Padding::all(20),
            width: Dimension::px(480),
            height: Dimension::px(240),
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
                "switch: OFF (0 flips)   checkbox: [ ] (0 flips)",
                id: "toggle_label",
                height: 30
            ) {}
            Row (grow: 1.0, justify: JustifyContent::SpaceEvenly, align: AlignItems::Center) {
                Switch (width: 60, height: 32)
                    on Toggled {
                        __world
                            .resource_mut::<ToggleStats>()
                            .map(|s| {
                                s.switch_on = *now;
                                s.switch_changes += 1;
                            });
                        refresh_label(__world);
                    } {}
                Checkbox (width: 32, height: 32)
                    on Toggled {
                        __world
                            .resource_mut::<ToggleStats>()
                            .map(|s| {
                                s.checkbox_checked = *now;
                                s.checkbox_changes += 1;
                            });
                        refresh_label(__world);
                    } {}
            }
        }
    };

    app.set_root(root);
    app.run();
}
