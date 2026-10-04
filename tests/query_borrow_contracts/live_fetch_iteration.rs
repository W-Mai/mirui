use mirui::{component, system};

#[component]
struct Position(i32);

struct Target(mirui::ecs::Entity);

#[system]
fn update(target: res!(Target), rows: query!(mut Position)) {
    let first = rows.get(target.0).unwrap();
    for position in rows {
        first.0 += position.0;
    }
}

fn main() {}
