use mirui::system;

struct Clock;
struct Settings;

#[system]
fn duplicate(value: res!(Clock), value: res!(Settings)) {
    let _ = value;
}

fn main() {}
