use mirui::prelude::*;

struct Counter(u32);

#[compose]
fn overlapping_access() {
    res!(mut Counter).with(|mut counter| {
        res!().insert(Counter(4));
        counter.0 += 1;
    });
}

fn main() {}
