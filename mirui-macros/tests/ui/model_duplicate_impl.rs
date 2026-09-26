use mirui::model;

#[model]
struct Counter(u8);

#[model]
impl Counter {
    fn increment(&mut self) {
        self.0 += 1;
    }
}

#[model]
impl Counter {
    fn decrement(&mut self) {
        self.0 -= 1;
    }
}

fn main() {}
