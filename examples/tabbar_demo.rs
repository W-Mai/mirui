extern crate alloc;

use mirui::anim::Tween;
use mirui::app::App;
use mirui::components::tabbar::TabBar;
use mirui::ecs::{Entity, World};
use mirui::layout::*;
use mirui::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};
use mirui::surface::sdl::SdlSurface;
use mirui::types::{Color, Dimension, Fixed};
use mirui::widget::builder::WidgetBuilder;
use mirui::widget::dirty::Dirty;
use mirui_macros::ui;

// Animate the indicator slide between selected tabs. The handler in
// widget_input snaps `indicator_offset` immediately; this animate!
// component overwrites it with a Tween value each frame so the slide
// looks smooth instead of teleporting.
mirui_macros::animate!(AnimateTabIndicator, |world, entity, value| {
    if let Some(tb) = world.get_mut::<TabBar>(entity) {
        tb.indicator_offset = value;
    }
    world.insert(entity, Dirty);
});

struct LastSelected(u8);

fn tabbar_observer(world: &mut World, entity: Entity) {
    let Some(tb) = world.get::<TabBar>(entity) else {
        return;
    };
    let current = tb.selected;
    let from = world
        .get::<LastSelected>(entity)
        .map(|s| s.0)
        .unwrap_or(current);
    if from == current {
        return;
    }
    let from_offset = Fixed::from_int(from as i32);
    let to_offset = Fixed::from_int(current as i32);
    world.insert(entity, LastSelected(current));
    world.insert(
        entity,
        AnimateTabIndicator(Tween::ease_to(from_offset, to_offset, 220).into()),
    );
}

fn observer_system(world: &mut World) {
    let entities: alloc::vec::Vec<_> = world.query::<TabBar>().collect();
    for e in entities {
        tabbar_observer(world, e);
    }
}

fn main() {
    let backend = SdlSurface::new("TabBar Demo", 480, 320);
    let mut app = App::new(backend);

    app.add_system(mirui::anim::sync_delta_time_ms);
    app.add_system(AnimateTabIndicator::system());
    app.add_system(observer_system);

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(20, 20, 30))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(480),
            height: Dimension::px(320),
            ..Default::default()
        })
        .id();

    let tabs = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        tabbar (
            bg_color: Color::rgb(40, 40, 56),
            width: 480,
            height: 40
        ) {
            tab0 (
                text: "Home",
                text_color: Color::rgb(220, 220, 230),
                grow: 1.0,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) {}
            tab1 (
                text: "Search",
                text_color: Color::rgb(180, 180, 190),
                grow: 1.0,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) {}
            tab2 (
                text: "Profile",
                text_color: Color::rgb(180, 180, 190),
                grow: 1.0,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) {}
        }
    };

    app.world.insert(
        tabs,
        TabBar::new(3).with_indicator(Color::rgb(88, 166, 255), 3),
    );

    app.set_root(root);
    app.add_plugin(StdInstantClockPlugin::default())
        .add_plugin(FpsSummaryPlugin::default());
    app.run();
}
