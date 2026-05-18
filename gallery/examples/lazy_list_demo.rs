extern crate alloc;

use mirui::app::App;
use mirui::components::lazy_list::{LazyList, LazyListBinder, LazyListPool, lazy_list_system};
use mirui::components::text::Text;
use mirui::ecs::{Entity, World};
use mirui::event::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
use mirui::layout::*;
use mirui::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};
use mirui::surface::sdl::SdlSurface;
use mirui::types::{Color, Dimension, Fixed};
use mirui::widget::builder::WidgetBuilder;
use mirui_macros::ui;

const ROW_H: i32 = 32;
const POOL_SIZE: usize = 12;
const ITEM_COUNT: u32 = 200;

fn row_binder(world: &mut World, entity: Entity, index: u32) {
    let label = alloc::format!("Row {index}");
    if let Some(t) = world.get_mut::<Text>(entity) {
        t.0 = label.into_bytes();
    } else {
        world.insert(entity, Text(label.into_bytes()));
    }
}

fn main() {
    let backend = SdlSurface::new("LazyList Demo", 320, 320);
    let mut app = App::new(backend).with_default_widgets();

    app.add_system(mirui::ecs::System::new(
        "lazy_list",
        mirui::ecs::run_order::LAZY_LIST,
        lazy_list_system,
    ));

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(20, 20, 30))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(320),
            height: Dimension::px(320),
            ..Default::default()
        })
        .id();

    let list = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        list (
            bg_color: Color::rgb(28, 28, 40),
            width: 320,
            height: 320
        ) [
            LazyList::new(ITEM_COUNT, ROW_H, POOL_SIZE as u8),
            LazyListBinder { bind: row_binder },
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
            ScrollConfig {
                direction: ScrollAxis::Vertical,
                elastic: true,
                content_height: Fixed::from_int(ROW_H * ITEM_COUNT as i32),
                content_width: Fixed::ZERO,
            },
        ] {
            walk 0..POOL_SIZE with _i {
                row (
                    bg_color: Color::rgb(40, 40, 56),
                    text_color: Color::rgb(220, 220, 230),
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: 320,
                    height: ROW_H
                ) {}
            }
        }
    };

    // The pool's per-slot identity is whatever ECS assigned; pull the
    // child list off the freshly-built tree so LazyListBinder knows
    // which entities to bind into.
    let pool: alloc::vec::Vec<Entity> = app
        .world
        .get::<mirui::widget::Children>(list)
        .map(|c| c.0.clone())
        .unwrap_or_default();
    app.world.insert(list, LazyListPool::new(pool));

    app.set_root(root);
    app.add_plugin(StdInstantClockPlugin::default())
        .add_plugin(FpsSummaryPlugin::default());
    app.run();
}
