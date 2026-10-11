use mirui::input::event::scroll::{ScrollBounds, ScrollConfig, ScrollOffset};
use mirui::prelude::*;
use mirui::render::DrawCommand;
use mirui::ui::widgets::LazyList;
use std::cell::Cell;
use std::rc::Rc;

#[component]
struct Readout;

struct Observed(Rc<Cell<Option<ScrollBounds>>>);

#[view(component = Readout)]
fn readout(
    entity: entity!(),
    bounds: query!(ScrollBounds),
    observed: res!(Observed),
    paint: paint!(),
) {
    let bounds = bounds.get(entity).copied();
    observed.0.set(bounds);
    if let Some(bounds) = bounds {
        paint.draw(&DrawCommand::Fill {
            area: paint.rect(),
            transform: paint.transform(),
            quad: paint.quad(),
            color: Color::rgb(bounds.max_y.to_int() as u8, 0, 0),
            radius: Fixed::ZERO,
            opa: 255,
        });
    }
}

#[event]
fn read_on_tap(entity: entity!(), bounds: query!(ScrollBounds), observed: res!(Observed)) {
    observed.0.set(bounds.get(entity).copied());
}

#[compose]
fn panel() -> Entity {
    ui! {
        View(width: 40, height: 40) [
            Readout, ScrollOffset::default(),
            ScrollConfig { content_height: 80.into(), ..ScrollConfig::default() },
        ] on Tap(read_on_tap::callback())
    }
}

#[test]
fn published_geometry_is_readable_from_view_and_event_queries() {
    let mut app = App::headless(80, 80);
    app.with_default_widgets().with_widget(readout::view());
    let observed = Rc::new(Cell::new(None));
    app.add_resource(Observed(observed.clone()));
    let root = app.spawn_root().id();
    let node = app.compose(root, panel());
    app.render().unwrap();
    assert_eq!(observed.get().unwrap().max_y, 40.into());
    observed.set(None);
    app.world
        .get_mut::<ScrollConfig>(node)
        .unwrap()
        .content_height = 120.into();
    app.render_dirty().unwrap();
    assert_eq!(observed.get().unwrap().max_y, 80.into());
    observed.set(None);
    mirui::__macro_support::dispatch_entity_gesture(
        &mut app.world,
        &mirui::input::event::gesture::GestureEvent::Tap {
            target: node,
            x: 5.into(),
            y: 5.into(),
        },
        0,
    )
    .unwrap();
    assert_eq!(observed.get().unwrap().max_y, 80.into());
}

#[compose]
fn list() -> Entity {
    ui! {
        LazyList(width: 80, height: 80, items: 100_u32, item_extent: 20)
            on CreateCell {
                let item = event.item.clone();
                ui! { View(height: 20, bg_color: ${
                    if item.index() % 2 == 0 { Color::rgb(0, 0, 255) } else { Color::rgb(0, 255, 0) }
                }) }
            }
    }
}

#[test]
fn lazy_list_bounds_cover_logical_items_instead_of_retained_cells() {
    let mut app = App::headless(80, 80);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let list = app.compose(root, list());
    app.render().unwrap();
    assert_eq!(
        app.world.get::<ScrollBounds>(list).unwrap().content_height,
        2000.into()
    );
    app.world.get_mut::<LazyList>(list).unwrap().items = 3_u32.into();
    app.render_dirty().unwrap();
    assert_eq!(
        app.world.get::<ScrollBounds>(list).unwrap().max_y,
        Fixed::ZERO
    );
}
