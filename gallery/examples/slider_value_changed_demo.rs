use mirui::components::{Slider, Text};
use mirui::plugins::StdInstantClockPlugin;
use mirui::prelude::*;
use mirui::surface::sdl::SdlSurface;
use mirui::widget::IdMap;
use mirui::widget::dirty::Dirty;

#[derive(Clone, Copy, Default)]
struct Stats {
    last_value: i32,
    changes: u32,
    drags: u32,
}

fn refresh_label(world: &mut World) {
    let stats = world.resource::<Stats>().copied().unwrap_or_default();
    let text = std::format!(
        "value: {}   changes: {}   drags started/ended: {}",
        stats.last_value,
        stats.changes,
        stats.drags,
    );
    let label = match world.find_by_id("stats_label") {
        Some(e) => e,
        None => return,
    };
    if let Some(t) = world.get_mut::<Text>(label) {
        t.0 = text.into_bytes();
    }
    world.insert(label, Dirty);
}

fn main() {
    let backend = SdlSurface::new("mirui - slider business events", 640, 200);
    let mut app = App::new(backend);
    app.add_plugin(StdInstantClockPlugin);
    app.with_default_widgets();
    app.world.insert_resource(IdMap::new());
    app.world.insert_resource(Stats::default());

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            padding: Padding::all(20),
            width: Dimension::px(640),
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
                "value: 0   changes: 0   drags started/ended: 0",
                id: "stats_label",
                height: 30
            ) {}
            Slider (
                width: 600,
                height: 24
            )
                on ValueChanged {
                    let new_value = new.to_int();
                    let _ = old;
                    ctx.world
                        .resource_mut::<Stats>()
                        .map(|s| {
                            s.last_value = new_value;
                            s.changes += 1;
                        });
                    refresh_label(ctx.world);
                }
                on DragStarted {
                    ctx.world.resource_mut::<Stats>().map(|s| s.drags += 1);
                    refresh_label(ctx.world);
                }
                on DragEnded {
                    ctx.world.resource_mut::<Stats>().map(|s| s.drags += 1);
                    refresh_label(ctx.world);
                } {}
        }
    };

    let sliders = app.world.query::<Slider>().collect();
    if let Some(&slider_entity) = sliders.first()
        && let Some(s) = app.world.get_mut::<Slider>(slider_entity)
    {
        s.min = mirui::types::Fixed::ZERO;
        s.max = mirui::types::Fixed::from_int(100);
        s.value = mirui::types::Fixed::ZERO;
    }

    app.set_root(root);
    app.run();
}
