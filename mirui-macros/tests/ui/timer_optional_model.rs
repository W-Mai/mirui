use mirui_macros::timer;

struct Counter;

#[timer(every = 10)]
fn tick(counter: model!(optional Counter)) {
    let _ = counter;
}

fn main() {}
