use mirui::prelude::*;

struct Counter(u32);

#[compose]
fn borrowed_resource() -> &'static Counter {
    res!(Counter).with(|counter| &*counter)
}

fn main() {}
