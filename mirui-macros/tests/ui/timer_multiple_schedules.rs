use mirui_macros::timer;

#[timer(after = 10, every = 20)]
fn tick() {}

fn main() {}
