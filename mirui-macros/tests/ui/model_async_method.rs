use mirui::model;

struct Counter(u8);

#[model]
impl Counter {
    async fn increment(&mut self) {
        self.0 += 1;
    }
}

fn main() {}
