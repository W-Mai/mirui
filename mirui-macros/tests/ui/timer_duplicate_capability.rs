use mirui_macros::timer;

#[timer(every = 10)]
fn tick(first: entity!(), second: entity!()) {
    let _ = (first, second);
}

fn main() {}
