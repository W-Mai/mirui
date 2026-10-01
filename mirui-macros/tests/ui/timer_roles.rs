use mirui::app::App;
use mirui::ecs::Component;
use mirui_macros::{model, timer};

struct Marker;

impl Component for Marker {}

#[model]
struct Counter {
    value: u32,
}

#[model]
impl Counter {
    fn advance(&mut self) {
        self.value += 1;
    }
}

#[timer(after = 10)]
fn once(timer: entity!(), commands: commands!()) {
    commands.insert(timer, Marker);
}

#[timer(every = 20)]
fn periodic(counter: model!(Counter), timer: entity!(), commands: commands!()) {
    counter.advance();
    commands.remove::<Marker>(timer);
}

#[timer(repeat = (3, 30))]
fn bounded_count() {}

#[timer(until = (120, 40))]
fn bounded_deadline() {}

fn main() {
    let mut app = App::headless(32, 32);
    let counter = app.add_model(Counter { value: 0 });
    app.add_timer(once::timer());
    app.add_timer(periodic::timer(counter));
    app.add_timer(bounded_count::timer());
    app.add_timer(bounded_deadline::timer());
}
