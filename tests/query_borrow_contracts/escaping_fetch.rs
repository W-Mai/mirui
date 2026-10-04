use mirui::{component, system};

#[component]
struct Position;

struct Target(mirui::ecs::Entity);

struct Retained(Option<&'static Position>);

#[system]
fn retain(target: res!(Target), rows: query!(Position), retained: res!(mut Retained)) {
    retained.0 = rows.get(target.0);
}

fn main() {}
