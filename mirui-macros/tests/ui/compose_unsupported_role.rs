use mirui::compose;

struct Time;

#[compose]
fn panel(time: res!(Time)) {
    let _ = time;
}

fn main() {}
