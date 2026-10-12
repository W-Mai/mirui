use mirui::input::event::scroll::{ScrollConfig, ScrollOffset};
use mirui::prelude::*;
use mirui::ui::widgets::LazyList;

#[model]
struct Count {
    changes: u32,
}

#[model]
impl Count {
    fn changed(&mut self, event: &ScrollChanged) {
        assert_eq!(event.new.y - event.old.y, Fixed::ONE / 4);
        self.changes += 1;
    }

    fn changes(&self) -> u32 {
        self.changes
    }
}

#[event]
fn count_changed(event: event!(ScrollChanged), count: model!(Count)) {
    count.changed(event);
}

#[event]
fn create_cell(_event: event!(mirui::ui::widgets::lazy_list::CreateCell)) -> Entity {
    ui! {
        View (height: 10)
    }
}

#[compose]
fn lists(block: model!(Count), closure: model!(Count), named: model!(Count)) {
    ui! {
        Column (width: 60, height: 100) {
            Column (width: 60, height: 30) [
                ScrollOffset::default(),
                ScrollConfig::default(),
            ] on ScrollChanged { block.changed(event); }
            View (width: 60, height: 30) [
                ScrollOffset::default(),
                ScrollConfig::default(),
            ] on ScrollChanged(|event: event!(ScrollChanged)| {
                closure.changed(event);
            })
            LazyList (width: 60, height: 30, items: 100_u32, item_extent: 10) on CreateCell(create_cell::callback()) on ScrollChanged(count_changed::callback(named))
        }
    };
}

struct Pending(bool);

#[system]
fn move_content(offsets: query!(mut ScrollOffset), pending: res!(mut Pending)) {
    if core::mem::take(&mut pending.0) {
        for offset in offsets {
            offset.y += Fixed::ONE / 4;
        }
    }
}

#[test]
fn all_callback_forms_observe_system_writes_on_containers_and_lazy_lists() {
    let mut app = App::headless(80, 120);
    app.with_default_widgets();
    app.add_resource(Pending(false));
    let counters =
        core::array::from_fn::<_, 3, _>(|_| app.add_model(Count { changes: 0 }).finish());
    let root = app.spawn_root().id();
    app.compose(
        root,
        lists(
            counters[0].clone(),
            counters[1].clone(),
            counters[2].clone(),
        ),
    );
    app.add_system(move_content::system());
    app.render().unwrap();
    app.world.resource_mut::<Pending>().unwrap().0 = true;
    app.tick();
    assert!(counters.iter().all(|count| count.changes() == 1));
    app.tick();
    assert!(counters.iter().all(|count| count.changes() == 1));
}
