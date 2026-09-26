use mirui::model;

struct Counter(u8);

#[model]
impl Counter {
    fn value(&self) -> &u8 {
        &self.0
    }
}

fn main() {}
