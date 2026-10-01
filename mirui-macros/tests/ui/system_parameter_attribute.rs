use mirui::system;

struct Clock;

#[system]
fn attributed(#[allow(unused)] clock: res!(Clock)) {
    let _ = clock;
}

fn main() {}
