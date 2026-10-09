use mirui::prelude::*;

#[event]
fn resized(change: event!(SizeChanged)) -> bool {
    change.old.is_none()
}

fn main() {}
