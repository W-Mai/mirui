use mirui::components::Text;
use mirui::prelude::*;
use mirui::surface::sdl::SdlSurface;
use mirui::widget::IdMap;
use mirui::widget::dirty::Dirty;

#[derive(Clone, Copy, Default)]
struct ClickCounter {
    single: u32,
    double: u32,
    triple: u32,
    long: u32,
}

fn refresh_label(world: &mut World) {
    let counter = world
        .resource::<ClickCounter>()
        .copied()
        .unwrap_or_default();
    let text = std::format!(
        "single: {}   double: {}   triple: {}   long: {}",
        counter.single,
        counter.double,
        counter.triple,
        counter.long,
    );
    let label = match world.find_by_id("counter_label") {
        Some(e) => e,
        None => return,
    };
    if let Some(t) = world.get_mut::<Text>(label) {
        t.0 = text.into_bytes();
    }
    world.insert(label, Dirty);
}

fn main() {
    let backend = SdlSurface::new("mirui - on handlers", 640, 320);
    let mut app = App::new(backend);
    app.with_default_widgets();
    app.world.insert_resource(IdMap::new());
    app.world.insert_resource(ClickCounter::default());

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            padding: Padding::all(20),
            width: Dimension::px(640),
            height: Dimension::px(320),
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
                "single: 0   double: 0   triple: 0   long: 0",
                id: "counter_label",
                height: 40
            ) {}
            Row (grow: 1.0, justify: JustifyContent::SpaceEvenly, align: AlignItems::Center) {
                View (
                    bg_color: Color::rgb(88, 166, 255),
                    width: 140,
                    height: 100,
                    border_radius: 10
                ) {
                    on Tap { if let Some (c) = __world . resource_mut :: < ClickCounter > () { c . single += 1 ; } refresh_label (__world) ; }
                }
                View (
                    bg_color: Color::rgb(63, 185, 80),
                    width: 140,
                    height: 100,
                    border_radius: 10
                ) {
                    on Tap(2) { if let Some (c) = __world . resource_mut :: < ClickCounter > () { c . double += 1 ; } refresh_label (__world) ; }
                }
                View (
                    bg_color: Color::rgb(248, 81, 73),
                    width: 140,
                    height: 100,
                    border_radius: 10
                ) {
                    on Tap(3) { if let Some (c) = __world . resource_mut :: < ClickCounter > () { c . triple += 1 ; } refresh_label (__world) ; }
                }
                View (
                    bg_color: Color::rgb(210, 168, 255),
                    width: 140,
                    height: 100,
                    border_radius: 10
                ) {
                    on LongPress { if let Some (c) = __world . resource_mut :: < ClickCounter > () { c . long += 1 ; } refresh_label (__world) ; }
                }
            }
        }
    };

    app.set_root(root);
    app.run();
}
